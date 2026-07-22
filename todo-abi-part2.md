# ABI part 2 — open issues

Temporary tracker for the follow-up to the ABI profile foundation. Delete once part 2 lands.

## Native TI output dialect

C28x now uses target word addresses for `header.start_address` and refs, while lengths, resolved
offsets, buffers, overlap checks and standard output addresses remain octet-based. Intel HEX and
Motorola S-record starts are emitted as `target_start_address * unit_octets`, matching the existing
BRUSA bootloader flow.

A native TI word-addressed HEX dialect, including word swapping, remains a possible later output
backend rather than an ABI rule. A future output-engine change, potentially using hexy, should own
that dialect and any output-level checksum transforms.

## Storage widening seam in conversions

The C28x profile rejects `u8`, `i8`, 8-bit fixed-point and strings, so its supported
scalars retain equal logical and storage widths. If a later explicitly named widened-byte type is
needed, plumb `ScalarAbi` into `conversions.rs`: `DataValue::to_bytes` currently emits logical
width, and strings need an explicit one-character-per-word or packed encoding contract.

## Header generation for 16-bit char targets

Generated assertions now use `sizeof/offsetof * CHAR_BIT == octets * 8u`. Verify them with the TI
compiler, including nested fields and final aggregate size.

## Future profiles

Per-profile scalar support is available through `Abi::scalar`, including rejection and C spelling.
Qualify compiler-dependent profiles carefully—for example, AVR profiles must identify their
configured `double` width rather than presenting one ambiguous ABI name.

## Tooling

- Layout-inspection command: per-field offset, storage width, alignment, aggregate size.
- Compile probes: golden layout/header tests against TI C2000 and `arm-none-eabi-gcc` where
  available on the build host.
- `Abi` parses via `FromStr` in clap; switch to `ValueEnum` when the profile list grows so
  `--help` and shell completion enumerate names.

## Release note

The fingerprint hash context is `v2`; all v1 fingerprints (flashed binaries, generated headers)
are incompatible. Call this out in the release notes of the release carrying the ABI work.
