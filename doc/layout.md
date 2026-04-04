# Layout Files

Layout files define memory blocks and their data fields. TOML is the canonical layout format and all examples in these docs use it. YAML layouts are still accepted by the parser as a compatibility format, but TOML is the primary workflow. JSON layout files are no longer supported. The data in the layout file helps mint understand the structure of the data, and how you want to represent the data in the output. Each block represents a contiguous region of memory (typically a single struct stored in a known location in flash). For an example of a block, see [`doc/examples/blocks.h`](doc/examples/blocks.h) and compare it to the layout file [`doc/examples/block.toml`](doc/examples/block.toml).

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
endianness = "little"      # "little" (default) or "big"
virtual_offset = 0x0       # Offset added to all addresses
word_addressing = false    # Enable for word-addressed memory (see below)

[mint.checksum.crc32]      # Named checksum config (can define multiple)
polynomial = 0x04C11DB7    # CRC polynomial
start = 0xFFFFFFFF         # Initial CRC value
xor_out = 0xFFFFFFFF       # XOR applied to final CRC
ref_in = true              # Reflect input bytes
ref_out = true             # Reflect output CRC
```

Multiple named checksum configurations can be defined (e.g., `[mint.checksum.crc32]`, `[mint.checksum.crc32c]`). Each is referenced by name in block data fields.

**Word Addressing Mode:**

When `word_addressing = true`:

- Addresses in output are doubled (16-bit word addresses instead of byte addresses)
- `start_address` and `length` values are expressed in word addresses (16-bit units)
- Block length in bytes becomes `length * 2`
- Byte pairs are swapped in the output to recreate the word-addressed byte order
- `u8` and `i8` types are not allowed (strings also blocked)
- `virtual_offset` is applied after doubling, so it is not doubled

---

## Block Header

Each block requires a header section defining the memory region.

```toml
[blockname.header]
start_address = 0x8B000    # Start address in memory (required)
length = 0x1000            # Block size in addresses (bytes unless word_addressing=true)
padding = 0xFF             # Padding byte value (default: 0xFF)
```

---

## Block Data

Data fields are key-value pairs where the key is a dotted path (matching C struct hierarchy) and the value defines the field.

### Field Attributes

| Attribute     | Description                                                                   |
| ------------- | ----------------------------------------------------------------------------- |
| `type`        | Data type (required)                                                          |
| `value`       | Literal value (mutually exclusive with `name`, `bitmap`, `ref`, `checksum`)   |
| `name`        | Data source lookup key (mutually exclusive with `value`, `bitmap`, `ref`, `checksum`) |
| `bitmap`      | Bitmap field definitions (see below)                                          |
| `ref`         | Pointer to another field in the same block (see below)                        |
| `checksum`    | Inline checksum referencing a named config (see below)                        |
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

- `ref` is mutually exclusive with `name`, `value`, `bitmap`, and `checksum`
- `type` must be an integer type (`u16`, `u32`, `u64`, `i16`, `i32`, `i64`)
- `size`/`SIZE` cannot be used with `ref`
- The target path must exist within the same block — cross-block refs are not supported
- The resolved address is `start_address + virtual_offset + target_offset` (with word-addressing multipliers applied when `word_addressing = true`)
- Refs can reference fields defined before or after the ref in the layout (forward and backward refs are both supported)

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

The CRC covers all bytes from the start of the block data up to (but not including) the checksum field itself. The checksum is computed after all other fields (including refs) are resolved.

**Checksum rules:**

- `checksum` is mutually exclusive with `name`, `value`, `bitmap`, and `ref`
- `type` must be `u32` (matching CRC-32 output width)
- `size`/`SIZE` cannot be used with `checksum`
- Only one checksum per block is allowed
- The referenced config name must exist in `[mint.checksum]`
- For more complex checksum operations (cross-block CRC, multiple checksums, non-CRC algorithms), use a dedicated hex post-processing tool

---

## Multiple Blocks

A single layout file can define multiple blocks, each with its own checksum configuration:

```toml
[mint]
endianness = "little"

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

[calibration.header]
start_address = 0x9000
length = 0x1000

[calibration.data]
coefficients = { name = "Coefficients", type = "f32", size = 16 }
checksum = { checksum = "crc32c", type = "u32" }
```

Build specific blocks with `file.toml#blockname` syntax:

```bash
mint layout.toml#config --xlsx data.xlsx -v Default
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
```

YAML layouts remain accepted, but TOML is the only format documented and recommended for authored layouts. JSON is reserved for data input.
