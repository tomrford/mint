---
name: mint
description: "Guide for mint, which builds Intel HEX or Motorola S-Record flash blocks from TOML layouts and Excel or JSON data, and generates matching C headers and ABI fingerprints. Use it whenever a project uses mint or mint-cli, has TOML layouts describing flash or firmware memory blocks, needs configuration or calibration blocks built from structured data, or wants to replace a hand-written hex generator."
---

# mint

mint turns a declarative TOML layout (one block per C struct stored at a known flash address) and an optional Excel or JSON data source into an Intel HEX or Motorola S-Record file. From the same layout it generates the C header firmware compiles against and a 64-bit fingerprint of each block's ABI, so firmware and data cannot silently disagree about struct shape.

## Reference

This guide does not repeat the reference. Print a topic before writing or changing anything it covers:

- `mint skill layout`: layout syntax, ABI profiles, field types and sources, refs, checksums, validation, generated headers and fingerprints.
- `mint skill sources`: Excel and JSON data sources and variant fallback.
- `mint skill cli`: block selectors, outputs, statistics and build-system integration.
- `mint <command> --help` for options, `mint abi list` for ABI names and `mint abi show ABI` for an ABI's scalar sizes, alignments and address units.

## Workflow

1. **Gather the target facts.** Infer what you can from the codebase (struct definitions, linker scripts, memory maps, existing hex generators, CRC routines) and confirm with the user before writing a layout:
   - the target compiler and ABI, matched to a profile from `mint abi list`;
   - each block's start address and allocated length;
   - the padding byte, usually the erased-flash value `0xFF`;
   - CRC parameters (polynomial, initial value, final XOR, input and output reflection) if blocks carry checksums;
   - the C struct for each block, including any packing attributes;
   - which values are fixed and which vary per build variant, the data source format and the variant names.
2. **Write the layout** from the struct definitions: one block per struct, fields in the struct's declaration order. Keep layout-only values as `value` or `const` and variant data as `name` lookups.
3. **Generate the header** with `mint header layout.toml -o layout.h` and compile it with the target compiler and flags. Its static assertions fail if the compiler lays the struct out differently. Prefer replacing the hand-written struct with the generated one so the layout stays the single source.
4. **Build** with `mint build layout.toml --xlsx data.xlsx --variants Default --stats`, adding `--strict` if lossy conversions should fail the build. Check the reported address ranges, space used and checksums against expectations.
5. **Wire it into the firmware build**: regenerate the header and output when the layout or data changes, and pass the block fingerprint into firmware if it should check stored data at runtime (`mint skill cli` has a CMake example).

## Decisions that go wrong

- **Packed structs are unsupported.** mint always applies the ABI's natural alignment and padding. If the firmware uses `__attribute__((packed))`, `#pragma pack` or similar, no layout can match it; raise this with the user before going further.
- **Declaration order is layout order.** Fields are placed in the order written, and a nested struct sits where its first field appears. Reordering fields changes offsets, checksums and the fingerprint.
- **Lenient conversion is the default.** Without `--strict`, out-of-range values saturate and floats truncate silently. `--strict` turns every value-changing conversion into an error, including decimals that `f32` cannot hold exactly, such as `3.3`; agree with the user which behaviour the build should have.
- **C28x counts addresses in 16-bit words.** `start_address` and literal ref addresses are word addresses while `length` is in octets, 8-bit types are rejected (strings use `u16`), and output files use octet addresses at twice the word address.
- **Strings are not NUL-terminated by mint.** Unused string and array elements take the block's padding byte, `0xFF` by default; size the field so the firmware's terminator or length convention holds.
- **Checksums cover only the bytes before them.** Put a checksum field after everything it should protect, normally last.
