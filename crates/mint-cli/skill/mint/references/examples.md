# mint examples and schema reference

Complete annotated examples and an exhaustive layout schema reference.

## Layout schema reference

Every accepted key in a mint layout file, with types, defaults, and constraints.

### `[mint]` — global configuration (required)

| Key   | Type                                | Default      | Description                                      |
| ----- | ----------------------------------- | ------------ | ------------------------------------------------ |
| `abi` | Named profile | — (required) | Target layout profile, such as `"riscv-ilp32-le"` or `"ti-c28x-eabi"`; run `mint abi list` to discover accepted names |

### `[mint.checksum.<name>]` — named CRC configurations (optional, repeatable)

Define as many as needed (e.g., `[mint.checksum.crc32]`, `[mint.checksum.crc32c]`). Referenced by name in checksum fields.

| Key          | Type   | Default      | Description                  |
| ------------ | ------ | ------------ | ---------------------------- |
| `polynomial` | `u32`  | — (required) | CRC polynomial               |
| `start`      | `u32`  | — (required) | Initial CRC value            |
| `xor_out`    | `u32`  | — (required) | XOR applied to final CRC     |
| `ref_in`     | `bool` | — (required) | Reflect each input byte      |
| `ref_out`    | `bool` | — (required) | Reflect final CRC before XOR |

All fields are required — no inheritance or partial configs.

### `[blockname.header]` — per-block memory region (required per block)

| Key             | Type           | Default      | Description                                   |
| --------------- | -------------- | ------------ | --------------------------------------------- |
| `start_address` | `u32` (hex ok) | — (required) | Base address in target address units          |
| `length`        | `u32` (hex ok) | — (required) | Allocated octets; resolved data must fit       |
| `padding`       | `u8` (hex ok)  | `0xFF`       | Array, alignment, and tail fill byte           |

### `[blockname.data]` — field definitions

Each key is a dotted path representing struct nesting. The value is an inline table with a required `type` and exactly one source.

#### Field attributes

| Attribute  | Type                      | Description                                                                                                                                         |
| ---------- | ------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `type`     | string                    | Required. One of: `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`, `i64`, `f32`, `f64`, or fixed-point `qI.F` / `uqI.F` with total width 8/16/32/64 |
| `value`    | scalar, string, or array  | Literal value. Mutually exclusive with other sources.                                                                                               |
| `name`     | string                    | Data source lookup key. Mutually exclusive with other sources.                                                                                      |
| `const`    | string                    | Const lookup key from `[mint.const]` or an auto-promoted block header const. Mutually exclusive with other sources.                                 |
| `bitmap`   | array of bitmap fields    | Bitfield packing. Mutually exclusive with other sources.                                                                                            |
| `ref`      | string                    | Dotted path to another field in same block. Mutually exclusive with other sources.                                                                  |
| `checksum` | string                    | Name of a `[mint.checksum.<name>]` config, used inside `checksum = { checksum = \"name\", type = \"u32\" }`. Mutually exclusive with other sources. |
| `fingerprint` | `true` or string       | This block's ABI fingerprint, or another block's fingerprint from the same layout. Mutually exclusive with other sources.                         |
| `size`     | integer or `[rows, cols]` | Array/string dimensions. Pads if data is shorter. Cannot combine with `SIZE`, `ref`, `checksum`, `fingerprint`, or `bitmap`.                        |
| `SIZE`     | integer or `[rows, cols]` | Strict array dimensions. Errors if data is shorter. Cannot combine with `size`, `ref`, `checksum`, `fingerprint`, or `bitmap`.                      |

#### Source constraints

| Source             | Allowed types       | `size`/`SIZE`              | Notes                                                      |
| ------------------ | ------------------- | -------------------------- | ---------------------------------------------------------- |
| `value` (scalar)   | any                 | no                         | Numeric or boolean literal                                 |
| `value` (string)   | `u8`, `u16`         | required                   | One zero-extended UTF-8 byte per scalar element            |
| `value` (1D array) | any                 | required                   | Inline array of values                                     |
| `value` (2D array) | —                   | —                          | **Not supported.** 2D arrays must come from a data source. |
| `const` (scalar)   | any                 | no                         | Reusable literal from `[mint.const]`                       |
| `const` (string)   | `u8`, `u16`         | required                   | Reusable string with one UTF-8 byte per scalar element     |
| `const` (1D array) | any                 | required                   | Reusable inline array from `[mint.const]`                  |
| `name` (scalar)    | any                 | no                         | Single value from data source                              |
| `name` (1D array)  | any                 | required (`size = N`)      | 1D array from data source                                  |
| `name` (2D array)  | any                 | required (`size = [R, C]`) | 2D array from data source                                  |
| `bitmap`           | integer types only  | no                         | Sum of `bits` must equal type width; fixed-point not allowed |
| `ref`              | `u16`, `u32`, `u64` | no                         | Resolves to absolute address of target; fixed-point not allowed |
| `checksum`         | `u32` only          | no                         | CRC over all preceding bytes in block; fixed-point not allowed |
| `fingerprint`      | `u64` only          | no                         | Injects a nameless ABI fingerprint for this or another same-file block |

#### Bitmap sub-field schema

Each element in the `bitmap` array:

| Key     | Type         | Description                                              |
| ------- | ------------ | -------------------------------------------------------- |
| `bits`  | integer (>0) | Number of bits this sub-field occupies                   |
| `name`  | string       | Data source lookup key (mutually exclusive with `value`) |
| `value` | scalar       | Literal value (mutually exclusive with `name`)           |

Fields pack LSB-first. Signed parent types use two's complement for negative sub-field values.

### Alignment behavior

Leaves are naturally aligned to their storage width:

- `u8`/`i8`: 1-byte aligned (no padding)
- `u16`/`i16`: 2-byte aligned
- `u32`/`i32`/`f32`: 4-byte aligned
- `u64`/`i64`/`f64`: 8-byte aligned
- fixed-point aligns to its storage width (`uq8.8` = 2-byte aligned, `q15.16` = 4-byte aligned)

Each dotted-path branch aligns to the maximum alignment of its children. Children retain their parsed order, each branch receives tail padding before its next sibling, and the root data struct is padded to its aggregate alignment. Gaps and tail padding use the block's `padding` byte. This alignment is always applied — mint does not support packed structs (`__attribute__((packed))`, `#pragma pack(1)`, etc.).

Strings use `u8` or `u16` storage. Each UTF-8 byte occupies one scalar element and is zero-extended in ABI byte order, so `size = N` counts `N` elements rather than Unicode code points. C28x strings use `type = "u16"`, one byte per 16-bit word.

---

## TOML layout and generated C header

Define the memory shape in the layout file:

```toml
[mint]
abi = "generic-le"

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
checksum = { checksum = "crc32", type = "u32" }

# --- data_t at 0x8100 ---
[data.header]
start_address = 0x8100
length = 0x100

[data.data]
schema = { fingerprint = true, type = "u64" }
config_schema = { fingerprint = "config", type = "u64" }
counter = { name = "Counter", type = "u64" }
message = { value = "Hello", type = "u8", size = 16 }
ip = { value = [192, 168, 1, 1], type = "u8", size = 4 }
checksum = { checksum = "crc32", type = "u32" }
```

Generate the corresponding C typedefs, array extent macros, and named bitmap shift/mask macros from that layout:

```bash
mint header layout.toml -o layout.h
mint fingerprint layout.toml
```

Key observations:

- Dotted paths (`device.id`, `device.name`) reproduce the struct nesting.
- `type = "u8", size = 16` generates a `uint8_t` array using a reusable `_LEN` macro.
- The bitmap's total bits (1+3+4+8 = 16) match the `u16` type width.
- `gain_q8_8` stores `1.5` as a Q8.8 fixed-point value in a `uint16_t`-sized slot.
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

Ref targets are dotted paths rooted at the block's data section and are validated before field values are emitted. `ref = "table"` resolves to the address of the branch containing `table.entries`. Forward and backward refs both work. Cross-block refs are not supported.

Resolved address: `start_address + field_offset_octets / address_unit_octets`. The address must fit the ref's `u16`, `u32` or `u64` storage type.

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
- Empty cells fall through to the next variant in the `--variants` priority chain.
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
mint build layout.toml --xlsx data.xlsx --variants Default -o firmware.hex

# Specific block, JSON source, variant fallback chain
mint build layout.toml#config --json data.json --variants Production/Default -o config.hex

# Multiple blocks from same file
mint build layout.toml#config layout.toml#data --xlsx data.xlsx --variants Default -o combined.hex

# Production build with safety checks and stats
mint build layout.toml --xlsx data.xlsx --variants Production/Default -o release/fw.hex --strict --stats

# Motorola S-Record output
mint build layout.toml --xlsx data.xlsx --variants Default -o firmware.mot --format mot

# Matching C header, with no data source
mint header layout.toml -o layout.h
```

## Starting from scratch checklist

When creating a mint layout for a new project:

1. **Identify the structs** — find the C headers defining flash-resident data structures.
2. **Get the memory map** — `start_address` and `length` for each block from the linker script or flash layout docs.
3. **Select the ABI profile** — match the target's byte order and layout rules; inspect choices with `mint abi list` and `mint abi show ABI`.
4. **Check for CRC requirements** — get the polynomial, initial value, XOR-out, and reflection settings from the firmware's CRC validation code.
5. **Choose padding byte** — usually `0xFF` (erased NOR flash) or `0x00`. Check what the firmware/bootloader expects in unused regions.
6. **Map each struct field** to a TOML entry, choosing `value` for constants or `name` for data-source-driven values.
7. **Set up the data source** — create the Excel workbook or JSON file with all the `name` keys the layout references.
8. **Generate the header** — run `mint header layout.toml -o layout.h` so firmware consumes the layout-owned struct shape.
9. **Verify** — build with `--stats` to confirm block sizes and checksums match expectations. Use `--strict` to catch type conversion issues early.
