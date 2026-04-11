---
name: mint
description: "Guide for working with mint, an embedded development tool that assembles flash memory hex files from TOML layout files and data sources (Excel/JSON). Use this skill whenever a project uses or mentions mint / mint-cli, when you encounter .toml layout files that define memory blocks for firmware or flash, when you need to create or modify flash block definitions, set up mint in a build system or CI pipeline, or work with Excel/JSON data sources for embedded device configuration. Also trigger when you see references to building Intel HEX or Motorola S-Record files from structured layout definitions, or when a user mentions replacing a custom hex-generation script with a declarative tool."
---

# mint

mint builds binary flash images (Intel HEX or Motorola S-Record) from a declarative TOML layout file and an optional data source (Excel workbook or JSON). Each layout describes one or more memory blocks — contiguous regions that map to C structs stored at known flash addresses. mint resolves data values, enforces types, computes CRCs, pads to size, and emits the output file.

Install: `cargo install mint-cli` or via nix flake.

## Layout file anatomy

A layout file has three levels: global config, per-block headers, and per-block data fields.

```toml
[mint]                    # Global config (required, even if empty)
endianness = "little"     # "little" (default) or "big"
virtual_offset = 0x0      # Added to all computed addresses (default: 0)

[mint.checksum.crc32]     # Named CRC config (define as many as needed)
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true

[myblock.header]          # Per-block memory region
start_address = 0x8000    # Required — base address in flash
length = 0x1000           # Required — allocated size in bytes
padding = 0xFF            # Fill byte for unused space (default: 0xFF)

[myblock.data]            # Field definitions (dotted paths = nested structs)
device.id = { value = 0x1234, type = "u32" }
device.name = { name = "DeviceName", type = "u8", size = 16 }
checksum = { checksum = "crc32", type = "u32" }
```

Multiple blocks can live in one file. Build specific blocks with `layout.toml#blockname`.

### Dotted paths mirror C struct nesting

The key `device.info.version.major` maps to `block.device.info.version.major` in the output — the same hierarchy as nested C structs. This is how mint knows field ordering and grouping.

## Information to gather before writing a layout

When setting up mint for a project, these parameters need to be established. If replacing an existing system, many can be inferred from the codebase (struct definitions, linker scripts, existing hex generators). Always confirm with the user before proceeding.

**From the hardware/firmware side:**

- **Endianness** — little or big. Check target MCU architecture or existing byte-swap calls.
- **Block addresses and sizes** — from linker script, memory map, or flash layout documentation. Each block needs a `start_address` and `length`.
- **Padding byte** — usually `0xFF` (erased flash state) but confirm. Some platforms use `0x00`.
- **CRC algorithm** — if blocks need integrity checks, you need the polynomial, initial value, XOR-out, and reflection settings. Check existing CRC routines or documentation.
- **Struct layout** — C header files defining the structs that live at each flash address. These become the `[block.data]` fields.

**From the data/build side:**

- **Which values are constants vs. configurable** — constants go as `value = ...` in the layout; configurable values use `name = "..."` to pull from a data source.
- **Data source format** — Excel workbook (typical for manufacturing/calibration workflows) or JSON (typical for CI pipelines that fetch or generate data).
- **Variant names** — the columns/keys that represent build variants (e.g., Default, Debug, Production). The `-v` flag controls fallback priority.

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

Strings and arrays require `size`. Strings are UTF-8 encoded into the byte array.

### Data source lookup (`name`)

```toml
device.name = { name = "DeviceName", type = "u8", size = 16 }
version = { name = "Version", type = "u16" }
gain = { value = 1.5, type = "uq8.8" }
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

Store the absolute address of another field within the same block.

```toml
table.entries = { name = "TableEntries", type = "u16", size = 32 }
table.count = { name = "TableCount", type = "u16" }
table_ptr = { ref = "table", type = "u32" }
count_ptr = { ref = "table.count", type = "u32" }
```

The ref target is a dotted path rooted at the block's data section. Refs resolve to `start_address + virtual_offset + field_offset`. The `type` must be an unsigned integer (`u16`, `u32`, `u64`). Fixed-point types are not valid with `ref`. Forward and backward refs both work. Cross-block refs are not supported.

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

mint applies **natural alignment**: each field is aligned to a boundary matching its type width (e.g., `u32` aligns to 4 bytes, `u16` / `uq8.8` to 2 bytes). Gaps are filled with the block's `padding` byte.

**This means mint does not support packed structs.** If the target C code uses `__attribute__((packed))`, `#pragma pack(1)`, or similar, the TOML layout will produce different offsets than the firmware expects. There is no way to disable alignment in mint. If the firmware uses packed structs, this is a fundamental incompatibility — raise it with the user immediately.

Similarly, mint writes fields in declaration order and cannot reorder them. If the compiler performs struct field reordering (some do for optimization), the layout must match the compiler's actual output, not the source declaration order. When in doubt, check the compiled output or a map file.

## Data sources

A data source is optional — layouts with only `value` fields build without one. You cannot combine multiple data sources in a single build.

### Excel (`--xlsx`)

The workbook has a **Main sheet** (or specify `--main-sheet`) with this structure:

| Name         | Default              | Debug              | Production |
| ------------ | -------------------- | ------------------ | ---------- |
| DeviceName   | MyDevice             | DebugDev           |            |
| Version      | 1                    | 2                  | 1          |
| Counter      | 1000                 | 0                  | 50000      |
| Coefficients | #DefaultCoefficients | #DebugCoefficients |            |
| Matrix       | #CalibrationMatrix   | #CalibrationMatrix |            |

- **Name column**: lookup keys matching layout `name` fields
- **Variant columns**: one per build variant. First non-empty value in the `-v` priority chain wins.
- **Array sheet refs**: A cell value like `#DefaultCoefficients` points to a separate sheet containing array data. First row is headers (ignored), values read row-by-row until an empty cell.

### JSON (`--json`)

```json
{
  "Default": {
    "DeviceName": "MyDevice",
    "Version": 1,
    "Counter": 1000,
    "Coefficients": [1.0, 2.0, 3.0, 4.0],
    "Matrix": [
      [10, 20],
      [30, 40]
    ]
  },
  "Debug": {
    "DeviceName": "DebugDev",
    "Version": 2
  }
}
```

Top-level keys are variant names. Each contains an object of name:value pairs. Arrays are native JSON arrays. 2D arrays are arrays of arrays. Accepts a file path or inline JSON string.

### Variant priority (`-v`)

`-v Debug/Default` means: look in Debug first, fall back to Default if the key is missing or null. The first non-empty value wins.

**Name matching**: The `name` field in the layout must exactly match a key in the data source. These are case-sensitive. When setting up a new data source, collect all `name = "..."` values from the layout and ensure each one exists in the source.

## CLI quick reference

```bash
# Basic build
mint layout.toml --xlsx data.xlsx -v Default -o firmware.hex

# Specific blocks
mint layout.toml#config layout.toml#data --xlsx data.xlsx -v Default -o out.hex

# JSON data source (file or inline)
mint layout.toml --json data.json -v Debug/Default -o out.hex
mint layout.toml --json '{"Default":{"DeviceName":"MyDevice","Version":1}}' -v Default -o out.hex

# Output format options
--format hex              # Intel HEX (default)
--format mot              # Motorola S-Record
--record-width 16         # Bytes per record (1-64, default 32)

# Build options
--strict                  # Error on lossy type conversions (instead of saturate/truncate)
--stats                   # Print block-by-block size and checksum summary
--quiet                   # Suppress all output except errors
--export-json report.json # Dump resolved field values as JSON
```

Run `mint --help` for the full argument list.

## Common patterns

**Multiple blocks, one file**: Define several `[blockname.header]` / `[blockname.data]` sections. Build all with `mint layout.toml` or select with `layout.toml#blockname`.

**Multiple CRC configs**: Define `[mint.checksum.crc32]` and `[mint.checksum.crc32c]` (or any names). Reference by name in checksum fields.

**Constants + data source in one block**: Mix `value` and `name` fields freely. Fields with `value` don't need a data source.

**CI integration**: mint's interface is a single command that reads files and writes a hex file. Wire it into any build system as a custom command that depends on the layout and data files and produces the hex output.

## Gotchas

- **Bitmap bit sum**: The total bits in a bitmap must exactly equal the type width. A `u16` bitmap needs exactly 16 bits across all sub-fields.
- **2D arrays must come from data source**: You cannot inline a 2D array literal in TOML. Use a `name` reference instead.
- **Checksum type**: Must be `u32`. No other widths are supported.
- **Ref type**: Must be unsigned (`u16`, `u32`, `u64`).
- **`size`/`SIZE` cannot combine with `ref`, `checksum`, or `bitmap`.**
- **Strict mode**: Without `--strict`, out-of-range integer values saturate and float-to-int casts truncate (e.g., 300 into `u8` becomes 255, 1.5 into `u8` becomes 1). Fixed-point values scale by `2^F`, round ties-to-even, then clamp. With `--strict`, mint errors instead.

## Further reference

For full annotated examples (layout + C struct + data source), read `references/examples.md` in this skill.

Online documentation: the mint repository's `doc/` directory contains `layout.md`, `sources.md`, and `cli.md` with exhaustive detail on every option (github.com/tomrford/mint).
