# ABI part 2 — open issues

Temporary tracker for the follow-up to the ABI profile foundation. Delete once part 2 lands.

## Address-unit semantics at the output boundary

The octet↔address-unit conversion exists only at the ref/fingerprint boundary
(`Abi::offset_to_address_units`). Word-addressed profiles (C28x) need decisions and hooks
everywhere else addresses and sizes meet:

- `header.start_address` is in target address units. `header.length`, resolved offsets, storage
  sizes and buffers are octets. Ref emission therefore remains
  `start_address + offset_octets / unit_octets`.
- For the first C28x implementation, Intel HEX and Motorola S-record output intentionally remain
  standard octet-addressed formats. Convert the target start address at the output boundary with
  `output_start_octets = start_address * unit_octets`; range ends and overlap checks then remain
  octet-based. This matches the existing BRUSA bootloader flow, which halves record addresses.
- A native TI word-addressed HEX dialect, including word swapping, remains a possible later output
  backend rather than an ABI rule.

## Storage widening seam in conversions

The initial C28x profile will reject `u8`, `i8`, 8-bit fixed-point and strings, so its supported
scalars retain equal logical and storage widths. If a later explicitly named widened-byte type is
needed, plumb `ScalarAbi` into `conversions.rs`: `DataValue::to_bytes` currently emits logical
width, and strings need an explicit one-character-per-word or packed encoding contract.

## Header generation for 16-bit char targets

Generated assertions now use `sizeof/offsetof * CHAR_BIT == octets * 8u`. Verify them with the TI
compiler when C28x lands, including nested fields and final aggregate size.

## New profiles

- `ti-c28x-eabi`, plus whichever embedded ABIs part 2 scopes in.
- Per-profile scalar support: `Abi::scalar` returns `Result` so profiles can reject types
  (e.g. no `f64`, or `f64` spelled `long double`).
- `mint abi show` now prints its scalar size/alignment/stride/C-type table from `Abi::scalar`.

## Tooling

- Layout-inspection command: per-field offset, storage width, alignment, aggregate size.
- Compile probes: golden layout/header tests against TI C2000 and `arm-none-eabi-gcc` where
  available on the build host.
- `Abi` parses via `FromStr` in clap; switch to `ValueEnum` when the profile list grows so
  `--help` and shell completion enumerate names.

## Release note

The fingerprint hash context is `v2`; all v1 fingerprints (flashed binaries, generated headers)
are incompatible. Call this out in the release notes of the release carrying the ABI work.
