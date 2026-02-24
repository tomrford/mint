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

### Nix environment

Nix is installed single-user at `~/.nix-profile`. Source it before use:

```
. /home/ubuntu/.nix-profile/etc/profile.d/nix.sh
```

All cargo commands must be run through the flake dev shell:

```
nix develop -c cargo build
nix develop -c cargo test
nix develop -c cargo fmt --check
nix develop -c cargo clippy
```

### Running the full gate

```
nix develop -c cargo fmt --check
nix develop -c cargo clippy
nix develop -c cargo test
```

### Quick CLI smoke test

`simple_block` in `tests/data/blocks.toml` uses only inline values (no data source needed):

```
nix develop -c cargo run -- simple_block@tests/data/blocks.toml -o /tmp/out.hex --stats
```

### Postgres tests

The nix dev shell provides PostgreSQL. To run the 12 ignored Postgres integration tests:

```bash
# Init + start (only needed once per session)
nix develop -c initdb -D /workspace/.pg_data --no-locale --encoding=UTF8
nix develop -c pg_ctl -D /workspace/.pg_data -l /workspace/.pg_data/logfile -o "-k /tmp -h localhost" start
nix develop -c createdb -h localhost mint_test

# Run tests (must be single-threaded — parallel creates race on table DDL)
nix develop -c cargo test --test postgres -- --include-ignored --test-threads=1
```

To stop Postgres: `nix develop -c pg_ctl -D /workspace/.pg_data stop`

### HTTP tests

`tests/http.rs` (12 tests) are also `#[ignore]`; they require an external HTTP server and are not part of the standard gate.
