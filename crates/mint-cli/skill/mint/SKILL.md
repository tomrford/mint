---
name: mint
description: "Guide for working with mint, an embedded development tool that assembles flash memory hex files from TOML layout files and data sources (Excel/JSON). Use this skill whenever a project uses or mentions mint / mint-cli, when you encounter .toml layout files that define memory blocks for firmware or flash, when you need to create or modify flash block definitions, set up mint in a build system or CI pipeline, or work with Excel/JSON data sources for embedded device configuration. Also trigger when you see references to building Intel HEX or Motorola S-Record files from structured layout definitions, or when a user mentions replacing a custom hex-generation script with a declarative tool."
---

# mint

mint builds binary flash images (Intel HEX or Motorola S-Record) from a declarative TOML layout file and an optional data source (Excel workbook or JSON). Each layout describes one or more memory blocks — contiguous regions that map to C structs stored at known flash addresses. mint resolves data values, enforces types, computes CRCs, pads to size, and emits the output file. It can also generate matching C headers and ABI fingerprints without a data source.

## Layout file anatomy

A layout file has three levels: global config, per-block headers, and per-block data fields.

```toml
[mint]                    # Global config (required)
abi = "generic-le"       # Required; discover profiles with `mint abi list`

[mint.checksum.crc32]     # Named CRC config (define as many as needed)
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true

[mint.const]              # Optional reusable literals
default_voltage = 3.3
fw_name = "BootloaderV2"

# --- config_t at 0x8000 ---
[config.header]
start_address = 0x8000    # Required — base address in target address units
length = 0x100            # Required — allocated octets; resolved data must fit
padding = 0xFF            # Array, alignment, and tail fill byte (default: 0xFF)

[config.data]
schema = { fingerprint = true, type = "u64" }
device.id = { value = 0x1234, type = "u32" }
device.name = { name = "DeviceName", type = "u8", size = 16 }
version = { name = "Version", type = "u16" }
gain_q8_8 = { value = 1.5, type = "uq8.8" }
flags = { type = "u16", bitmap = [
    { bits = 1, name = "EnableDebug" },
    { bits = 3, value = 0 },
    { bits = 4, name = "RegionCode" },
    { bits = 8, value = 0 },
] }
coefficients = { name = "Coefficients", type = "f32", size = 4 }
matrix = { name = "Matrix", type = "i16", size = [2, 2] }
voltage = { const = "default_voltage", type = "f32" }
checksum = { checksum = "crc32", type = "u32" }

# --- data_t at 0x8100 ---
[data.header]
start_address = 0x8100
length = 0x100

[data.data]
schema = { fingerprint = true, type = "u64" }
config_schema = { fingerprint = "config", type = "u64" }
counter = { name = "Counter", type = "u64" }
message = { const = "fw_name", type = "u8", size = 16 }
ip = { value = [192, 168, 1, 1], type = "u8", size = 4 }
checksum = { checksum = "crc32", type = "u32" }
```

`mint header layout.toml -o layout.h` generates the matching C typedefs, block address and length macros, array extent macros, named bitmap shift/mask macros, and fingerprint macros. `mint fingerprint layout.toml` prints each block's ABI fingerprint.

Key observations:

- Dotted paths (`device.id`, `device.name`) reproduce the struct nesting.
- `type = "u8", size = 16` generates a `uint8_t` array using a reusable `_LEN` macro.
- The bitmap's total bits (1+3+4+8 = 16) match the `u16` type width.
- `gain_q8_8` stores `1.5` as a Q8.8 fixed-point value in a `uint16_t`-sized slot.
- `device.id` uses `value`, `voltage` uses `const`, and `device.name` uses `name`.
- Checksum is the last field — it covers everything above it in the block.

Block names and every `[myblock.data]` path segment must be valid, non-keyword C identifiers matching `[_a-zA-Z][_a-zA-Z0-9]*` and must avoid C-reserved underscore forms. Block names cannot start with `_`; fields cannot start with `__` or an underscore followed by an uppercase letter. Use unquoted dotted keys or nested tables for nested structs; quoted dotted keys are rejected as flat fields.

Multiple blocks can live in one file. Build specific blocks with `layout.toml#blockname`. The layout remains the source of truth for nested structs, storage types, arrays, and bitmap macros.

### Dotted paths mirror C struct nesting

The key `device.info.version.major` maps to `block.device.info.version.major` in the output — the same hierarchy as nested C structs. This is how mint knows field ordering and grouping.

## Layout schema

Every accepted key in a mint layout file, with types, defaults, and constraints.

### `[mint]` — global configuration (required)

| Key   | Type          | Default      | Description                                      |
| ----- | ------------- | ------------ | ------------------------------------------------ |
| `abi` | Named profile | — (required) | Target layout profile; discover names with `mint abi list` |

### `[mint.header]` — C header generation (optional)

`includes` is an ordered array of strings containing C include delimiters, for example `includes = ['"platform/types.h"', '<project/extra.h>']`. It replaces the complete include list. Omission defaults to `['<limits.h>', '<stddef.h>', '<stdint.h>']`; `[]` emits no includes. Names must be non-empty, without control characters, backslashes or nested delimiters. Paths are emitted unchanged for the C compiler to resolve. Layouts combined into one header must use identical effective lists, including order.

The supplied headers must provide the existing generated type names, `CHAR_BIT`, `offsetof` and any integer constant macros used. This setting leaves binary output and ABI fingerprints unchanged.

### `[mint.checksum.<name>]` — named CRC configurations (optional, repeatable)

Define as many as needed (e.g., `[mint.checksum.crc32]`, `[mint.checksum.crc32c]`). Referenced by name in checksum fields. All fields are required — no inheritance or partial configs.

| Key          | Type   | Default      | Description                  |
| ------------ | ------ | ------------ | ---------------------------- |
| `polynomial` | `u32`  | — (required) | CRC polynomial               |
| `start`      | `u32`  | — (required) | Initial CRC value            |
| `xor_out`    | `u32`  | — (required) | XOR applied to final CRC     |
| `ref_in`     | `bool` | — (required) | Reflect each input byte      |
| `ref_out`    | `bool` | — (required) | Reflect final CRC before XOR |

### `[blockname.header]` — per-block memory region (required per block)

| Key             | Type           | Default      | Description                                   |
| --------------- | -------------- | ------------ | --------------------------------------------- |
| `start_address` | `u32` (hex ok) | — (required) | Base address in target address units          |
| `length`        | `u32` (hex ok) | — (required) | Allocated octets; resolved data must fit      |
| `padding`       | `u8` (hex ok)  | `0xFF`       | Array, alignment, and tail fill byte          |

### `[blockname.data]` — field definitions

Each key is a dotted path representing struct nesting. The value is an inline table with a required `type` and exactly one source.

| Attribute     | Type                              | Description |
| ------------- | --------------------------------- | ----------- |
| `type`        | string                            | Required. `u8`/`u16`/`u32`/`u64`, `i8`/`i16`/`i32`/`i64`, `f32`/`f64`, or fixed-point `qI.F` / `uqI.F` with total width 8/16/32/64 |
| `value`       | scalar, string, or array          | Literal value. Mutually exclusive with other sources. |
| `name`        | string                            | Data source lookup key. Mutually exclusive with other sources. |
| `const`       | string                            | Const lookup from `[mint.const]` or an auto-promoted block header const. Mutually exclusive with other sources. |
| `bitmap`      | array of bitmap fields            | Bitfield packing. Mutually exclusive with other sources. |
| `ref`         | string, unsigned integer or array | Same-block target path or absolute target address; arrays form reflists. Mutually exclusive with other sources. |
| `checksum`    | string                            | Name of a `[mint.checksum.<name>]` config. Mutually exclusive with other sources. |
| `fingerprint` | `true` or string                  | This block's ABI fingerprint, or another block's fingerprint from the same layout. Mutually exclusive with other sources. |
| `size`        | integer or `[rows, cols]`         | Array/string dimensions. Pads if data is shorter. A one-dimensional reflist capacity zero-fills missing addresses. Cannot combine with `SIZE`, scalar `ref`, `checksum`, `fingerprint`, or `bitmap`. |
| `SIZE`        | integer or `[rows, cols]`         | Strict array dimensions. Errors if data is shorter. A reflist uses only a one-dimensional exact capacity. Cannot combine with `size`, scalar `ref`, `checksum`, `fingerprint`, or `bitmap`. |

| Source             | Allowed types       | `size`/`SIZE`              | Notes |
| ------------------ | ------------------- | -------------------------- | ----- |
| `value` (scalar)   | any                 | no                         | Numeric or boolean literal |
| `value` (string)   | `u8`, `u16`         | required                   | One zero-extended UTF-8 byte per scalar element |
| `value` (1D array) | any                 | required                   | Inline array of values |
| `value` (2D array) | —                   | —                          | **Not supported.** 2D arrays must come from a data source. |
| `const` (scalar)   | any                 | no                         | Reusable literal from `[mint.const]` |
| `const` (string)   | `u8`, `u16`         | required                   | Reusable string with one UTF-8 byte per scalar element |
| `const` (1D array) | any                 | required                   | Reusable inline array from `[mint.const]` |
| `name` (scalar)    | any                 | no                         | Single value from data source |
| `name` (1D array)  | any                 | required (`size = N`)      | 1D array from data source |
| `name` (2D array)  | any                 | required (`size = [R, C]`) | 2D array from data source |
| `bitmap`           | integer types only  | no                         | Sum of `bits` must equal type width; fixed-point not allowed |
| scalar `ref`       | `u16`, `u32`, `u64` | no                         | Same-block path or absolute unsigned literal; fixed-point not allowed |
| reflist            | `u16`, `u32`, `u64` | required (`size = N`)      | Mixed path/literal address array; lowercase underfill is zero |
| `checksum`         | `u32` only          | no                         | CRC over all preceding bytes in block; fixed-point not allowed |
| `fingerprint`      | `u64` only          | no                         | Injects a nameless ABI fingerprint for this or another same-file block |

Each `bitmap` element:

| Key     | Type         | Description                                              |
| ------- | ------------ | -------------------------------------------------------- |
| `bits`  | integer (>0) | Number of bits this sub-field occupies                   |
| `name`  | string       | Data source lookup key (mutually exclusive with `value`) |
| `value` | scalar       | Literal value (mutually exclusive with `name`)           |

Fields pack LSB-first. Signed parent types use two's complement for negative sub-field values.

## Information to gather before writing a layout

When setting up mint for a project, these parameters need to be established. If replacing an existing system, many can be inferred from the codebase (struct definitions, linker scripts, existing hex generators). Always confirm with the user before proceeding.

**From the hardware/firmware side:**

- **ABI profile** — select the target's byte order and layout rules. Run `mint abi list` and `mint abi show ABI` to inspect the supported choices.
- **Block addresses and sizes** — from linker script, memory map, or flash layout documentation. Each block needs a `start_address` in target address units and a `length` in octets.
- **Padding byte** — usually `0xFF` (erased flash state) but confirm. Some platforms use `0x00`.
- **CRC algorithm** — if blocks need integrity checks, you need the polynomial, initial value, XOR-out, and reflection settings. Check existing CRC routines or documentation.
- **Struct layout** — C header files defining the structs that live at each flash address. These become the `[block.data]` fields.

**From the data/build side:**

- **Which values are constants vs. configurable** — one-off constants go as `value = ...`; reusable constants go in `[mint.const]` and fields use `const = "..."`; configurable values use `name = "..."` to pull from a data source.
- **Data source format** — Excel workbook (typical for manufacturing/calibration workflows) or JSON (typical for CI pipelines that fetch or generate data).
- **Variant names** — the columns/keys that represent build variants (e.g., Default, Debug, Production). The `--variants` flag controls fallback priority.

After the layout exists, generate the header with `mint header layout.toml -o layout.h` so firmware consumes the layout-owned struct shape. Build with `--stats` to confirm sizes and checksums, and `--strict` to catch lossy conversions early.

## Scalar types

| Type                      | Width      | Notes                              |
| ------------------------- | ---------- | ---------------------------------- |
| `u8`, `u16`, `u32`, `u64` | 1–8 bytes  | Unsigned integers                  |
| `i8`, `i16`, `i32`, `i64` | 1–8 bytes  | Signed integers (two's complement) |
| `f32`, `f64`              | 4, 8 bytes | IEEE 754 floats                    |
| `qI.F`, `uqI.F`           | 1–8 bytes  | Binary fixed-point, width must be 8/16/32/64 bits |

Booleans use integer types: `{ value = true, type = "u8" }` stores 1.

Fixed-point examples: `uq8.8` (unsigned 16-bit), `uq0.16` (unsigned 16-bit pure fraction), `q7.8` (signed 16-bit), `q15.16` (signed 32-bit). mint encodes them as `round_ties_even(input * 2^F)`.

## Field sources

Every field in `[block.data]` has a `type` and exactly one source. Sources are mutually exclusive.

### Literal values (`value`)

```toml
device.id = { value = 0x1234, type = "u32" }
message = { value = "Hello", type = "u8", size = 16 }
ip_addr = { value = [192, 168, 1, 1], type = "u8", size = 4 }
```

Strings and arrays require `size`. Strings use `u8` or `u16` storage. Each UTF-8 byte occupies one scalar element and is zero-extended to the storage width in ABI byte order, so `size` counts elements rather than Unicode code points. C28x strings use `type = "u16"`, one byte per 16-bit word.

### Reusable constants (`const`)

```toml
[mint.const]
default_voltage = 3.3
fw_name = "BootloaderV2"
ip_addr = [192, 168, 1, 1]

[block.data]
voltage = { const = "default_voltage", type = "f32" }
message = { const = "fw_name", type = "u8", size = 16 }
ip_addr = { const = "ip_addr", type = "u8", size = 4 }
base = { const = "block.start_address", type = "u32" }
```

Const values use the same literal shapes and conversion rules as `value`. Each block automatically exposes `<block>.start_address` and `<block>.length` using block header values.

### Data source lookup (`name`)

```toml
device.name = { name = "DeviceName", type = "u8", size = 16 }
version = { name = "Version", type = "u16" }
gain = { name = "VoltageGain", type = "uq8.8" }
coefficients = { name = "Coefficients", type = "f32", size = 4 }
matrix = { name = "Matrix", type = "i16", size = [2, 2] }
```

The `name` string must match a key in the data source. For arrays, `size` specifies dimensions — use `size = N` for 1D, `size = [rows, cols]` for 2D.

**`size` vs `SIZE`**: Lowercase `size` pads undersized data with the block's padding byte. Uppercase `SIZE` errors if the data source provides fewer elements than declared. Use `SIZE` when short data would indicate a real problem.

### Bitmaps (`bitmap`)

Pack multiple named or literal values into a single integer field.

```toml
config.flags = { type = "u16", bitmap = [
    { bits = 1, name = "EnableDebug" },
    { bits = 3, value = 0 },
    { bits = 4, name = "RegionCode" },
    { bits = 8, value = 0 },
] }
```

Fields pack LSB-first. The total bits **must** equal the type's bit width (e.g., 16 for `u16`). Each bitmap sub-field can use `name` (data source) or `value` (literal). Signed types use two's complement for negative values.

Fixed-point types are not valid with `bitmap`.

### Refs / pointers (`ref`)

Store resolved or literal absolute target addresses.

```c
typedef struct {
  struct {
    uint16_t entries[32];
    uint16_t count;
  } table;
  uint32_t table_ptr;   /* address of table */
  uint32_t count_ptr;   /* address of table.count */
  uint32_t none;
  uint32_t external;
  uint32_t ptrs[8];
} lookup_t;
```

```toml
table.entries = { name = "TableEntries", type = "u16", size = 32 }
table.count = { name = "TableCount", type = "u16" }
table_ptr = { ref = "table", type = "u32" }
count_ptr = { ref = "table.count", type = "u32" }
none = { ref = 0, type = "u32" }
external = { ref = 0x40001000, type = "u32" }
ptrs = { ref = ["table", 0, "table.count", 0x40001000], type = "u32", size = 8 }
```

A path target is rooted at the block's data section and is validated before field values are emitted. It resolves to `start_address + field_offset_octets / address_unit_octets`. An unsigned integer target is already an absolute address in target address units, so mint does not rebase or convert it; zero is an intentional zero/null address. Nonzero literals can address targets outside the layout, but mint cannot validate or rebase them.

The `type` must be `u16`, `u32` or `u64`, and every address must fit it. A scalar ref cannot use `size`/`SIZE`. A reflist can mix paths and literals and requires a one-dimensional capacity. Lowercase `size` zero-fills missing address slots; uppercase `SIZE` requires the exact number of entries. Both reject overfill. Refs to paths can point forward or backward within the same block; cross-block path refs are not supported. Generated headers keep refs as integer address storage rather than C pointer objects.

### ABI fingerprints (`fingerprint`)

Store the containing block's ABI fingerprint or another block's fingerprint from the same layout:

```toml
[config.data]
schema = { fingerprint = true, type = "u64" }

[manifest.data]
config_schema = { fingerprint = "config", type = "u64" }
```

Fingerprint fields require `u64` and cannot use `size`/`SIZE`. Fingerprints cover the effective, nameless ABI: byte order, address-unit width, types, dimensions, offsets, storage sizes, alignment, array strides, bitmap widths and ref topology. Resolved ref targets contribute target shape and position. Literal addresses contribute an opaque marker but not their numeric value; reflist underfill also uses that marker. ABI names, field names, values, producer choices (`name`, `value` or `const`), block addresses, allocated lengths and padding values do not contribute. A selected block is fully validated, while its fingerprint targets have only their ABI shapes resolved; unrelated siblings are not resolved. `mint fingerprint layout.toml#config` prints one bare 16-character lowercase value; `mint fingerprint layout.toml` fully validates the whole file and prints `block fingerprint` lines. Generated headers expose fingerprint fields as `<BLOCK>_<FIELD>_FINGERPRINT` macros.

### Checksums (`checksum`)

Compute a CRC over all preceding bytes in the block and place the result inline.

```toml
[mint.checksum.crc32]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true

[block.data]
# ... fields ...
checksum = { checksum = "crc32", type = "u32" }
```

The checksum covers everything from the start of the block's data up to (but not including) the checksum field itself, including any alignment padding between fields. Type must be `u32`. Fixed-point types are not valid with `checksum`. The referenced name must match a `[mint.checksum.<name>]` config. Multiple checksum fields are resolved in order, so later checksums include earlier ones.

For cross-block CRC or non-CRC algorithms, use a separate hex post-processing tool.

## Alignment

mint applies the selected ABI profile's **natural C aggregate alignment**. The generic, ARM AAPCS32 and RISC-V ILP32 profiles align each integer or fixed-point leaf to its storage width, `f32` to 4 octets and `f64` to 8 octets. The TriCore and TI C28x EABI profiles instead align 64-bit scalars to 4 octets while retaining 8-octet storage and array stride. They also give every aggregate larger than one octet at least 2-octet alignment; a single-octet aggregate stays byte-aligned. C28x rejects exact-width 8-bit fields. Its strings therefore use `type = "u16"`, with one UTF-8 byte per 16-bit word. Its standard HEX/S-record output uses octet addresses equal to twice the target word address. Each dotted-path branch otherwise aligns to the maximum alignment of its children, preserves parsed child order, and receives tail padding before the next sibling. The root data struct also receives tail padding, so its reserved size matches `sizeof` under this ABI. Generated headers assert every field offset and final structure size against the target compiler. All gaps use the block's `padding` byte. The resolved data payload must fit the configured block length and cannot exceed Mint's 256 MiB in-memory materialization limit.

**This means mint does not support packed structs.** If the target C code uses `__attribute__((packed))`, `#pragma pack(1)`, or similar, the TOML layout will produce different offsets than the firmware expects. There is no way to disable alignment in mint. If the firmware uses packed structs, this is a fundamental incompatibility — raise it with the user immediately.

Similarly, mint writes fields in declaration order and cannot reorder them. If the compiler performs struct field reordering (some do for optimization), the layout must match the compiler's actual output, not the source declaration order. When in doubt, check the compiled output or a map file.

## Data sources

A data source is optional — layouts with only `value` fields build without one. You cannot combine multiple data sources in a single build.

### Excel (`--xlsx`)

The workbook has a **Main sheet** (or specify `--main-sheet`) with this structure:

| Name         | Default        | Debug              | Production |
| ------------ | -------------- | ------------------ | ---------- |
| DeviceName   | MyDevice       | DebugDev           |            |
| Version      | 1              | 2                  | 1          |
| EnableDebug  | 0              | 1                  | 0          |
| RegionCode   | 5              | 5                  | 12         |
| Counter      | 1000           | 0                  | 50000      |
| Coefficients | #Coefficients  | #DebugCoefficients |            |
| Matrix       | #Matrix        | #Matrix            |            |

- **Name column**: lookup keys matching layout `name` fields
- **Variant columns**: one per build variant. Empty and whitespace-only cells fall through in `--variants` order.
- **Array sheet refs**: A cell value like `#Coefficients` points to a separate sheet containing array data. First row is headers (ignored). Header column count defines 2D width. Values are read row-by-row until an empty cell.

A `Coefficients` sheet for a 1D `f32` array:

| C1  |
| --- |
| 1.0 |
| 2.5 |
| 3.7 |
| 4.2 |

A `Matrix` sheet for a 2D `i16` array (2x2):

| C1  | C2  |
| --- | --- |
| 10  | 20  |
| 30  | 40  |

### JSON (`--json`)

```json
{
  "Default": {
    "DeviceName": "MyDevice",
    "Version": 1,
    "EnableDebug": 0,
    "RegionCode": 5,
    "Counter": 1000,
    "Coefficients": [1.0, 2.5, 3.7, 4.2],
    "Matrix": [
      [10, 20],
      [30, 40]
    ]
  },
  "Debug": {
    "DeviceName": "DebugDev",
    "Version": 2,
    "EnableDebug": 1
  },
  "Production": {
    "RegionCode": 12,
    "Counter": 50000
  }
}
```

Top-level keys are variant names. Each contains an object of name:value pairs. Arrays are native JSON arrays. 2D arrays are arrays of arrays. Accepts a file path or inline JSON string.

### Variant priority (`--variants`)

`--variants Debug/Default` checks Debug first. Missing keys and `null` values fall through to Default.

**Name matching**: The `name` field in the layout must exactly match a key in the data source. These are case-sensitive. When setting up a new data source, collect all `name = "..."` values from the layout and ensure each one exists in the source.

## CLI quick reference

```bash
# Basic build
mint build layout.toml --xlsx data.xlsx --variants Default -o firmware.hex

# Specific blocks
mint build layout.toml#config layout.toml#data --xlsx data.xlsx --variants Default -o out.hex

# C header for all blocks (no data source required)
mint header layout.toml -o layout.h

# ABI fingerprints without a data source or build
mint fingerprint layout.toml#config
mint fingerprint layout.toml

# Discover accepted ABI profiles and inspect their effective rules
mint abi list
mint abi show arm-aapcs32-le

# JSON data source (file or inline)
mint build layout.toml --json data.json --variants Debug/Default -o out.hex
mint build layout.toml --json '{"Default":{"DeviceName":"MyDevice","Version":1}}' --variants Default -o out.hex

# Output format options
--format hex              # Intel HEX (default)
--format mot              # Motorola S-Record
--record-width 16         # Bytes per record (1-128, default 32)

# Build options
--strict                  # Error on lossy type conversions (instead of saturate/truncate)
--stats                   # Print block-by-block size and checksum summary
--quiet                   # Suppress all output except errors
--export-json report.json # Dump resolved field values as JSON
```

Run `mint --help` for the full argument list.

## Common patterns

**Multiple blocks, one file**: Define several `[blockname.header]` / `[blockname.data]` sections. Build all with `mint build layout.toml` or select with `layout.toml#blockname`.

**Generated C header**: Run `mint header layout.toml -o layout.h`. Each block emits `<BLOCK>_START_ADDRESS` and `<BLOCK>_LENGTH` macros. Dotted paths become nested structs, arrays use generated extent macros, named bitmap regions receive shift and mask macros, and fingerprint fields receive expected-value macros. Layout parsing guarantees valid block and field names; header generation rejects statically invalid selected layouts and generated-name collisions.

**Multiple CRC configs**: Define `[mint.checksum.crc32]` and `[mint.checksum.crc32c]` (or any names). Reference by name in checksum fields.

**Constants + data source in one block**: Mix `value` and `name` fields freely. Fields with `value` don't need a data source.

**CI integration**: `mint build` reads files and writes a hex file. Wire it into any build system as a custom command that depends on the layout and data files and produces the hex output.

## Gotchas

- **Bitmap bit sum**: The total bits in a bitmap must exactly equal the type width. A `u16` bitmap needs exactly 16 bits across all sub-fields.
- **2D arrays must come from data source**: You cannot inline a 2D array literal in TOML. Use a `name` reference instead.
- **Checksum type**: Must be `u32`. No other widths are supported.
- **Ref type**: Must be unsigned (`u16`, `u32`, `u64`).
- **Fingerprint type**: Must be `u64`; targets are `true` or another block in the same layout.
- **`size`/`SIZE` cannot combine with scalar `ref`, `checksum`, `fingerprint`, or `bitmap`.** Reflists require one-dimensional `size`/`SIZE`.
- **Strict mode**: Without `--strict`, out-of-range integer values saturate and float-to-int casts truncate (e.g., 300 into `u8` becomes 255, 1.5 into `u8` becomes 1). Fixed-point values scale by `2^F`, round ties-to-even, then clamp. With `--strict`, mint errors instead.

## Further reference

Online documentation: the mint repository's `doc/` directory contains `layout.md`, `sources.md`, and `cli.md` with exhaustive detail on every option (github.com/tomrford/mint).
