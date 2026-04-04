# mint examples and schema reference

Complete annotated examples and an exhaustive layout schema reference.

## Layout schema reference

Every accepted key in a mint layout file, with types, defaults, and constraints.

### `[mint]` — global configuration (required)

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `endianness` | `"little"` \| `"big"` | — (required) | Byte order for all multi-byte values |
| `virtual_offset` | `u32` (hex ok) | `0` | Offset added to all computed addresses (refs, output) |

Legacy key `[settings]` is rejected with a migration hint.

### `[mint.checksum.<name>]` — named CRC configurations (optional, repeatable)

Define as many as needed (e.g., `[mint.checksum.crc32]`, `[mint.checksum.crc32c]`). Referenced by name in checksum fields.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `polynomial` | `u32` | — (required) | CRC polynomial |
| `start` | `u32` | — (required) | Initial CRC value |
| `xor_out` | `u32` | — (required) | XOR applied to final CRC |
| `ref_in` | `bool` | — (required) | Reflect each input byte |
| `ref_out` | `bool` | — (required) | Reflect final CRC before XOR |

All fields are required — no inheritance or partial configs. Legacy key `[mint.crc]` is rejected.

### `[blockname.header]` — per-block memory region (required per block)

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `start_address` | `u32` (hex ok) | — (required) | Base address in flash |
| `length` | `u32` (hex ok) | — (required) | Allocated size in bytes |
| `padding` | `u8` (hex ok) | `0xFF` | Fill byte for unused space and alignment gaps |

Legacy keys `crc` and `crc_location` on headers are rejected with migration hints.

### `[blockname.data]` — field definitions

Each key is a dotted path representing struct nesting. The value is an inline table with a required `type` and exactly one source.

#### Field attributes

| Attribute | Type | Description |
|-----------|------|-------------|
| `type` | string | Required. One of: `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`, `i64`, `f32`, `f64` |
| `value` | scalar, string, or array | Literal value. Mutually exclusive with other sources. |
| `name` | string | Data source lookup key. Mutually exclusive with other sources. |
| `bitmap` | array of bitmap fields | Bitfield packing. Mutually exclusive with other sources. |
| `ref` | string | Dotted path to another field in same block. Mutually exclusive with other sources. |
| `checksum` | string | Name of a `[mint.checksum.<name>]` config. Mutually exclusive with other sources. |
| `size` | integer or `[rows, cols]` | Array/string dimensions. Pads if data is shorter. Cannot combine with `SIZE`, `ref`, `checksum`, or `bitmap`. |
| `SIZE` | integer or `[rows, cols]` | Strict array dimensions. Errors if data is shorter. Cannot combine with `size`, `ref`, `checksum`, or `bitmap`. |

#### Source constraints

| Source | Allowed types | `size`/`SIZE` | Notes |
|--------|--------------|---------------|-------|
| `value` (scalar) | any | no | Numeric, boolean, or string literal |
| `value` (string) | `u8` | required | UTF-8 encoded into byte array |
| `value` (1D array) | any | required | Inline array of values |
| `value` (2D array) | — | — | **Not supported.** 2D arrays must come from a data source. |
| `name` (scalar) | any | no | Single value from data source |
| `name` (1D array) | any | required (`size = N`) | 1D array from data source |
| `name` (2D array) | any | required (`size = [R, C]`) | 2D array from data source |
| `bitmap` | integer types only | no | Sum of `bits` must equal type width |
| `ref` | `u16`, `u32`, `u64` | no | Resolves to absolute address of target |
| `checksum` | `u32` only | no | CRC over all preceding bytes in block |

#### Bitmap sub-field schema

Each element in the `bitmap` array:

| Key | Type | Description |
|-----|------|-------------|
| `bits` | integer (>0) | Number of bits this sub-field occupies |
| `name` | string | Data source lookup key (mutually exclusive with `value`) |
| `value` | scalar | Literal value (mutually exclusive with `name`) |

Fields pack LSB-first. Signed parent types use two's complement for negative sub-field values.

### Alignment behavior

Fields are naturally aligned to their type width:
- `u8`/`i8`: 1-byte aligned (no padding)
- `u16`/`i16`: 2-byte aligned
- `u32`/`i32`/`f32`: 4-byte aligned
- `u64`/`i64`/`f64`: 8-byte aligned

Gaps between fields are filled with the block's `padding` byte. This alignment is always applied — mint does not support packed structs (`__attribute__((packed))`, `#pragma pack(1)`, etc.).

---

## C struct to TOML layout

Given this C header:

```c
typedef struct {
  struct {
    uint32_t id;
    uint8_t name[16];
  } device;
  uint16_t version;
  uint16_t flags; /* bitmap: [0] EnableDebug, [1:3] reserved, [4:7] RegionCode, [8:15] reserved */
  float coefficients[4];
  int16_t matrix[2][2];
  uint32_t crc;
} config_t; /* at 0x8000, 256 bytes allocated */

typedef struct {
  uint64_t counter;
  uint8_t message[16];
  uint8_t ip[4];
  uint32_t crc;
} data_t; /* at 0x8100, 256 bytes allocated */
```

The corresponding layout file:

```toml
[mint]
endianness = "little"

[mint.checksum.crc32]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true

# --- config_t at 0x8000 ---
[config.header]
start_address = 0x8000
length = 0x100

[config.data]
device.id = { value = 0x1234, type = "u32" }
device.name = { name = "DeviceName", type = "u8", size = 16 }
version = { name = "Version", type = "u16" }
flags = { type = "u16", bitmap = [
    { bits = 1, name = "EnableDebug" },
    { bits = 3, value = 0 },
    { bits = 4, name = "RegionCode" },
    { bits = 8, value = 0 },
] }
coefficients = { name = "Coefficients", type = "f32", size = 4 }
matrix = { name = "Matrix", type = "i16", size = [2, 2] }
checksum = { checksum = "crc32", type = "u32" }

# --- data_t at 0x8100 ---
[data.header]
start_address = 0x8100
length = 0x100

[data.data]
counter = { name = "Counter", type = "u64" }
message = { value = "Hello", type = "u8", size = 16 }
ip = { value = [192, 168, 1, 1], type = "u8", size = 4 }
checksum = { checksum = "crc32", type = "u32" }
```

Key observations:

- Dotted paths (`device.id`, `device.name`) reproduce the struct nesting.
- `uint8_t name[16]` becomes `type = "u8", size = 16` — this is how strings and byte arrays work.
- The bitmap's total bits (1+3+4+8 = 16) match the `u16` type width.
- `device.id` uses `value` (constant), while `device.name` uses `name` (from data source).
- Checksum is the last field — it covers everything above it in the block.

## Ref (pointer) fields

When a struct contains a pointer to another field:

```c
typedef struct {
  uint16_t entries[32];
  uint16_t count;
  uint32_t entries_ptr;  /* address of entries[] */
  uint32_t count_ptr;    /* address of count */
} lookup_t;
```

```toml
[lookup.header]
start_address = 0xA000
length = 0x200

[lookup.data]
table.entries = { name = "TableEntries", type = "u16", size = 32 }
table.count = { name = "TableCount", type = "u16" }
entries_ptr = { ref = "table.entries", type = "u32" }
count_ptr = { ref = "table.count", type = "u32" }
```

Ref targets are dotted paths rooted at the block's data section. `ref = "table"` would resolve to the address of the first field under `table` (i.e., `table.entries`). Forward and backward refs both work. Cross-block refs are not supported.

Resolved address: `start_address + virtual_offset + field_offset`.

## Excel data source

### Main sheet

The main sheet (named `Main` by default, override with `--main-sheet`) maps lookup names to variant values:

| Name         | Default       | Debug       | Production |
| ------------ | ------------- | ----------- | ---------- |
| DeviceName   | MyDevice      | DebugDevice |            |
| Version      | 1             | 2           | 1          |
| EnableDebug  | 0             | 1           | 0          |
| RegionCode   | 5             | 5           | 12         |
| Counter      | 1000          | 0           | 50000      |
| Coefficients | #Coefficients |             |            |
| Matrix       | #Matrix       |             |            |

- The **Name** column contains the lookup keys. These must match the `name = "..."` values in the layout exactly (case-sensitive).
- Empty cells fall through to the next variant in the `-v` priority chain.
- `#Coefficients` means "read from the sheet named Coefficients".

### Array sheets

A sheet named `Coefficients` for a 1D `f32` array:

| C1  |
| --- |
| 1.0 |
| 2.5 |
| 3.7 |
| 4.2 |

A sheet named `Matrix` for a 2D `i16` array (2 rows x 2 cols):

| C1  | C2  |
| --- | --- |
| 10  | 20  |
| 30  | 40  |

- First row is always headers (ignored for data).
- Number of header columns defines 2D array width.
- Values are read row-by-row until an empty cell.

## JSON data source

Equivalent to the Excel example above:

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
    "DeviceName": "DebugDevice",
    "Version": 2,
    "EnableDebug": 1,
    "Counter": 0
  },
  "Production": {
    "RegionCode": 12,
    "Counter": 50000
  }
}
```

- Top-level keys are variant names.
- Only include values that differ — missing keys fall through to the next variant.
- 1D arrays are JSON arrays. 2D arrays are arrays of arrays.
- Values: numbers, booleans, strings, arrays. `null` is treated as missing (falls through).

## Build invocations

```bash
# All blocks, Excel source, single variant
mint layout.toml --xlsx data.xlsx -v Default -o firmware.hex

# Specific block, JSON source, variant fallback chain
mint layout.toml#config --json data.json -v Production/Default -o config.hex

# Multiple blocks from same file
mint layout.toml#config layout.toml#data --xlsx data.xlsx -v Default -o combined.hex

# Production build with safety checks and stats
mint layout.toml --xlsx data.xlsx -v Production/Default -o release/fw.hex --strict --stats

# Motorola S-Record output
mint layout.toml --xlsx data.xlsx -v Default -o firmware.mot --format mot
```

## Starting from scratch checklist

When creating a mint layout for a new project:

1. **Identify the structs** — find the C headers defining flash-resident data structures.
2. **Get the memory map** — `start_address` and `length` for each block from the linker script or flash layout docs.
3. **Determine endianness** — match the target MCU's byte order.
4. **Check for CRC requirements** — get the polynomial, initial value, XOR-out, and reflection settings from the firmware's CRC validation code.
5. **Choose padding byte** — usually `0xFF` (erased NOR flash) or `0x00`. Check what the firmware/bootloader expects in unused regions.
6. **Map each struct field** to a TOML entry, choosing `value` for constants or `name` for data-source-driven values.
7. **Set up the data source** — create the Excel workbook or JSON file with all the `name` keys the layout references.
8. **Verify** — build with `--stats` to confirm block sizes and checksums match expectations. Use `--strict` to catch type conversion issues early.
