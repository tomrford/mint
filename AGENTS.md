## Project Overview

mint is an embedded development tool that works with TOML layout files and data sources (Excel or JSON) to build flash blocks (Intel HEX or Motorola S-Record) for microcontrollers, with matching C headers and ABI fingerprints. Signing and post-processing are handled downstream (see the hexy project).

## Architecture & Codebase

mint is a Cargo workspace with two crates:

- `crates/mint-core`: library crate for layout parsing, data sources, bytestream assembly, output rendering, and library-facing build APIs.
- `crates/mint-cli`: binary crate for argument parsing, data-source construction from CLI args, writing files, and terminal output.

### Build Flow

1. **Parse**: Serde parses each TOML layout into layout configuration types.
2. **Preflight**: before any command work, `layout/plan.rs` runs `ResolvedLayout` (`layout/resolved.rs`) on every selected block to calculate each field's offset, size and alignment, statically validate the block (length and address-unit fit, output address range, literal and const shapes, checksum placement and ref addresses) and fingerprint it and its fingerprint targets. Build, header and fingerprint all start from this plan.
3. **Emit**: `layout/block.rs` starts each block as a padding-filled buffer. Data values, refs, fingerprints and prefix checksums are written at resolved offsets in field order.
4. **Output**: `build.rs` turns emitted blocks into address ranges and rejects overlaps; `output/` renders the selected format.

### Documentation

The layout, data-source and command-line reference lives in `crates/mint-cli/doc/` so the binary can embed it: `mint skill <topic>` prints those pages and `mint skill` prints the agent guide in `crates/mint-cli/skill/mint/SKILL.md`. Each fact belongs in one page; the guide covers workflow and pitfalls and points at the reference instead of restating it.

## Development Environment

- Run `cargo test --workspace` after changes, and `cargo fmt` and `cargo clippy --workspace --all-targets --all-features` before submitting. `nix develop` provides a matching toolchain but nothing else requires it.
- The ABI compiler checks need Nix on x86_64 Linux: `nix build .#checks.x86_64-linux.abi-<name>` compiles generated headers with each target compiler (see `flake.nix` for the check names). TriCore compilers are licensed, so `tests/abi/check-tricore.sh` runs the same probe, plus `tests/abi/bytes.toml`, with a compiler given through `TRICORE_GCC` and `TRICORE_FLAGS`. Update the ABI table in `crates/mint-cli/doc/layout.md` when a compiler result changes.

## Testing rules

- Never write unit tests after you write code.
- Highly prefer E2E or integration tests as the sole testing mechanism. Use them to verify complex features work. At the end of those tests, produce a verifiable and repeatable artifact.
- If you must test a system in isolation, first write down all the ways it could fail, then write the code.
- A test that breaks under a behavior-preserving refactor is asserting implementation, not behavior. Do not add it.
- Never delete or weaken a failing test to make the suite pass. Fix the code, or ask.
