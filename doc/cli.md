# Command Line Interface

mint builds flash blocks from layout files and data sources, and generates matching C headers and ABI fingerprints from layouts.

```
mint build [OPTIONS] [FILE[#BLOCK] | FILE]...
mint header [FILE[#BLOCK] | FILE]... -o FILE
mint fingerprint FILE[#BLOCK]
mint abi list
mint abi show ABI
mint skill
```

`mint build` is the build command.

## Positional Arguments

### `[FILE[#BLOCK] | FILE]...`

Specifies which blocks to build. Two formats are supported:

| Format              | Description                             |
| ------------------- | --------------------------------------- |
| `layout.toml#block` | Build specific block from layout file   |
| `layout.toml`       | Build all blocks defined in layout file |

**Examples:**

```bash
# Build single block
mint build layout.toml#config --xlsx data.xlsx --variants Default -o config.hex

# Build multiple specific blocks
mint build layout.toml#config layout.toml#data --xlsx data.xlsx --variants Default -o firmware.hex

# Build all blocks from a file
mint build layout.toml --xlsx data.xlsx --variants Default -o output.hex

# Mix both styles
mint build layout.toml#config layout.toml --xlsx data.xlsx --variants Default -o combined.hex
```

---

## C header generation

`mint header` uses the same file and block selectors as `mint build`, but requires no data source:

```bash
# Generate every block in layout order
mint header layout.toml -o layout.h

# Generate selected blocks in argument order
mint header layout.toml#config layout.toml#data -o blocks.h
```

The command renders and validates the complete header before writing it. Each block becomes a `<block>_t` typedef. Dotted paths become inline nested structs, array dimensions use generated macros, and named bitmap regions receive `_SHIFT` and `_MASK` macros. The output contains storage types and shape only; it does not contain data values, block addresses, packing directives, or explicit padding members.

Header generation runs the build's static validation for selected blocks. It rejects invalid resolved shapes and selector-only errors, including dangling const names, scalar consts with array sizes, two-dimensional literals, zero-extent arrays, invalid checksum placement, ref addresses that do not fit their storage type and emitted ranges outside the 32-bit address space.

Generated structs use the selected ABI profile's aggregate rules and include C11 `_Static_assert` checks for every field offset and final structure size. The checks use `CHAR_BIT` so their expected values remain expressed in octets.

---

## ABI discovery

Every layout selects a named ABI profile in `[mint]`. List the accepted names without parsing a layout:

```bash
mint abi list
```

Inspect one profile's byte order, target addressable unit, output-address convention, scalar storage/alignment/stride table and aggregate rules:

```bash
mint abi show generic-le
```

`generic-le`, `generic-be`, `arm-aapcs32-le` and `riscv-ilp32-le` use the same natural-width layout family. `tricore-eabi-le` and `ti-c28x-eabi` align 64-bit scalars to 4 octets while retaining 8-octet storage and array stride. C28x rejects exact-width 8-bit fields and strings. Profile names do not contribute to ABI fingerprints: profiles with the same effective layout and address semantics remain compatible.

Output format remains an independent build option. Intel HEX and Motorola S-record output use standard octet addresses. For C28x, record addresses are twice the target word address and record width must be an even number of octets. Mint does not currently emit TI's native word-addressed HEX dialect.

---

## ABI fingerprints

`mint fingerprint` calculates fingerprints without a data source or block build. Selecting one block fully validates that block and resolves the ABI shape of its fingerprint targets without fully validating those targets or resolving unrelated siblings. It then prints exactly the selected block's 16-character lowercase hexadecimal value:

```bash
mint fingerprint layout.toml#config
```

```text
206a2310660bb1cf
```

Selecting a file fully validates every block and prints them in declaration order as `<block> <fingerprint>`:

```bash
mint fingerprint layout.toml
```

```text
config 206a2310660bb1cf
data c1c13126ea0f1e6b
```

Stdout contains only these values. Diagnostics use stderr and failures return a non-zero exit code, so the command is suitable for build-system extraction. The fingerprint is also available as a macro when `mint header` encounters a `fingerprint` field.

For configure-time CMake integration, track the layout as a configure dependency and pass the bare selected-block value into firmware compilation:

```cmake
set(LAYOUT "${CMAKE_CURRENT_SOURCE_DIR}/layout.toml")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${LAYOUT}")

execute_process(
  COMMAND mint fingerprint "${LAYOUT}#config"
  OUTPUT_VARIABLE CONFIG_FINGERPRINT
  OUTPUT_STRIP_TRAILING_WHITESPACE
  COMMAND_ERROR_IS_FATAL ANY
)

target_compile_definitions(
  firmware PRIVATE "CONFIG_SCHEMA_FINGERPRINT=0x${CONFIG_FINGERPRINT}ULL"
)
```

---

## Data Source Options

You can specify exactly one supported data source (`-x`/`--xlsx` or `-j`/`--json`) along with variants (`--variants`).

If your data currently comes from another system, fetch or transform it first and then pass the resulting JSON via `--json`.

### `-x, --xlsx <FILE>`

Path to Excel workbook containing variant data.

```bash
mint build layout.toml --xlsx data.xlsx --variants Default -o output.hex
```

### `--main-sheet <NAME>`

Override the default main sheet name (`Main`) for the excel data source.

```bash
mint build layout.toml --xlsx data.xlsx --main-sheet Config --variants Default -o output.hex
```

### `-j, --json <PATH or JSON>`

Use raw JSON as the data source. Accepts a JSON file path or inline JSON string.

The JSON format is an object with variant names as top-level keys. Each variant contains an object with name:value pairs.

```bash
# Using a JSON file
mint build layout.toml --json data.json --variants Debug/Default -o output.hex

# Using inline JSON
mint build layout.toml --json '{"Default":{"DeviceName":"MyDevice","Version":1,"Counter":1000},"Debug":{"DeviceName":"DebugDevice","Version":2}}' --variants Debug/Default -o output.hex
```

**Example JSON format:**

```json
{
  "Default": {
    "DeviceName": "MyDevice",
    "Version": 1,
    "Counter": 1000,
    "Coefficients": [1.0, 2.5, 3.7, 4.2],
    "Matrix": [
      [10, 20],
      [30, 40]
    ]
  },
  "Debug": {
    "DeviceName": "DebugDevice",
    "Version": 2
  }
}
```

See [Data Sources](sources.md#json---json) for format details.

### `-v, --variants <NAME[/NAME...]>`

Variant columns to query, in priority order. The first non-empty value found wins.

```bash
# Single variant
mint build layout.toml --xlsx data.xlsx --variants Default -o output.hex

# Fallback chain: try Debug first, then Default
mint build layout.toml --xlsx data.xlsx --variants Debug/Default -o output.hex

# Three-level fallback
mint build layout.toml --xlsx data.xlsx --variants Production/Debug/Default -o output.hex
```

---

## Output Options

### `-o, --out <FILE>`

Output file path. Parent directories are created if they don't exist.

**Default:** `out.hex`

```bash
# Output to specific file
mint build layout.toml --xlsx data.xlsx --variants Default -o build/firmware.hex

# Output with .mot extension for Motorola S-Record
mint build layout.toml --xlsx data.xlsx --variants Default -o build/firmware.mot --format mot
```

### `--format <FORMAT>`

Output file format.

| Value | Description         | Extension |
| ----- | ------------------- | --------- |
| `hex` | Intel HEX (default) | `.hex`    |
| `mot` | Motorola S-Record   | `.mot`    |

```bash
# Intel HEX (default)
mint build layout.toml --xlsx data.xlsx --variants Default -o output.hex --format hex

# Motorola S-Record
mint build layout.toml --xlsx data.xlsx --variants Default -o output.mot --format mot
```

Mint warns when a recognised file extension conflicts with the selected format. It keeps the output path unchanged. Custom and extensionless file names remain valid.

### `--record-width <N>`

Bytes per data record in output file. Range: 1-128.

**Default:** `32`

```bash
# 16 bytes per record (shorter lines)
mint build layout.toml --xlsx data.xlsx --variants Default -o output.hex --record-width 16

# 64 bytes per record (longer lines)
mint build layout.toml --xlsx data.xlsx --variants Default -o output.hex --record-width 64
```

### `--export-json <FILE>`

Export used `block.data` values as JSON. Report is nested by layout file, then block name.

```bash
mint build layout.toml --xlsx data.xlsx --variants Default -o output.hex --export-json build/report.json
```

---

## Build Options

### `--strict`

Enable strict type conversions. Errors on lossy casts instead of saturating/truncating/clamping.

```bash
mint build layout.toml --xlsx data.xlsx --variants Default -o output.hex --strict
```

**Without `--strict`:**

- Float `1.5` → `u8` becomes `1` (truncated)
- Value `300` → `u8` becomes `255` (saturated)
- Fixed-point `300.5` → `uq8.8` becomes `65535` after scaling, ties-to-even rounding, and clamping

**With `--strict`:**

- Float `1.5` → `u8` produces an error
- Value `300` → `u8` produces an error
- Fixed-point `300.5` → `uq8.8` produces an error after scaling and ties-to-even rounding

For fixed-point `qI.F` / `uqI.F` types, mint always scales by `2^F` and rounds to nearest with ties to even before checking the storage range. Non-finite values are always rejected.

---

## Display Options

### `--stats`

Show detailed build statistics after completion.

```bash
mint build layout.toml --xlsx data.xlsx --variants Default -o output.hex --stats
```

**Example output:**

```
+------------------+-----------+
| Build Summary    |           |
+==============================+
| Build Time       | 1.396ms   |
|------------------+-----------|
| Blocks Processed | 2         |
|------------------+-----------|
| Total Allocated  | 512 bytes |
|------------------+-----------|
| Total Reserved   | 84 bytes  |
|------------------+-----------|
| Space Reserved   | 16.4%     |
+------------------+-----------+

+--------+---------------+--------------------+------------+----------------+
| Block  | Address Range | Reserved/Alloc     | Space Reserved | Checksum Value |
+===========================================================================+
| config | 0x8000-0x80FF | 52 bytes/256 bytes | 20.3%      | 0x89ECCA27     |
|--------+---------------+--------------------+------------+----------------|
| data   | 0x8100-0x81FF | 32 bytes/256 bytes | 12.5%      | 0x160D17D3     |
+--------+---------------+--------------------+------------+----------------+
```

### `--quiet`

Suppress all output except errors.

```bash
mint build layout.toml --xlsx data.xlsx --variants Default -o output.hex --quiet
```

---

## Help & Version

### `-h, --help`

Print help information.

```bash
mint --help
```

### `-V, --version`

Print version information.

```bash
mint --version
```

### `skill`

Print the bundled Mint skill text.

```bash
mint skill
```

---

## Complete Examples

### Basic build with Excel data

```bash
mint build layout.toml --xlsx data.xlsx --variants Default -o firmware.hex
```

### Production build with all options

```bash
mint build \
  layout.toml#config \
  layout.toml#data \
  --xlsx data.xlsx \
  --variants Default \
  -o release/FW_v1.2.3.mot \
  --format mot \
  --record-width 32 \
  --strict \
  --stats
```

### Build with JSON data source

```bash
mint build layout.toml \
  --json data.json \
  --variants Debug/Default \
  -o firmware.hex
```

See [Data Sources](sources.md#json---json) for format details.
