# Python Bindings

`mint-python` exposes the `mint-core` build engine to Python. It is for scripts, notebooks, tests, and other programs that need mint output in memory. Use the CLI when the job is file-in/file-out and terminal output is the desired interface.

The bindings are not published to PyPI; releases cover the Rust crates and
the `mint` binary only. From a checkout, build the extension and run the
binding tests with:

```bash
nix develop -c uv run --directory crates/mint-python --group dev maturin develop --manifest-path Cargo.toml
nix develop -c uv run --directory crates/mint-python --group dev pytest tests
```

## Shape

The Python API follows the same build model as the CLI:

1. Load or create one or more layouts with `mint.Layout`.
2. Select blocks with `layout.blocks(...)`.
3. Call `mint.build(...)` with an optional data source and variant stack.
4. Read ranges, stats, used values, or rendered HEX/S-Record text from the result.

The difference is where side effects happen. The CLI reads layout/data paths, writes the output file, optionally writes the used-values JSON report, and prints terminal statistics. The Python bindings return a `BuildResult`; callers decide whether to keep bytes in memory, render text, write files, inspect stats, or pass results to another library.

## Layouts and Blocks

Use `Layout.from_file(...)` for normal TOML layout files:

```python
import mint

layout = mint.Layout.from_file("layout.toml")
result = mint.build(layout.blocks("config", "calibration"))
```

Use `Layout.from_string(...)` when a program generates or stores the layout text itself:

```python
layout = mint.Layout.from_string(
    "generated.toml",
    """
    [mint]
    endianness = "little"

    [config.header]
    start_address = 0x8000
    length = 0x10

    [config.data]
    value = { value = 1, type = "u8" }
    """,
)

result = mint.build(layout.blocks())
```

Calling `layout.blocks()` with no names builds every block in that layout. Passing names builds those blocks in the requested order.

A named `BuildBlock` computes the same selector-scoped lowercase hexadecimal ABI fingerprint as `mint fingerprint FILE#BLOCK`. File-backed selectors read the current layout each time, so the property stays consistent with later builds. Unrelated sibling blocks are not resolved:

```python
config = layout.blocks("config")[0]
print(config.fingerprint)
result = mint.build([config])
```

The singular property requires a named selector. Accessing `fingerprint` on a selector returned by `layout.blocks()` raises `ValueError` because it represents every block and has no single fingerprint value.

## Data Sources

`mint.build(...)` accepts at most one data source:

- `data={...}` for an in-memory JSON-style `dict`.
- `json_path="data.json"` for a JSON file.
- `xlsx_path="data.xlsx"` for an Excel workbook.

When a data source is provided, `variants=[...]` is required. The order matches the CLI variant priority: earlier names win, later names are fallbacks.

```python
layout = mint.Layout.from_file("layout.toml")

result = mint.build(
    layout.blocks("config"),
    data={"Debug": {"DeviceName": "dev", "Version": 2}},
    variants=["Debug"],
)
```

For Excel input, the default main sheet is `Main` and can be overridden:

```python
result = mint.build(
    layout.blocks("config"),
    xlsx_path="data.xlsx",
    variants=["Debug", "Default"],
    main_sheet="Config",
)
```

Use `strict=True` to match the CLI `--strict` conversion behavior.

## Results

`BuildResult.ranges` contains the generated address ranges as bytes:

```python
for data_range in result.ranges:
    print(hex(data_range.start_address), data_range.data)
```

`BuildResult.stats` exposes the same build summary data the CLI prints: `blocks_processed`, `total_allocated`, `total_reserved`, `total_duration_ms`, per-block `block_stats`, and `space_reserved_pct`. Each `DataRange` also exposes `reserved_size` and `allocated_size`.

`BuildResult.used_values` is the same used-values report shape that the CLI writes with `--export-json`: layout name, then block name, then the values used for that block.

Render output text with:

```python
hex_text = result.to_intel_hex(record_width=32)
srec_text = result.to_srec(record_width=32)
```

The Python bindings do not write output files directly. Write `hex_text`, `srec_text`, or `data_range.data` from Python when a file is needed.
