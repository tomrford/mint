# Layout Files

Layout files define memory blocks and their data fields. Supported formats: TOML, YAML, JSON. The data in the layout file helps mint understand the structure of the data, and how you want to represent the data in the output. Each block represents a contiguous region of memory (typically a single struct stored in a known location in flash). For an example of a block, see [`doc/examples/blocks.h`](doc/examples/blocks.h) and compare it to the layout file [`doc/examples/block.toml`](doc/examples/block.toml).

## Structure

```toml
[settings]          # Global settings (required)
# ...

[blockname.header]  # Block header (required per block)
# ...

[blockname.data]    # Block data fields (required per block)
# ...
```

---

## Settings

Global settings apply to all blocks. The `[settings.crc]` section defines default CRC parameters used when a block's `[header.crc]` doesn't override them.

```toml
[settings]
endianness = "little"      # "little" (default) or "big"
virtual_offset = 0x0       # Offset added to all addresses
word_addressing = false    # Enable for word-addressed memory (see below)

[settings.crc]             # Optional: only required if any block uses CRC
location = "end_data"      # CRC placement: "end_data", "end_block" - absolute address is not allowed here as this is a global setting
polynomial = 0x04C11DB7    # CRC polynomial
start = 0xFFFFFFFF         # Initial CRC value
xor_out = 0xFFFFFFFF       # XOR applied to final CRC
ref_in = true              # Reflect input bytes
ref_out = true             # Reflect output CRC
area = "data"              # CRC coverage: "data", "block_zero_crc", "block_pad_crc", or "block_omit_crc"
```

**CRC Area Options:**

- `data` - CRC covers only the data (padded to 4-byte alignment)
- `block_zero_crc` - Pad to full block, zero CRC bytes before calculation
- `block_pad_crc` - Pad to full block, include CRC bytes as padding value
- `block_omit_crc` - Pad to full block, exclude CRC bytes from calculation

**Word Addressing Mode:**

When `word_addressing = true`:

- Addresses in output are doubled (16-bit word addresses instead of byte addresses)
- `start_address`, `length`, and absolute CRC `location` values are expressed in word addresses (16-bit units)
- Block length in bytes becomes `length * 2`
- Byte pairs are swapped in the output to recreate the word-addressed byte order
- `u8` and `i8` types are not allowed (strings also blocked)
- `virtual_offset` is applied after doubling, so it is not doubled

---

## Block Header

Each block requires a header section defining memory layout. CRC is configured per-header via the optional `[blockname.header.crc]` section.

```toml
[blockname.header]
start_address = 0x8B000    # Start address in memory (required)
length = 0x1000            # Block size in addresses (bytes unless word_addressing=true)
padding = 0xFF             # Padding byte value (default: 0xFF)

[blockname.header.crc]     # Optional: enables CRC for this block
location = "end_data"      # CRC placement: "end_data", "end_block", or absolute address (optional)
polynomial = 0x04C11DB7    # Override global polynomial (optional)
start = 0xFFFFFFFF         # Override global start value (optional)
xor_out = 0xFFFFFFFF       # Override global xor_out (optional)
ref_in = true              # Override global ref_in (optional)
ref_out = true             # Override global ref_out (optional)
area = "data"              # Override global area (optional)
```

**CRC Location Options:**

- `"end_data"` - Append CRC as u32 after data (4-byte aligned - designed such that it lands in a u32 placed at the end of the struct that you're building in flash. Note that the CRC for this setting if the area is set to 'data' will include any padding up to the alignment of the CRC itself.)
- `"end_block"` - CRC in final 4 bytes of block
- `0x8BFF0` - Absolute address for CRC placement - must be within the block

Absolute CRC addresses use the same address units as `start_address` (word addresses when `word_addressing = true`).

To disable CRC for a block, simply omit the `[header.crc]` section.

**Per-Header CRC Overrides:**

Each header can override any CRC parameter from `[settings.crc]`. If a parameter is not specified in the header, the global value is used. If no global value exists and the header doesn't specify the value, an error occurs.

## Block Data

Data fields are key-value pairs where the key is a dotted path (matching C struct hierarchy) and the value defines the field.

### Field Attributes

| Attribute     | Description                                                                   |
| ------------- | ----------------------------------------------------------------------------- |
| `type`        | Data type (required)                                                          |
| `value`       | Literal value (mutually exclusive with `name`, `bitmap`, `ref`)               |
| `name`        | Data source lookup key (mutually exclusive with `value`, `bitmap`, `ref`)     |
| `bitmap`      | Bitmap field definitions (see below)                                          |
| `ref`         | Pointer to another field in the same block (see below)                        |
| `size`/`SIZE` | Array size; `size` pads if data is shorter, `SIZE` errors if data is shorter. |

---

## Field Examples

### Scalar Values

```toml
[block.data]
# Literal numeric
device.id = { value = 0x1234, type = "u32" }

# From data source
device.serial = { name = "SerialNumber", type = "u32" }

# Boolean (stored as integer)
config.enable = { value = true, type = "u8" }
```

### Strings

Strings use `u8` type with `size` for fixed-length fields.

```toml
[block.data]
# Literal string (padded to size)
message = { value = "Hello", type = "u8", size = 16 }

# From data source
device.name = { name = "DeviceName", type = "u8", size = 32 }
```

### Arrays

```toml
[block.data]
# 1D literal array
network.ip = { value = [192, 168, 1, 100], type = "u8", size = 4 }

# 1D from data source
calibration.coeffs = { name = "Coefficients1D", type = "f32", size = 8 }

# 2D array (e.g., 3x3 matrix)
calibration.matrix = { name = "CalibrationMatrix", type = "i16", size = [3, 3] }

# Strict size (error if data source has fewer elements)
strict.array = { name = "SomeArray", type = "f32", SIZE = 8 }
```

### Bitmaps

Pack multiple values into a single integer.

```toml
[block.data]
config.flags = { type = "u16", bitmap = [
    { bits = 1, name = "EnableDebug" },   # 1 bit from data source
    { bits = 3, name = "ModeSelect" },    # 3 bits from data source
    { bits = 1, value = true },           # 1 bit literal
    { bits = 4, name = "RegionCode" },    # 4 bits from data source
    { bits = 7, value = 0 },              # 7 bits padding
] }
```

Bitmap fields are packed LSB-first into the specified type. signedness of fields match the type. Negative values are represented as two's complement. The sum of the bits in the bitmap must match the type size.

### Refs (Pointers)

A `ref` entry resolves to the absolute memory address of another field within the same block. The ref target is a dotted path rooted at `block.data` — for example, `device.info.version` refers to `[block.data] device.info.version`. Refs can point to leaf fields or branch nodes (nested structs); a branch ref resolves to the address of the branch's first child (post-alignment).

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

- `ref` is mutually exclusive with `name`, `value`, and `bitmap`
- `type` must be an integer type (`u16`, `u32`, `u64`, `i16`, `i32`, `i64`)
- `size`/`SIZE` cannot be used with `ref`
- The target path must exist within the same block — cross-block refs are not supported
- The resolved address is `start_address + virtual_offset + target_offset` (with word-addressing multipliers applied when `word_addressing = true`)
- Refs can reference fields defined before or after the ref in the layout (forward and backward refs are both supported)

---

## Multiple Blocks

A single layout file can define multiple blocks:

```toml
[settings]
endianness = "little"

[settings.crc]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true
area = "data"

[config.header]
start_address = 0x8000
length = 0x1000

[config.header.crc]
location = "end_data"

[config.data]
version = { value = 1, type = "u16" }

[calibration.header]
start_address = 0x9000
length = 0x1000

[calibration.header.crc]
location = "end_data"
polynomial = 0x1EDC6F41    # Different CRC polynomial for this block

[calibration.data]
coefficients = { name = "Coefficients", type = "f32", size = 16 }
```

Build specific blocks with `blockname@file.toml` syntax:

```bash
mint config@layout.toml --xlsx data.xlsx -v Default
```

---

## Format Examples

### TOML

```toml
[block.header]
start_address = 0x8000
length = 0x100

[block.data]
device.id = { value = 0x1234, type = "u32" }
device.name = { name = "DeviceName", type = "u8", size = 16 }
```

### YAML

```yaml
block:
  header:
    start_address: 0x8000
    length: 0x100
  data:
    device.id:
      value: 0x1234
      type: "u32"
    device.name:
      name: "DeviceName"
      type: "u8"
      size: 16
```

### JSON

```json
{
  "block": {
    "header": {
      "start_address": 32768,
      "length": 256
    },
    "data": {
      "device.id": { "value": 4660, "type": "u32" },
      "device.name": { "name": "DeviceName", "type": "u8", "size": 16 }
    }
  }
}
```
