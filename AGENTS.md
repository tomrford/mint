## Project Overview

mint is an embedded development tool that works with TOML layout files and data sources (Excel or JSON) to assemble, export, sign (and more) static binary files for flashing to microcontrollers. YAML layouts are parser-compatible but not the primary documented workflow.

## Architecture & Codebase

### Core Concepts

- **Layouts**: TOML/YAML/JSON files defining memory blocks (`src/layout`).
- **DataSource**: Provides variant values by name (`src/data`).
  - **Excel** (`.xlsx`): Uses `Name` column for lookups; arrays referenced by sheet name (prefixed with `#`).
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
nix develop -c cargo run -- tests/data/blocks.toml#simple_block -o /tmp/out.hex --stats
```
