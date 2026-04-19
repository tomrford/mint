# Header Generation

This document captures the proposed C header generation feature for mint layouts.

## Goal

Generate a C header from a mint layout file so that the layout file remains the source of truth and the header becomes a checked projection of that schema.

## Non-Goals

- Parsing C into mint layouts
- Matching compiler-specific packed-layout behavior
- Generating C bitfields that depend on compiler ABI rules
- Building a general code generator for multiple languages in V1

## Motivation

mint already models structured memory layouts well enough to describe C-like data structures.

The current docs maintain a hand-written example header alongside the example TOML. That header has already drifted from the TOML, which is exactly the kind of problem generation should solve.

## CLI Shape

mint does not currently have subcommands, so V1 should use a flag.

Recommended flag:

```text
--export-header <FILE>
```

Behavior:

- parse layouts normally
- emit the generated header to the requested path
- generation should not require a data source
- generation can happen alongside normal build output

Possible future behavior:

- if `--export-header` is set without a data source, allow schema export without requiring a buildable datasource configuration

## Scope for V1

Generate:

- one `typedef struct` per block
- nested anonymous structs from dotted paths
- scalar fields
- array fields from `size` and `SIZE`
- integer storage fields for bitmap entries
- integer storage fields for checksum entries
- integer storage fields for ref entries
- constants for block addresses and lengths
- constants for array sizes
- constants for bitmap masks and shifts where names are available

Do not generate:

- C bitfields
- compiler-specific attributes
- packed pragmas
- enums or named constants beyond what already exists in the layout

## Source of Truth

The TOML layout is canonical.

The generated header should reflect mint's layout semantics, not a target compiler ABI.

That means:

- field order follows the layout file
- array extents come from `size` and `SIZE`
- nested structs come from dotted paths
- bitmap fields remain integer storage with helper macros

If exact binary layout checking is needed later, the generator can emit explicit pad members derived from mint's alignment rules.

## Example Output Shape

For a block like:

```toml
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
```

V1 could emit:

```c
#define CONFIG_START_ADDRESS 0x8000u
#define CONFIG_LENGTH 0x100u

#define CONFIG_DEVICE_NAME_LEN 16u

#define CONFIG_FLAGS_ENABLEDEBUG_SHIFT 0u
#define CONFIG_FLAGS_ENABLEDEBUG_MASK  0x0001u
#define CONFIG_FLAGS_REGIONCODE_SHIFT  4u
#define CONFIG_FLAGS_REGIONCODE_MASK   0x00F0u

typedef struct {
  struct {
    uint32_t id;
    uint8_t name[CONFIG_DEVICE_NAME_LEN];
  } device;
  uint16_t version;
  uint16_t flags;
} config_t;
```

## Naming

V1 should define deterministic naming rules and document them.

Recommended rules:

- block `config` -> `config_t`
- nested path `device.name` -> nested struct `device`, member `name`
- constants use uppercase snake case from block name + field path

Examples:

- `config.header.start_address` -> `CONFIG_START_ADDRESS`
- `config.data.device.name size = 16` -> `CONFIG_DEVICE_NAME_LEN`
- `config.data.matrix size = [2, 2]` -> `CONFIG_MATRIX_ROWS`, `CONFIG_MATRIX_COLS`

## Checksums and Refs

These should be emitted as their storage field types.

Examples:

- `checksum = { checksum = "crc32", type = "u32" }` -> `uint32_t checksum;`
- `ptr = { ref = "table.entries", type = "u32" }` -> `uint32_t ptr;`

The generated header should describe storage, not runtime computation.

## Bitmap Macros

Bitmap storage should stay as a plain integer field in V1.

Example:

```c
uint16_t flags;
```

Emit helper macros for named bitmap members:

- shift
- mask

This avoids compiler bitfield layout issues.

## Bitmap Naming Problem

Unnamed literal bitmap regions do not have meaningful names today.

V1 options:

1. Skip constants for unnamed bitmap spans.
2. Emit generic names like `RESERVED_0`.
3. Add explicit bitmap labels in the layout model later.

Recommendation:

- V1 should skip unnamed spans
- only emit macros for bitmap members backed by `name`

This avoids low-quality generated symbols.

## Exact Layout vs Structural Layout

There are two possible generator modes:

1. Structural header
2. Exact layout header

Structural header means:

- express the schema naturally in C
- do not insert synthetic pad members

Exact layout means:

- compute mint's implicit alignment
- insert generated pad members
- optionally emit `_Static_assert(sizeof(...))`
- optionally emit `offsetof(...)` assertions

Recommendation:

- V1 should start with structural generation
- exact-layout mode can come later if needed

The structural version is simpler and already solves drift for most users.

## Implementation Shape

This feature can be implemented as a schema walker over the already parsed layout representation.

High-level steps:

1. Parse the layout file into the existing config model.
2. Build a nested field tree from each block's dotted paths.
3. Map mint scalar types to C types.
4. Emit array dimensions from `size` and `SIZE`.
5. Emit storage fields for `bitmap`, `ref`, and `checksum`.
6. Emit block-level and field-level constants.
7. Write the header file.

## Type Mapping

Recommended V1 mapping:

- `u8` -> `uint8_t`
- `u16` -> `uint16_t`
- `u32` -> `uint32_t`
- `u64` -> `uint64_t`
- `i8` -> `int8_t`
- `i16` -> `int16_t`
- `i32` -> `int32_t`
- `i64` -> `int64_t`
- `f32` -> `float`
- `f64` -> `double`

If fixed-point types land first, decide whether generated headers should:

- emit storage integers only, or
- emit comments describing the fixed-point format

Recommendation:

- emit storage integer types
- add a comment with the fixed-point type string

## Current Example Drift

The example header should be corrected before or with generator work.

Today:

- the `config` checksum field is named `checksum` in TOML but `crc` in the header
- the `data` checksum field exists in TOML but is missing from the header

Generator work should treat the layout file as canonical and remove this drift.

## Tests

Minimum V1 test coverage:

- generate a header from `doc/examples/block.toml`
- assert that key typedefs and constants are present
- assert that size macros are emitted
- assert that bitmap mask and shift macros are emitted for named bitmap fields
- assert that unnamed bitmap spans do not produce junk macros

## Documentation Updates Needed

If implemented, update:

- `README.md` if feature is user-facing there
- `doc/cli.md`
- `doc/layout.md`
- `doc/examples/blocks.h` or replace it with generated output
- `skill/mint/references/examples.md`

## Recommendation

This is feasible, but it needs more design than fixed-point.

The cleanest V1 is:

- `--export-header <FILE>`
- structural C generation only
- constants for block size, block address, array dimensions, and named bitmap masks or shifts
- no packed-layout promises
- no C bitfields
