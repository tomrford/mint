# Layout Files

Layout files define memory blocks and their data fields. Layouts are written in TOML. The data in the layout file helps mint understand the structure of the data and how you want to represent it in the output. Each block represents a contiguous region of memory, typically a single struct stored at a known flash address. The canonical example layout is [`doc/examples/block.toml`](examples/block.toml); `mint header` generates [`doc/examples/blocks.h`](examples/blocks.h) from it.

## Structure

```toml
[mint]              # Global configuration (required)
# ...

[blockname.header]  # Block header (required per block)
# ...

[blockname.data]    # Block data fields (required per block)
# ...
```

---

## Mint Configuration

Global configuration applies to all blocks. The `[mint.checksum]` section defines named checksum configurations that can be referenced by inline checksum fields in block data.

```toml
[mint]
abi = "generic-le"

[mint.checksum.crc32]      # Named checksum config (can define multiple)
polynomial = 0x04C11DB7    # CRC polynomial
start = 0xFFFFFFFF         # Initial CRC value
xor_out = 0xFFFFFFFF       # XOR applied to final CRC
ref_in = true              # Reflect input bytes
ref_out = true             # Reflect output CRC
```

Multiple named checksum configurations can be defined (e.g., `[mint.checksum.crc32]`, `[mint.checksum.crc32c]`). Each is referenced by name in block data fields.

Reusable constants are defined in `[mint.const]`. Const values use the same literal shapes as field `value`: scalar values, strings, booleans, and one-dimensional arrays. The const table is flat; quote names that contain dots.

```toml
[mint.const]
default_voltage = 3.3
fw_name = "BootloaderV2"
ip_octets = [192, 168, 1, 10]
"app.length" = 0x4000
```

Each block also exposes `<block_name>.start_address` and `<block_name>.length` as consts. These promoted values use the block header values.

### ABI profiles

The required `abi` setting selects the layout rules used for every block in the file. The currently supported profiles are:

| ABI | Family | Byte order | Addressable unit |
| --- | --- | --- | --- |
| `generic-le` | natural-width C layout | little-endian | 8 bits |
| `generic-be` | natural-width C layout | big-endian | 8 bits |
| `arm-aapcs32-le` | ARM AAPCS32 | little-endian | 8 bits |
| `tricore-eabi-le` | Infineon TriCore EABI | little-endian | 8 bits |

`generic-le`, `generic-be` and `arm-aapcs32-le` share the same natural-width scalar and aggregate rules. TriCore differs by aligning 64-bit scalars to 4 octets while retaining their 8-octet storage size and array stride. Run `mint abi list` for accepted names or `mint abi show ABI` for the effective scalar table.

The ABI does not select the output container: `--format hex` and `--format mot` remain independent choices. Both currently use standard octet-addressed Intel HEX or Motorola S-record addresses. Target address-unit semantics, C layout and output record addressing are separate contracts.

---

## Block Header

Each block requires a header section defining the memory region.

```toml
[blockname.header]
start_address = 0x8B000    # Start address in memory (required)
length = 0x1000            # Block size in bytes
padding = 0xFF             # Padding byte value (default: 0xFF)
```

The resolved data aggregate must fit within `length` and cannot exceed 256 MiB. Mint materializes block payloads in memory and rejects larger layouts before allocation.

---

## Block Data

Data fields are key-value pairs where the key is a dotted path (matching C struct hierarchy) and the value defines the field.

Every block name and data field path segment must be a valid C identifier matching `[_a-zA-Z][_a-zA-Z0-9]*`, must not be a C11 keyword and must not use an implementation-reserved underscore form. Block names cannot start with `_`; field names cannot start with `__` or an underscore followed by an uppercase letter. Quote other strings only where the layout treats them as values, such as data-source names, bitmap region names, ref targets, const names and checksum config names. Quoted dotted data keys such as `"device.id"` are rejected because they create one flat key; use an unquoted dotted key or nested table instead.

### Aggregate alignment

Each ABI family lays out dotted paths as naturally aligned C aggregates. Every leaf gets its storage size, alignment and array stride from the selected profile. The generic and ARM profiles align exact-width integers to their width, `f32` to 4 octets and `f64` to 8 octets. TriCore uses 4-octet alignment for 64-bit scalars while retaining 8-octet storage and array stride. Each branch aligns to the maximum alignment of its children. Children are laid out recursively in their parsed order, and each branch is padded to a multiple of its alignment before the next sibling. The root `block.data` aggregate receives the same tail padding, so the reserved size matches `sizeof` for the equivalent C struct under this ABI.

All alignment gaps and aggregate tail padding use the block header's configured `padding` byte. Mint does not support packed structs. Use `mint abi show` to inspect the selected profile before matching a generated header to a compiler target.

### C header generation

Generate C11 typedefs directly from the layout:

```bash
mint header layout.toml -o layout.h
mint header layout.toml#config layout.toml#data -o blocks.h
```

Each selected block becomes a `<block>_t` typedef, and dotted paths become inline nested structs. Integer and floating-point fields use `<stdint.h>` storage types, while fixed-point fields use the matching signed or unsigned integer storage type with the Mint type in a comment. Bitmap, checksum, ref and fingerprint fields remain integer members.

Generated headers include C11 `_Static_assert` checks for every field offset and final structure size. The checks compare `sizeof` and `offsetof` through `CHAR_BIT`, so Mint's octet offsets remain valid on targets whose C addressable unit is wider than 8 bits. Compiling the header with the target compiler therefore verifies that its C ABI agrees with Mint's selected profile.

Array dimensions become reusable macros prefixed by the block and full field path. One-dimensional arrays use `_LEN`; two-dimensional arrays use `_ROWS` and `_COLS`. Named bitmap regions use `_SHIFT` and `_MASK` macros; literal reserved regions do not generate macros. Fingerprint fields emit an expected-value `<BLOCK>_<FIELD>_FINGERPRINT` macro.

The layout parser guarantees valid block and field names. Header generation runs the build's static validation for selected blocks, including resolved shape, const, checksum, ref and address-range rules. It also rejects duplicate typedefs and generated names that collide when converted to upper snake case. It renders the complete header before writing the output file.

### Field Attributes

| Attribute     | Description                                                                           |
| ------------- | ------------------------------------------------------------------------------------- |
| `type`        | Data type (required)                                                                  |
| `value`       | Literal value (mutually exclusive with other sources)                             |
| `name`        | Data source lookup key (mutually exclusive with other sources)                    |
| `const`       | Const lookup key from `[mint.const]` or an auto-promoted block header const            |
| `bitmap`      | Bitmap field definitions (see below)                                                  |
| `ref`         | Pointer to another field in the same block (see below)                                |
| `checksum`    | Inline checksum referencing a named config (see below)                                |
| `fingerprint` | `true` for this block or another block name in the same file (see below)              |
| `size`/`SIZE` | Array size (minimum 1 per dimension); `size` pads if data is shorter, `SIZE` errors if data is shorter. |

---

## Field Examples

### Scalar Values

```toml
[block.data]
# Literal numeric
device.id = { value = 0x1234, type = "u32" }

# From data source
version = { name = "Version", type = "u16" }

# Unsigned Q-format fixed-point
gain = { value = 1.5, type = "uq8.8" }

# Signed Q-format fixed-point
offset = { value = -1.25, type = "q7.8" }

# Larger scalar from data source
counter = { name = "Counter", type = "u64" }
```

mint also supports binary Q-format fixed-point storage directly in `type`:

- `qI.F` = signed fixed-point, total width `1 + I + F`
- `uqI.F` = unsigned fixed-point, total width `I + F`
- total width must be exactly 8, 16, 32, or 64 bits
- alignment and byte order follow the implied storage width

Examples:

- `q0.15` = signed 16-bit fixed-point
- `uq0.16` = unsigned 16-bit pure-fraction format
- `q15.16` = signed 32-bit fixed-point
- `uq8.8` = unsigned 16-bit fixed-point

mint encodes fixed-point as `round_ties_even(input * 2^fractional_bits)`. In strict mode, overflow is an error. Without `--strict`, the rounded encoded value is clamped to the storage range.

### Const Values

```toml
[mint.const]
default_voltage = 3.3
fw_name = "BootloaderV2"
ip_octets = [192, 168, 1, 10]

[app.header]
start_address = 0x8000
length = 0x100

[app.data]
voltage = { const = "default_voltage", type = "f32" }
label = { const = "fw_name", type = "u8", size = 16 }
ip = { const = "ip_octets", type = "u8", size = 4 }
base = { const = "app.start_address", type = "u32" }
len = { const = "app.length", type = "u32" }
```

`const` uses the same conversion and size rules as `value`. Scalar consts do not use `size`; string and array consts use a one-dimensional `size` or `SIZE`.

### Strings

Strings use `u8` type with `size` for fixed-length fields.

```toml
[block.data]
# Literal string (padded to size)
message = { value = "Hello", type = "u8", size = 16 }

# From data source
device.name = { name = "DeviceName", type = "u8", size = 16 }
```

### Arrays

```toml
[block.data]
# 1D literal array
ip = { value = [192, 168, 1, 1], type = "u8", size = 4 }

# 1D from data source
coefficients = { name = "Coefficients", type = "f32", size = 4 }

# 2D array
matrix = { name = "Matrix", type = "i16", size = [2, 2] }

# Strict size (error if data source has fewer elements)
matrix = { name = "Matrix", type = "i16", SIZE = [2, 2] }
```

### Bitmaps

Pack multiple values into a single integer.

```toml
[block.data]
flags = { type = "u16", bitmap = [
    { bits = 1, name = "EnableDebug" },   # 1 bit from data source
    { bits = 3, value = 0 },              # 3 bits reserved
    { bits = 4, name = "RegionCode" },    # 4 bits from data source
    { bits = 8, value = 0 },              # 8 bits reserved
] }
```

Bitmap fields are packed LSB-first into the specified type. signedness of fields match the type. Negative values are represented as two's complement. The sum of the bits in the bitmap must match the type size.

Fixed-point types are not valid with `bitmap`.

### Refs (Pointers)

A `ref` entry resolves to the absolute memory address of another field within the same block. The ref target is a dotted path rooted at `block.data` — for example, `device.info.version` refers to `[block.data] device.info.version`. Refs can point to leaf fields or branch nodes (nested structs); a branch ref resolves to the branch's aligned aggregate start.

```toml
[block.data]
# Table data at some offset
table.entries = { name = "TableEntries", type = "u16", size = 32 }
table.count = { name = "TableCount", type = "u16" }

# Pointer to the table (resolves to absolute address of "table")
table_ptr = { ref = "table", type = "u32" }

# Pointer to a specific nested field
count_ptr = { ref = "table.count", type = "u32" }
```

**Ref rules:**

- `ref` is mutually exclusive with every other source
- `type` must be an unsigned integer type (`u16`, `u32`, `u64`)
- fixed-point types are not valid with `ref`
- `size`/`SIZE` cannot be used with `ref`
- The target path must exist within the same block — cross-block refs are not supported
- The resolved address is `start_address + target_offset`
- The target path is validated from the resolved layout before field values are emitted
- The resolved address must fit the ref's `u16`, `u32` or `u64` storage type
- Refs can reference fields defined before or after the ref in the layout (forward and backward refs are both supported)

### ABI fingerprints

A `fingerprint` field stores a deterministic 64-bit identifier for a block's resolved ABI. Use `true` for the containing block or name another block in the same TOML file:

```toml
[config.data]
schema = { fingerprint = true, type = "u64" }

[manifest.data]
config_schema = { fingerprint = "config", type = "u64" }
manifest_schema = { fingerprint = true, type = "u64" }
```

Build and header generation fully validate selected blocks and calculate fingerprints only for the blocks referenced by their fingerprint fields. Fingerprint target blocks have their ABIs resolved and shape-checked, but are not otherwise fully validated unless they are also selected. A named `mint fingerprint layout.toml#block` selector fully validates and fingerprints that block, resolves the ABI shape of its fingerprint targets and does not resolve unrelated siblings. `mint fingerprint layout.toml` fully validates and fingerprints every block in declaration order. Referenced blocks do not need their own fingerprint field. Cross-file fingerprint references are not supported.

The fingerprint covers the effective, nameless ABI: byte order, address-unit width, aggregate shape, offsets, scalar storage sizes, alignments, array strides, scalar and fixed-point types, array dimensions, bitmap widths and ref topology. Ref targets contribute their resolved address-unit offset and target kind rather than their name. The ABI profile name, block names, field names, values, `name`/`value`/`const` source choices, addresses, allocated block length and padding byte value do not contribute.

Fingerprint fields require `type = "u64"` and cannot use `size` or `SIZE`. The marker and referenced fingerprint value are not inputs to the containing block's own fingerprint; the field contributes as a normal `u64` at its resolved position. This keeps self-fingerprints non-recursive and prevents cross-block dependency cycles.

Calculate fingerprints without building data:

```bash
mint fingerprint layout.toml#config  # one bare 16-character value
mint fingerprint layout.toml         # "block fingerprint" lines
```

### Checksums

An inline `checksum` field computes a CRC over all preceding data in the block and places the result at the field's position. The checksum value references a named configuration from `[mint.checksum]`.

```toml
[mint.checksum.crc32]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true

[block.data]
device.id = { value = 0x1234, type = "u32" }
device.name = { name = "DeviceName", type = "u8", size = 16 }
version = { name = "Version", type = "u16" }
checksum = { checksum = "crc32", type = "u32" }
```

The CRC covers all bytes from the start of the block data up to (but not including) the checksum field itself, including any alignment padding inserted between fields. Checksums are resolved after all non-checksum fields are emitted; if a block contains multiple checksum fields, they are resolved in field order, so later checksums include the bytes of earlier checksum fields.

**Checksum rules:**

- `checksum` is mutually exclusive with every other source
- `type` must be `u32` (matching CRC-32 output width)
- fixed-point types are not valid with `checksum`
- `size`/`SIZE` cannot be used with `checksum`
- a checksum must follow at least one data byte
- The referenced config name must exist in `[mint.checksum]`
- For more complex checksum operations (cross-block CRC or non-CRC algorithms), use a dedicated hex post-processing tool

---

## Multiple Blocks

A single layout file can define multiple blocks, each with its own checksum configuration:

```toml
[mint]
abi = "generic-le"

[mint.checksum.crc32]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true

[mint.checksum.crc32c]
polynomial = 0x1EDC6F41
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true

[config.header]
start_address = 0x8000
length = 0x1000

[config.data]
version = { value = 1, type = "u16" }
checksum = { checksum = "crc32", type = "u32" }

[data.header]
start_address = 0x8100
length = 0x100

[data.data]
counter = { name = "Counter", type = "u64" }
message = { value = "Hello", type = "u8", size = 16 }
checksum = { checksum = "crc32c", type = "u32" }
```

Build specific blocks with `file.toml#blockname` syntax:

```bash
mint build layout.toml#config --xlsx data.xlsx --variants Default
```

---

## Format

```toml
[block.header]
start_address = 0x8000
length = 0x100

[block.data]
device.id = { value = 0x1234, type = "u32" }
device.name = { name = "DeviceName", type = "u8", size = 16 }
version = { name = "Version", type = "u16" }
```

Layouts are TOML only. JSON is reserved for data input.
