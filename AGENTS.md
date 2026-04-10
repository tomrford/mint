## Project Overview

mint is an embedded development tool that works with TOML layout files and data sources (Excel or JSON) to assemble, export, sign (and more) static binary files for flashing to microcontrollers. YAML layouts are parser-compatible but not the primary documented workflow.

## Architecture & Codebase

### Core Concepts

- **Layouts**: TOML/YAML files defining memory blocks (`src/layout`).
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
