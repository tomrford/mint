## Project Overview

mint is an embedded development tool that works with layout files (toml/yaml/json) and data sources (Excel, Postgres, or REST) to assemble, export, sign (and more) static binary files for flashing to microcontrollers.

## Architecture & Codebase

### Core Concepts

- **Layouts**: TOML/YAML/JSON files defining memory blocks (`src/layout`).
- **DataSource**: Provides variant values by name (`src/data`).
  - **Excel** (`.xlsx`): Uses `Name` column for lookups; arrays referenced by sheet name (prefixed with `#`).
  - **Postgres**: JSON config with `url` and `query_template`; query returns JSON object per variant.
  - **REST**: JSON config with `url` (using `$1` placeholder) and optional `headers`; response must be JSON object per variant.
  - **JSON**: Raw JSON object with variant names as top-level keys, each containing an object with name:value pairs.
  - Supports variant priority ordering (e.g., `-v Debug/Default`).
- **Output**: Generates binary files, handling block overlaps and CRC calculations (`src/output`).

### Build Flow

1. **Parse Args**: `clap` defines arguments in `src/args.rs`.
2. **Resolve Blocks**: Parallel loading of layout files (`rayon`).
3. **Build Bytestreams**: Each block is built by combining layout config with data from the selected source.
4. **Output**: Binary files are generated (either per-block or combined).

### Key Directories

- `src/commands/`: Command implementations (e.g., `build`).
- `src/layout/`: Layout parsing and block configuration.
- `src/data/`: Data source interaction and value retrieval.
- `src/output/`: Binary generation and data ranges.

## Development Environment

- **Nix**: Use `nix develop` for the environment.
- **Commands**:
  - Build: `cargo build`
  - Test: `cargo test` (Always run after changes)
  - Format: `cargo fmt` (Run before submitting)
  - Clippy: `cargo clippy` (Run before submitting)

## Working Guidelines

- **Minimal Changes**: Do only what is asked. Aim to keep changes minimal and focused, and reuse existing code and patterns when possible.
- **Clarification**: Ask if goals are unclear - lay out a clear plan and get feedback before implementing anything.
- **Comments**: No "history" comments (e.g., "changed x to y"). Document current state only.
- **Compatibility**: Do not maintain backwards compatibility unless trivially possible or explicitly requested. Focus on better functionality and cleaner code.
- **Documentation**: functions and structs should be documented with succinct doc comments. Keep documentation (including readme) up to date with the code.
- **Testing**: Add at least unit test and one integration test for each new feature/functionality addition.

## Cursor Cloud specific instructions

### Rust toolchain

The project uses `edition = "2024"` which requires **Rust >= 1.85**. The VM update script installs the latest stable toolchain via `rustup`. Nix is not available in Cloud; all cargo commands run directly.

### Running the full gate

```
cargo fmt --check
cargo clippy
cargo test
```

### Quick CLI smoke test

`simple_block` in `tests/data/blocks.toml` uses only inline values (no Excel/Postgres needed):

```
cargo run -- simple_block@tests/data/blocks.toml -o /tmp/out.hex --stats
```

For Excel integration, use the committed `tests/data/data.xlsx`:

```
cargo run -- block@tests/data/blocks.toml --xlsx tests/data/data.xlsx -v Default -o /tmp/out.hex --stats
```

### Ignored tests

- `tests/postgres.rs` (12 tests): ignored by default; require a running Postgres instance.
- `tests/http.rs` (12 tests): ignored by default; require a running HTTP server.

These are not required for the standard development gate.
