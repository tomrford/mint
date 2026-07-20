# ABI part 2 — open issues

Temporary tracker for the follow-up to the ABI profile foundation. Delete once part 2 lands.

## Address-unit semantics at the output boundary

The octet↔address-unit conversion exists only at the ref/fingerprint boundary
(`Abi::offset_to_address_units`). Word-addressed profiles (C28x) need decisions and hooks
everywhere else addresses and sizes meet:

- `header.start_address` and `header.length` units are undefined for word-addressed profiles.
  Ref emission computes `start_address + offset_in_address_units`, implying start_address is in
  address units, but `resolved.rs` compares octet `total_size` against `length`, and
  `output/mod.rs` computes range ends as `start_address + bytestream.len()` (octets).
- Proposed rule: user-visible addresses (headers, refs, output records) are always in target
  address units; buffers and sizes are always octets; convert exactly once at the range boundary
  (`end = start + len_octets / unit_octets`). Overlap checks must use one unit consistently.
- TI HEX word addressing and word swapping belong in an output backend, not in the ABI rules.

## Storage widening seam in conversions

`DataValue::to_bytes(scalar_type, endianness)` emits logical width. A profile where
`storage_size != logical width` (C28x 16-bit char: `u8` occupies one 16-bit word) trips the
`debug_assert` in `append_array_element`. Plumb `ScalarAbi` into `conversions.rs` so encoding
produces storage-width bytes. Strings feed byte-by-byte through the same path and need a char
encoding decision (one char per word vs packed).

## Header generation for 16-bit char targets

Generated `_Static_assert(sizeof/offsetof == Nu)` assertions assume 8-bit char. Word-addressed
profiles need the `* CHAR_BIT` formulation, since `sizeof`/`offsetof` count words on C28x.

## New profiles

- `arm-aapcs32-le`, `ti-c28x-eabi`, plus whichever embedded ABIs part 2 scopes in.
- Per-profile scalar support: `Abi::scalar` returns `Result` so profiles can reject types
  (e.g. no `f64`, or `f64` spelled `long double`).
- `mint abi show` derives its prose from `AbiFamily`; once families diverge, print the
  per-scalar size/alignment/stride table instead of prose.

## Tooling

- Layout-inspection command: per-field offset, storage width, alignment, aggregate size.
- Compile probes: golden layout/header tests against TI C2000 and `arm-none-eabi-gcc` where
  available on the build host.
- `Abi` parses via `FromStr` in clap; switch to `ValueEnum` when the profile list grows so
  `--help` and shell completion enumerate names.

## Release note

The fingerprint hash context is `v2`; all v1 fingerprints (flashed binaries, generated headers)
are incompatible. Call this out in the release notes of the release carrying the ABI work.
