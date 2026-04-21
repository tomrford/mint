# Command Line Interface

mint builds flash blocks from layout files and data sources, emitting Intel HEX or Motorola S-Record files.

```
mint [OPTIONS] [FILE[#BLOCK] | FILE]...
```

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
mint layout.toml#config --xlsx data.xlsx -v Default -o config.hex

# Build multiple specific blocks
mint layout.toml#config layout.toml#data --xlsx data.xlsx -v Default -o firmware.hex

# Build all blocks from a file
mint layout.toml --xlsx data.xlsx -v Default -o output.hex

# Mix both styles
mint layout.toml#config layout.toml --xlsx data.xlsx -v Default -o combined.hex
```

---

## Data Source Options

You can specify exactly one supported data source (`-x`/`--xlsx` or `-j`/`--json`) along with versions (`-v`).

If your data currently comes from another system, fetch or transform it first and then pass the resulting JSON via `--json`.

### `-x, --xlsx <FILE>`

Path to Excel workbook containing versioned data.

```bash
mint layout.toml --xlsx data.xlsx -v Default -o output.hex
```

### `--main-sheet <NAME>`

Override the default main sheet name (`Main`) for the excel data source.

```bash
mint layout.toml --xlsx data.xlsx --main-sheet Config -v Default -o output.hex
```

### `-j, --json <PATH or JSON>`

Use raw JSON as the data source. Accepts a JSON file path or inline JSON string.

The JSON format is an object with version names as top-level keys. Each version contains an object with name:value pairs.

```bash
# Using a JSON file
mint layout.toml --json data.json -v Debug/Default -o output.hex

# Using inline JSON
mint layout.toml --json '{"Default":{"DeviceName":"MyDevice","Version":1,"Counter":1000},"Debug":{"DeviceName":"DebugDevice","Version":2}}' -v Debug/Default -o output.hex
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

### `-v, --versions <NAME[/NAME...]>`

Version columns to query, in priority order. The first non-empty value found wins.

```bash
# Single version
mint layout.toml --xlsx data.xlsx -v Default -o output.hex

# Fallback chain: try Debug first, then Default
mint layout.toml --xlsx data.xlsx -v Debug/Default -o output.hex

# Three-level fallback
mint layout.toml --xlsx data.xlsx -v Production/Debug/Default -o output.hex
```

---

## Output Options

### `-o, --out <FILE>`

Output file path. Parent directories are created if they don't exist.

**Default:** `out.hex`

```bash
# Output to specific file
mint layout.toml --xlsx data.xlsx -v Default -o build/firmware.hex

# Output with .mot extension for Motorola S-Record
mint layout.toml --xlsx data.xlsx -v Default -o build/firmware.mot --format mot
```

### `--format <FORMAT>`

Output file format.

| Value | Description         | Extension |
| ----- | ------------------- | --------- |
| `hex` | Intel HEX (default) | `.hex`    |
| `mot` | Motorola S-Record   | `.mot`    |

```bash
# Intel HEX (default)
mint layout.toml --xlsx data.xlsx -v Default -o output.hex --format hex

# Motorola S-Record
mint layout.toml --xlsx data.xlsx -v Default -o output.mot --format mot
```

### `--record-width <N>`

Bytes per data record in output file. Range: 1-64.

**Default:** `32`

```bash
# 16 bytes per record (shorter lines)
mint layout.toml --xlsx data.xlsx -v Default -o output.hex --record-width 16

# 64 bytes per record (longer lines)
mint layout.toml --xlsx data.xlsx -v Default -o output.hex --record-width 64
```

### `--export-json <FILE>`

Export used `block.data` values as JSON. Report is nested by layout file, then block name.

```bash
mint layout.toml --xlsx data.xlsx -v Default -o output.hex --export-json build/report.json
```

---

## Build Options

### `--strict`

Enable strict type conversions. Errors on lossy casts instead of saturating/truncating/clamping.

```bash
mint layout.toml --xlsx data.xlsx -v Default -o output.hex --strict
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
mint layout.toml --xlsx data.xlsx -v Default -o output.hex --stats
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
| Total Used       | 84 bytes  |
|------------------+-----------|
| Space Used       | 16.4%     |
+------------------+-----------+

+--------+---------------+--------------------+------------+----------------+
| Block  | Address Range | Used/Alloc         | Space Used | Checksum Value |
+===========================================================================+
| config | 0x8000-0x80FF | 52 bytes/256 bytes | 20.3%      | 0x89ECCA27     |
|--------+---------------+--------------------+------------+----------------|
| data   | 0x8100-0x81FF | 32 bytes/256 bytes | 12.5%      | 0x160D17D3     |
+--------+---------------+--------------------+------------+----------------+
```

### `--quiet`

Suppress all output except errors.

```bash
mint layout.toml --xlsx data.xlsx -v Default -o output.hex --quiet
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

---

## Complete Examples

### Basic build with Excel data

```bash
mint layout.toml --xlsx data.xlsx -v Default -o firmware.hex
```

### Production build with all options

```bash
mint \
  layout.toml#config \
  layout.toml#data \
  --xlsx data.xlsx \
  -v Default \
  -o release/FW_v1.2.3.mot \
  --format mot \
  --record-width 32 \
  --strict \
  --stats
```

### Build with JSON data source

```bash
mint layout.toml \
  --json data.json \
  -v Debug/Default \
  -o firmware.hex
```

See [Data Sources](sources.md#json---json) for format details.
