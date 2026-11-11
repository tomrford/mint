# Command line

`mint --help` and `mint <command> --help` list every option. This page covers what the help text cannot show.

## Selecting blocks

`build`, `header` and `fingerprint` take layout selectors. `layout.toml` selects every block in declaration order and `layout.toml#config` selects one. `build` and `header` accept several selectors, which may mix both forms and several files; a block selected twice is processed once, and paths that name the same file count as one layout.

```bash
mint build layout.toml#config layout.toml#data --xlsx data.xlsx --variants Default -o firmware.hex
mint header layout.toml -o layout.h
```

## Build output

`mint build` writes every selected block into one file and rejects blocks whose address ranges overlap. It refuses to write the output over an input file, and warns when the output extension suggests the other format. Data-source options are described in [sources.md](sources.md) (`mint skill sources`).

`--export-json` writes the values each block used, nested by layout file, block and field path. Bitmaps appear as objects of their sub-fields, with literal sub-fields named `reserved_<bit offset>_<bits>`.

`--stats` prints a summary and one row per block; for `doc/examples/block.toml` with `doc/examples/data.xlsx`:

```text
+------------------+-----------+
| Build Summary    |           |
+==============================+
| Build Time       | 852us     |
|------------------+-----------|
| Blocks Processed | 2         |
|------------------+-----------|
| Total Allocated  | 512 bytes |
|------------------+-----------|
| Total Reserved   | 112 bytes |
|------------------+-----------|
| Space Reserved   | 21.9%     |
+------------------+-----------+

+-------------------+------------------------------+----------------------------+----------------+----------------+
| Block             | Address Range (target units) | Reserved/Allocated (bytes) | Space Reserved | Checksum Value |
+=================================================================================================================+
| block.toml#config | 0x8000-0x80FF                | 64 bytes/256 bytes         | 25.0%          | 0x20E90F51     |
|-------------------+------------------------------+----------------------------+----------------+----------------|
| block.toml#data   | 0x8100-0x81FF                | 48 bytes/256 bytes         | 18.8%          | 0xC51E4C95     |
+-------------------+------------------------------+----------------------------+----------------+----------------+
```

## Fingerprints in a build system

`mint fingerprint layout.toml#config` prints one 16-character lowercase hexadecimal value; `mint fingerprint layout.toml` prints `<block> <fingerprint>` lines. Stdout holds only those values, diagnostics go to stderr and failures exit non-zero, so the output can be captured directly. In CMake, re-run configuration when the layout changes and pass the value to the firmware:

```cmake
set(LAYOUT "${CMAKE_CURRENT_SOURCE_DIR}/layout.toml")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${LAYOUT}")

execute_process(
  COMMAND mint fingerprint "${LAYOUT}#config"
  OUTPUT_VARIABLE CONFIG_FINGERPRINT
  OUTPUT_STRIP_TRAILING_WHITESPACE
  COMMAND_ERROR_IS_FATAL ANY
)

target_compile_definitions(
  firmware PRIVATE "CONFIG_SCHEMA_FINGERPRINT=0x${CONFIG_FINGERPRINT}ULL"
)
```

A layout fingerprint field also appears in the generated header as `<BLOCK>_<FIELD>_FINGERPRINT`, which is often simpler when the firmware already includes that header.
