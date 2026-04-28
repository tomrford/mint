# Const handling — design notes

Status: pre-implementation, awaiting second pass.
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

User-declared scalar/string consts, sibling of `[mint.checksum]`:

```toml
[mint.const]
default_voltage = 3.3
fw_name         = "BootloaderV2"
magic           = 0xDEADBEEF
```

Values are TOML scalars; conversion to the leaf's storage type happens at
use site through the same path `value =` already uses (`DataValue::to_bytes`).

### Implicit promotion of header values

Every block in a config auto-exports two entries into the same lookup table:

- `<block_name>.start_address` — `u32` from `block.header.start_address`
- `<block_name>.length` — `u32` from `block.header.length`

Header fields stay strict `u32` — no const syntax inside headers. Implicit
promotion is precisely what allows that: if you want one block to reference
another block's bounds, you do it from the *leaf* side using the auto-exported
name; you don't need to put a const in the header itself.

### Leaf entry usage

New `EntrySource` variant, used like:

```toml
peer_base = { type = "u32", const = "main.start_address" }
peer_size = { type = "u32", const = "main.length" }
voltage   = { type = "f32", const = "default_voltage" }
fw_label  = { type = "u8",  size = 32, const = "fw_name" }
```

One syntax for both implicit (`block.field`) and explicit (`name`) consts;
no `CONST[...]` wrapper.

### Validation

- `const = ""` rejected (empty name).
- Unknown name → error listing available const names.
- Value/storage-type mismatch → existing `DataValue::to_bytes` errors apply
  (range overflow, signedness, fixed-point unsupported, etc.).
- `const` is mutually exclusive with `name` / `value` / `bitmap` / `ref` /
  `checksum` (already enforced by the `EntrySource` enum shape).
- `size` / `SIZE` allowed with `const` only when the resolved value is a
  string (paralleling how strings work under `value =`). For scalar consts
  the entry is single-valued and `size` is rejected.
- Collision check: a user `[mint.const]` key matching an auto-promoted name
  (`foo.start_address` / `foo.length` for any block named `foo` in the same
  config) → error at config-load time.

## Resolution model

Three small staged steps after layout parse, before block build:

1. Capture user-declared `[mint.const]` entries into a single table on
   `MintConfig` (e.g. `pub const_table: HashMap<String, ConstValue>`).
2. For each block in the config, insert `<block_name>.start_address` and
   `<block_name>.length` into the same table. Error on collision with step 1.
3. During `build_bytestream_inner`, leaf entries with
   `EntrySource::Const(name)` look up the table on `&MintConfig` (already
   threaded through) and dispatch through the same scalar-conversion path
   used by `EntrySource::Value(Single)`. No fixup pass; no `pending_*` state.

Builds remain fully parallel — the table is read-only by the time block
building starts.

## Code changes

- `src/layout/settings.rs`
  - Add `pub const_table: HashMap<String, ConstValue>` on `MintConfig` (or
    `#[serde(rename = "const")] pub consts: ...`; pick a field name that
    does not shadow the Rust keyword).
  - Define `ConstValue` — minimal enum over the `DataValue` shapes accepted
    in `value =` (integer, float, string). Reuse `DataValue` directly if the
    serde shape is compatible; otherwise a thin wrapper.
- `src/layout/entry.rs`
  - Add `EntrySource::Const(String)` next to `Ref` / `Checksum`.
  - Add `validate_const` mirroring `validate_ref` / `validate_checksum`
    (empty-name check, size-key rules per "Validation" above).
  - Update `emit_bytes_*` arms to dispatch `Const` correctly (most likely
    handled at one site that resolves the const to a `DataValue` then falls
    through to existing single/array logic).
- `src/layout/block.rs`
  - In `build_bytestream_inner` leaf branch, before the existing
    `EntrySource::Ref` / `EntrySource::Checksum` early returns: handle
    `EntrySource::Const` by looking up against `&MintConfig.const_table`
    and emitting through the standard path (no pending state).
  - Pass `&MintConfig` (already available via `settings`) to wherever the
    lookup happens.
- `src/layout/mod.rs` (or new helper near `load_layout`)
  - Post-parse pass: iterate `Config.blocks`, insert auto-promoted entries
    into `cfg.mint.const_table`, error on collision with user keys.
  - Runs once per loaded layout file, immediately after `try_into::<Config>`.
- Tests
  - Leaf consuming a `[mint.const]` literal of each scalar shape (int,
    float, string with `size`).
  - Leaf consuming an auto-promoted `<block>.start_address` / `<block>.length`.
  - Collision: user const named `foo.start_address` while block `foo` exists
    → error.
  - Unknown const name → error message lists available names.
  - Type mismatch (e.g. const string into a `u32` leaf) → existing error.
  - Const used with conflicting `size`/`SIZE` keys for non-string → error.

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
- 2D array consts. Single values and 1D strings only, matching `value =`.
- Cross-file const resolution (see open question above).
