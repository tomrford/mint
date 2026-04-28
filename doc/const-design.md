# Const handling — design notes

Status: implemented on this branch.
Branch: `claude/expand-cross-block-refs-VsaDF`.

## Problem

We want to reuse a single value (a block's start address, a shared scalar
literal, etc.) in more than one place — e.g. block A's start address embedded
in block B's data.

Cross-block refs were considered first and rejected: resolving a ref into
another block requires walking that block's data tree, which forces work for
blocks the user did not ask to build. Consts sidestep this because they are
known from layout *parse* alone, independent of which blocks are being built.

## Decided shape

### `[mint.const]` table

User-declared consts, sibling of `[mint.checksum]`:

```toml
[mint.const]
default_voltage = 3.3
fw_name         = "BootloaderV2"
magic           = 0xDEADBEEF
ip_octets       = [192, 168, 1, 10]
"app.length"    = 0x4000
```

Values use the same shape accepted by leaf `value = ...`: scalar literals,
strings, booleans, and one-dimensional arrays. Conversion to the leaf's
storage type happens at use site through the same emission path `value =`
already uses.

The table is one level deep. Nested tables under `[mint.const]` are rejected.
Const names containing dots are allowed only as flat string keys, e.g.
`"app.length" = 0x4000`.

### Implicit promotion of header values

Every block in a config auto-exports two entries into the same lookup table:

- `<block_name>.start_address` — `u32` from `block.header.start_address`
- `<block_name>.length` — `u32` from `block.header.length`

Header fields stay strict `u32` — no const syntax inside headers. Implicit
promotion is precisely what allows that: if you want one block to reference
another block's bounds, you do it from the *leaf* side using the auto-exported
name; you don't need to put a const in the header itself.

Auto-promoted addresses are real device addresses. They do not include
`[mint].virtual_offset`. The virtual offset is applied only when mint writes
output ranges, so the emitted HEX/S-record can use remapped transport
addresses while data embedded for firmware consumption still points at real
memory.

### Leaf entry usage

New `EntrySource` variant, used like:

```toml
peer_base = { type = "u32", const = "main.start_address" }
peer_size = { type = "u32", const = "main.length" }
voltage   = { type = "f32", const = "default_voltage" }
fw_label  = { type = "u8",  size = 32, const = "fw_name" }
ip        = { type = "u8",  size = 4, const = "ip_octets" }
```

One syntax for both implicit (`block.field`) and explicit (`name`) consts;
no `CONST[...]` wrapper.

### Validation

- `const = ""` rejected (empty name).
- Unknown name → error listing available const names.
- Value/storage-type mismatch → existing `value =` errors apply (range
  overflow, signedness, fixed-point unsupported, string/array size mismatch,
  etc.).
- `const` is mutually exclusive with `name` / `value` / `bitmap` / `ref` /
  `checksum` (already enforced by the `EntrySource` enum shape).
- `size` / `SIZE` with `const` follows the same rules as `value =`: strings
  and one-dimensional arrays require a one-dimensional size, scalar values
  reject size, and two-dimensional sizes are unsupported for consts.
- Nested `[mint.const]` values are rejected. Dotted implicit names are flat
  lookup strings, not nested table paths.
- Collision check: a user `[mint.const]` key matching an auto-promoted name
  (`foo.start_address` / `foo.length` for any block named `foo` in the same
  config) → error at config-load time.

## Resolution model

Three small staged steps after layout parse, before block build:

1. Capture user-declared `[mint.const]` entries into a single table on
   `MintConfig` (e.g. `#[serde(rename = "const")] pub consts:
   HashMap<String, ValueSource>`). Nested values fail serde deserialization
   because they do not match `ValueSource`.
2. For each block in the config, insert `<block_name>.start_address` and
   `<block_name>.length` into the same table as `ValueSource::Single`
   `DataValue::U64` values. Error on collision with step 1. Start address
   uses the raw header address, without `virtual_offset`.
3. During leaf emission, `EntrySource::Const(name)` looks up the table on
   `&MintConfig` and dispatches through the same scalar/string/array path
   used by `EntrySource::Value`. No fixup pass; no `pending_*` state.

Builds remain fully parallel — the table is read-only by the time block
building starts.

### Ref address model

Refs also resolve to real device addresses, not virtualized output addresses.
`ref = "target"` should encode `block.header.start_address + target_offset`
only. `[mint].virtual_offset` remains an output formatting concern and is
applied by `bytestream_to_datarange`.

## Code changes

- `src/layout/settings.rs`
  - Add `#[serde(rename = "const", default)] pub consts:
    HashMap<String, ValueSource>` on `MintConfig`.
  - Keep `ValueSource` as the const value type so consts are a true
    alternative to `value =`.
- `src/layout/entry.rs`
  - Add `EntrySource::Const(String)` next to `Ref` / `Checksum`.
  - Add `validate_const` mirroring `validate_ref` / `validate_checksum`
    (empty-name check, unknown-name error, size-key rules per "Validation"
    above).
  - Update `emit_bytes`, `emit_bytes_single`, and `emit_bytes_1d` so
    `EntrySource::Const` reuses the existing `ValueSource::Single` /
    `ValueSource::Array` emission behavior.
  - Return the same unsupported-2D error for `const` that `value =` returns
    for two-dimensional sizes.
- `src/layout/block.rs`
  - Pass `&MintConfig` into leaf emission so const lookup happens in
    `LeafEntry`.
  - Change `resolve_pending_refs` to encode
    `header.start_address + target_offset`, without adding
    `settings.virtual_offset`.
- `src/layout/mod.rs` (or new helper near `load_layout`)
  - Post-parse pass: iterate `Config.blocks`, insert auto-promoted entries
    into `cfg.mint.consts`, error on collision with user keys.
  - Runs once per loaded layout file, immediately after `try_into::<Config>`.
- Tests
  - Leaf consuming a `[mint.const]` literal of each `value =` shape: bool,
    int, float, string with `size`, and one-dimensional array with `size`.
  - Leaf consuming an auto-promoted `<block>.start_address` / `<block>.length`.
  - Collision: user const named `foo.start_address` while block `foo` exists
    → error.
  - Unknown const name → error message lists available names.
  - Type mismatch (e.g. const string into a `u32` leaf) → existing error.
  - Const used with scalar plus `size`/`SIZE` → error.
  - Nested `[mint.const]` tables → error.
  - Ref encodes the raw block address when `[mint].virtual_offset` is set;
    output `DataRange.start_address` still includes the virtual offset.

## Open question — multi-file `[mint.const]` merge

**Recommendation: per-file, no merge. Match the existing model.**

Rationale from the code shape:

- `src/commands/mod.rs` loads layouts into `HashMap<String, Config>` keyed by
  filename and calls `build_bytestream(&layout.mint, ...)` against each file's
  own `MintConfig`. There is no cross-file merge today — `mint.checksum`,
  `mint.endianness`, and `mint.virtual_offset` are all per-file.
- Per-file consts require zero new infrastructure: the post-parse pass already
  runs per file, and `build_bytestream` already receives the file-local
  `MintConfig`.
- A union-merge step would be a new concept in the codebase, would need
  duplicate-key resolution rules across files, and would couple files that
  are otherwise independent.

Trade-off accepted: a block in file A cannot reference a const (or block
header) defined in file B. If that case appears, the user can either move
the consumer into the same file or declare the value in both. Cross-file
union remains a clean future extension if real demand emerges.

If during implementation it turns out parts of the codebase already imply
cross-file sharing (e.g. shared `[mint]` settings expected to align across
files), revisit this — but on current evidence per-file is both simpler and
more consistent.

## Out of scope for v1

- `block.end_address` (start + length) auto-export. Useful but not yet
  needed; trivial to add later when the table-population helper exists.
- `padding` auto-export.
- Const-of-const chaining (a `[mint.const]` entry whose value names another
  const). Keep entries flat literals; relax later if needed.
- Const usage inside `[block.header]` fields. Headers stay strict `u32`.
- 2D array consts. Single values and 1D arrays only, matching inline
  `value =`.
- Cross-file const resolution (see open question above).
