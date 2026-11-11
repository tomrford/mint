# Layout files

A layout is a TOML file describing one or more blocks. Each block is a contiguous memory region, normally one C struct stored at a known flash address. The canonical example is [`doc/examples/block.toml`](../../../doc/examples/block.toml); `mint header` generates [`doc/examples/blocks.h`](../../../doc/examples/blocks.h) from it.

## File structure

```toml
[mint]                      # required: settings for every block in the file
abi = "generic-le"

[mint.const]                # optional reusable literals
[mint.checksum.crc32]       # optional named CRC configurations
[mint.header]               # optional C header settings

[config.header]             # one header and one data table per block
start_address = 0x8000
length = 0x100

[config.data]
version = { name = "Version", type = "u16" }
```

Layout files must use the `.toml` extension. Unknown keys are rejected everywhere. Blocks keep their declaration order, which is the order a whole-file selector builds, generates and fingerprints them.

Block names and every field path segment must be C identifiers matching `[_a-zA-Z][_a-zA-Z0-9]*` that are not C11 keywords or reserved underscore forms: block names cannot start with `_`, and field names cannot start with `__` or `_` followed by an uppercase letter. Quoted dotted data keys such as `"device.id"` are rejected because they would create one flat field; use unquoted dotted keys or nested tables. Other strings, such as data-source names, ref targets, const names and checksum names, are values and are quoted.

## ABI profiles

The required `abi` selects the layout rules for every block in the file. `mint abi list` prints the accepted names and `mint abi show ABI` prints a profile's scalar storage sizes, alignments, array strides and C types.

| ABI | Byte order | Address unit | Verification |
| --- | --- | --- | --- |
| `generic-le` | little-endian | 8 bits | Continuously checked with GCC (`arm-none-eabi`), little-endian |
| `generic-be` | big-endian | 8 bits | Continuously checked with GCC (`arm-none-eabi`), big-endian |
| `arm-aapcs32-le` | little-endian | 8 bits | Continuously checked with GCC (`arm-none-eabi`), AAPCS soft-float Cortex-M3 |
| `tricore-eabi-le` | little-endian | 8 bits | Manually validated with HighTec TriCore GCC 4.9.4 (HDP-v4.9.4.1), default core and `-mtc162` |
| `riscv-ilp32-le` | little-endian | 8 bits | Continuously checked with GCC (`riscv32-none-elf`), RV32IMAC ILP32 |
| `ti-c28x-eabi` | little-endian | 16 bits | Continuously checked with TI C2000 CGT EABI |

"Continuously checked" means CI compiles headers generated for representative layouts with that compiler and checks scalar storage and alignment, array stride, nested aggregates, field offsets, final size, byte order and `CHAR_BIT`. "Manually validated" records a one-off result for the stated compiler version and flags.

### Aggregate layout

Every leaf takes its storage size, alignment and array stride from the profile. The generic, ARM and RISC-V profiles align exact-width integers and fixed-point types to their width, `f32` to 4 octets and `f64` to 8. TriCore and C28x align 64-bit scalars to 4 octets while keeping 8-octet storage and stride. C28x has a 16-bit `char` and rejects `u8`, `i8` and 8-bit fixed-point types.

Dotted paths form nested aggregates. Fields are placed in declaration order, and a branch sits where its first field appears, with its children in their own declaration order. Each branch aligns to the largest alignment of its children (TriCore and C28x raise any aggregate larger than one octet to at least 2 octets) and is tail-padded to a multiple of its alignment before the next sibling. The root `<block>.data` aggregate is tail-padded the same way, so the reserved size equals `sizeof` of the equivalent C struct. Alignment gaps and tail padding use the block's `padding` byte. Packed structs are not supported.

### Addresses and output

`start_address`, ref targets and literal ref addresses are in the ABI's address units; sizes, offsets and `length` are in octets. The ABI does not choose the output container: `--format hex` and `--format mot` both write standard octet-addressed records. For C28x, mint writes each target word address multiplied by two and requires an even `--record-width`, matching byte-addressed image tools and bootloaders; it does not emit TI's word-addressed HEX dialect.

## Block header

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `start_address` | `u32` | required | Block address in target address units |
| `length` | `u32` | required | Allocated octets |
| `padding` | `u8` | `0xFF` | Fill for alignment gaps, tail padding, unused array and string elements, and the allocated space after the data |

The resolved data must fit in `length` and in mint's 256 MiB in-memory limit. On profiles with address units wider than one octet, `length` and the resolved data size must be multiples of the unit width. The block's octet-addressed output range must fit the 32-bit address space.

## Constants

`[mint.const]` holds reusable literals with the same shapes as a field `value`: scalars, booleans, strings and one-dimensional arrays. The table is flat; quote names that contain dots.

```toml
[mint.const]
default_voltage = 3.3
fw_name = "BootloaderV2"
ip_octets = [192, 168, 1, 10]
```

Every block also exposes `<block>.start_address` and `<block>.length` as consts with its header values. A `[mint.const]` key with one of those names is rejected.

## Fields

Each key in `[<block>.data]` is a field path and each value is an inline table with a `type`, exactly one source key, and `size` or `SIZE` where the source allows it.

| Key | Meaning |
| --- | --- |
| `type` | Required storage type |
| `value` | Literal scalar, boolean, string or one-dimensional array |
| `const` | Name of a const |
| `name` | Data-source lookup key, matched exactly and case-sensitively |
| `bitmap` | Bit fields packed into one integer |
| `ref` | Address of a field in the same block, or a literal address; an array makes a reflist |
| `checksum` | Name of a `[mint.checksum.<name>]` configuration |
| `fingerprint` | `true` for this block's ABI fingerprint, or the name of another block in the file |
| `size` / `SIZE` | Array or string dimensions: `N` or `[rows, cols]`, each at least 1. Shorter data is padded with `size` and rejected with `SIZE`; longer data is always rejected |

### Types

- Integers: `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`, `i64` (signed values are two's complement).
- Floats: `f32`, `f64` (IEEE 754).
- Binary fixed point: `qI.F` is signed with total width `1 + I + F`; `uqI.F` is unsigned with total width `I + F`. The width must be 8, 16, 32 or 64 bits and the field is stored, aligned and byte-ordered like the integer of that width. For example `q0.15` and `uq8.8` are 16-bit and `q15.16` is 32-bit.

Booleans are stored in integer types as 1 or 0.

### Value conversion

Values from every source are converted to the field type when the block is built. A fixed-point value is encoded as `round_ties_even(value * 2^F)`. Non-finite floats are always rejected.

Without `--strict`, conversions are lenient: floats truncate toward zero, out-of-range integers saturate to the type's range, out-of-range fixed-point values clamp to the storage range, and bitmap values saturate to their bit width. With `--strict`, any conversion that changes the value is an error, including inexact integer-to-float and `f64`-to-`f32` conversions.

| Value and type | Default | `--strict` |
| --- | --- | --- |
| `1.5` into `u8` | `1` | error |
| `300` into `u8` | `255` | error |
| `-1` into `u8` | `0` | error |
| `300.5` into `uq8.8` | `0xFFFF` | error |
| `3.3` into `f32` | nearest `f32` | error |

### Values, consts and data-source lookups

| Source | Shape | `size` / `SIZE` |
| --- | --- | --- |
| `value`, `const` | scalar or boolean | not allowed |
| `value`, `const` | string | required, one dimension, type `u8` or `u16` |
| `value`, `const` | array | required, one dimension |
| `name` | scalar | not allowed |
| `name` | string or one-dimensional array | required, `N` |
| `name` | two-dimensional array | required, `[rows, cols]` |

Two-dimensional arrays come only from a data source. A two-dimensional source must have exactly `cols` columns in every row; `size` pads missing rows and `SIZE` rejects them.

Strings are stored as their UTF-8 bytes, one byte per element, zero-extended to the element width in ABI byte order; `size` counts elements, not characters. mint adds no terminator: unused elements take the `padding` byte. On C28x use `type = "u16"`, which stores one byte per 16-bit word.

```toml
[block.data]
device.id = { value = 0x1234, type = "u32" }
gain = { value = 1.5, type = "uq8.8" }
voltage = { const = "default_voltage", type = "f32" }
base = { const = "block.start_address", type = "u32" }
message = { value = "Hello", type = "u8", size = 16 }
device.name = { name = "DeviceName", type = "u8", size = 16 }
coefficients = { name = "Coefficients", type = "f32", size = 4 }
matrix = { name = "Matrix", type = "i16", SIZE = [2, 2] }
```

### Bitmaps

A bitmap packs sub-fields into one integer field, least significant bit first. Each element has `bits` (at least 1) and exactly one of `name` or `value`. The `bits` must add up to the type's width. The type must be an integer type, not fixed point, and cannot take `size`. Sub-field values use the signedness of the field type.

```toml
flags = { type = "u16", bitmap = [
    { bits = 1, name = "EnableDebug" },
    { bits = 3, value = 0 },
    { bits = 4, name = "RegionCode" },
    { bits = 8, value = 0 },
] }
```

### Refs

A ref stores an absolute address in a `u16`, `u32` or `u64` field, and every address must fit that type.

- A string target is a field path in the same block, rooted at `<block>.data`. It can name a leaf or a branch (the branch's aligned start) and can point forwards or backwards. It is stored as `start_address + offset_octets / address_unit_octets`. Cross-block paths are not supported.
- An unsigned integer target is stored as written, already in target address units (on C28x a word address). Use `0` for a null address. mint cannot check, rebase or fingerprint what a nonzero literal points at.
- A scalar ref takes one target and no `size`. A reflist takes an array mixing both kinds and a one-dimensional `size` or `SIZE`. Extra entries are rejected; `size` fills missing entries with zero addresses (not the padding byte) and `SIZE` rejects them.
- `--strict` does not affect refs. The `--export-json` report lists a scalar ref's address and a reflist's supplied addresses, without zero fill.

```toml
table.entries = { name = "TableEntries", type = "u16", size = 32 }
table.count = { name = "TableCount", type = "u16" }
table_ptr = { ref = "table", type = "u32" }
peripheral = { ref = 0x40001000, type = "u32" }
ptrs = { ref = ["table", 0, "table.count", 0x40001000], type = "u32", size = 8 }
```

### Checksums

A checksum field stores a CRC-32 of every byte of the block's data before it, including alignment padding and earlier checksum fields. It must be `u32`, needs at least one byte before it and takes no `size`. Each configuration needs all five keys:

```toml
[mint.checksum.crc32]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF   # initial value
xor_out = 0xFFFFFFFF # applied to the final value
ref_in = true        # reflect each input byte
ref_out = true       # reflect the final value before xor_out

[block.data]
crc = { checksum = "crc32", type = "u32" }
```

Cross-block CRCs and other algorithms belong in a post-processing tool.

### Fingerprint fields

A fingerprint field stores a block's [ABI fingerprint](#abi-fingerprints). It must be `u64` and takes no `size`. `fingerprint = true` names the containing block; a string names another block in the same file, which needs no fingerprint field of its own.

```toml
[config.data]
schema = { fingerprint = true, type = "u64" }

[manifest.data]
config_schema = { fingerprint = "config", type = "u64" }
```

## Validation

`mint build`, `mint header` and `mint fingerprint` resolve and validate every selected block before reading a data source or writing anything. This checks the source and size rules above, field shapes and literal and const values against their types and sizes, that the data fits `length` and the address-unit rules, the output address range, checksum placement and configuration names, and that ref targets exist and their addresses fit. It also calculates the fingerprints the block stores. A block named only as a fingerprint target is resolved for its shape but not otherwise validated.

`mint build` then checks values from the data source while building, and rejects blocks whose output ranges overlap.

## C header

`mint header` writes C11 for the selected blocks without a data source:

- a `<block>_t` typedef per block, with dotted paths as inline nested structs. Integer and float fields use `<stdint.h>` types; fixed-point fields use the integer type of their width with the mint type in a comment. Bitmap, checksum, ref and fingerprint fields are integers; refs are address values, not C pointers;
- `<BLOCK>_START_ADDRESS` and `<BLOCK>_LENGTH`; `<BLOCK>_<PATH>_LEN` for one-dimensional arrays and `_ROWS` and `_COLS` for two-dimensional ones; `<BLOCK>_<PATH>_<NAME>_SHIFT` and `_MASK` for each bitmap sub-field with a `name`; `<BLOCK>_<PATH>_FINGERPRINT` with the expected value of each fingerprint field;
- `_Static_assert` checks of every field offset and each block struct's size through `sizeof`, `offsetof` and `CHAR_BIT`, so compiling the header with the target compiler checks that it agrees with mint.

Macro names are the block name and field path in upper snake case, so `flags` sub-field `EnableDebug` in block `config` becomes `CONFIG_FLAGS_ENABLE_DEBUG_SHIFT`.

The header includes `<limits.h>`, `<stddef.h>` and `<stdint.h>` unless `[mint.header].includes` replaces the list:

```toml
[mint.header]
includes = ['"platform/types.h"', '<limits.h>', '<stddef.h>']
```

Each entry keeps its quotes or angle brackets, names a non-empty header without control characters, backslashes or nested delimiters, and is passed to the compiler unchanged. `includes = []` emits none. The included headers must still provide the fixed-width types, `CHAR_BIT`, `offsetof` and integer constant macros such as `UINT64_C` that the header uses. Includes do not affect binary output or fingerprints.

Beyond [validation](#validation), header generation rejects block or macro names that collide once converted to upper snake case, fields that collide with a generated macro, and blocks from several layouts unless their ABIs share layout rules, byte order and address-unit width and their include lists are identical. Nothing is written unless the whole header renders.

## ABI fingerprints

A fingerprint is a 64-bit value identifying a block's resolved ABI, so firmware can reject data built for a different struct shape. It covers byte order, address-unit width, aggregate shape, offsets, scalar storage sizes, alignments, array strides, scalar and fixed-point types, array dimensions, bitmap widths and ref topology: a path target contributes its address-unit offset, size, alignment and whether it is a branch or a leaf, while a literal target, and each unused reflist slot, contributes only an opaque marker. A fingerprint field contributes as an ordinary `u64`, not its target or value, so a block can store its own fingerprint and blocks can reference each other without cycles.

The ABI name, block and field names, values, the choice between `name`, `value` and `const`, block addresses, `length` and the padding byte do not contribute. Changing any of those keeps the fingerprint; changing the struct shape changes it.
