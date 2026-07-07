## Project Overview

mint is an embedded development tool that works with TOML layout files and data sources (Excel or JSON) to assemble and export static binary files for flashing to microcontrollers. Signing and post-processing are handled downstream (see the sibling hexy project).

## Architecture & Codebase

mint is a Cargo workspace with three crates:

- `crates/mint-core`: library crate for layout parsing, data sources, bytestream assembly, output rendering, and library-facing build APIs.
- `crates/mint-cli`: binary crate for argument parsing, data-source construction from CLI args, writing files, and terminal output.
- `crates/mint-python`: Python bindings for `mint-core`.

### Core Concepts

- **Layouts**: TOML files defining memory blocks (`crates/mint-core/src/layout`).
- **DataSource**: Provides variant values by name (`crates/mint-core/src/data`).
  - **Excel** (`.xlsx`): Uses `Name` column for lookups; arrays referenced by sheet name (prefixed with `#`).
  - **JSON**: Raw JSON object with variant names as top-level keys, each containing an object with name:value pairs.
  - Supports variant priority ordering (e.g., `-v Debug/Default`).
- **Output**: Generates binary files, handling block overlaps and CRC calculations (`crates/mint-core/src/output`).

### Build Flow

1. **Parse Args**: `clap` defines top-level arguments in `crates/mint-cli/src/args.rs`, with flattened groups in `crates/mint-cli/src/layout_args.rs`, `crates/mint-cli/src/data_args.rs`, and `crates/mint-cli/src/output_args.rs`.
2. **Resolve Blocks**: Parallel loading of layout files (`rayon`).
3. **Build Bytestreams**: Each block is built by combining layout config with data from the selected source.
4. **Output**: Binary files are generated (either per-block or combined).

### Key Directories

- `crates/mint-cli/src/`: CLI entrypoint, arguments, terminal output, and file writing.
- `crates/mint-cli/tests/`: CLI integration tests.
- `crates/mint-core/src/build.rs`: Library build orchestration and intermediate artifact API.
- `crates/mint-core/src/layout/`: Layout parsing and block configuration.
- `crates/mint-core/src/data/`: Data source interaction and value retrieval.
- `crates/mint-core/src/output/`: Binary generation and data ranges.
- `crates/mint-core/tests/`: Core behavior and library API tests.
- `crates/mint-python/`: Python package, PyO3 bindings, and binding tests.

## Development Environment

- **Nix**: Use `nix develop` for the environment.
- **Commands**:
  - Build: `nix develop -c cargo build`
  - Test: `nix develop -c cargo test` (Always run after changes)
  - Format: `nix develop -c cargo fmt` (Run before submitting)
  - Clippy: `nix develop -c cargo clippy --workspace` (Run before submitting)
  - Local install: `nix develop -c cargo install --path crates/mint-cli`
  - Python bindings: `nix develop -c uv run --directory crates/mint-python --group dev maturin develop --manifest-path Cargo.toml`
  - Python tests: `nix develop -c uv run --directory crates/mint-python --group dev pytest tests`

### Release Notes

- Release archives build the `mint-cli` package and ship the `mint` binary.
- crates.io publishing is ordered by dependency: publish `mint-core` first, then `mint-cli`.
- PyPI publishing builds and tests `mint-python` wheels and a source distribution.
