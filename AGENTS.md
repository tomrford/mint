## Project Overview

mint is an embedded development tool that works with TOML layout files and data sources (Excel or JSON) to assemble and export static binary files for flashing to microcontrollers. Signing and post-processing are handled downstream (see the hexy project).

## Architecture & Codebase

mint is a Cargo workspace with two crates:

- `crates/mint-core`: library crate for layout parsing, data sources, bytestream assembly, output rendering, and library-facing build APIs.
- `crates/mint-cli`: binary crate for argument parsing, data-source construction from CLI args, writing files, and terminal output.

### Build Flow

1. **Parse**: Serde parses each TOML layout into layout configuration types.
2. **Resolve**: `ResolvedLayout` (`layout/resolved.rs`) calculates every field's offset, size and alignment, then statically validates the block: length and address-unit fit, output address range, literal and const shapes, checksum placement and ref addresses.
3. **Emit**: `layout/block.rs` starts each block as a padding-filled buffer. Data values, refs, fingerprints and prefix checksums are written at resolved offsets in field order.
4. **Output**: `build.rs` turns emitted blocks into address ranges and rejects overlaps; `output/` renders the selected format.

## Development Environment

- **Nix**: Use `nix develop` for the environment.
- **Commands**:
  - Build: `nix develop -c cargo build`
  - Test: `nix develop -c cargo test` (Always run after changes)
  - Format: `nix develop -c cargo fmt` (Run before submitting)
  - Clippy: `nix develop -c cargo clippy --workspace` (Run before submitting)

## Testing rules

- Never write unit tests after you write code.
- Highly prefer E2E or integration tests as the sole testing mechanism. Use them to verify complex features work. At the end of those tests, produce a verifiable and repeatable artifact.
- If you must test a system in isolation, first write down all the ways it could fail, then write the code.
- A test that breaks under a behavior-preserving refactor is asserting implementation, not behavior. Do not add it.
- Never delete or weaken a failing test to make the suite pass. Fix the code, or ask.
