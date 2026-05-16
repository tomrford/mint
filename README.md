# mint

mint builds static binary flash blocks from TOML layout files and Excel or JSON data sources.

mint is available as:

- `mint-core` - Rust library crate for layout parsing, data sources, bytestream assembly, output rendering, and in-memory build APIs.
- `mint-cli` - Implements the `mint` command-line tool for reading layout/data files, writing outputs, and printing terminal summaries.
- `mint-python` - Python bindings for `mint-core`.

![img](doc/img.png)

### Install

```bash
cargo add mint-core
cargo install mint-cli
pip install mint-python
```

From a checkout, install the CLI with:

```bash
cargo install --path crates/mint-cli
```

### Workspace Commands

```bash
nix develop -c cargo build
nix develop -c cargo test
nix develop -c cargo clippy --workspace
nix develop -c cargo run -p mint-cli -- block.toml --xlsx data.xlsx -v Default
nix develop -c uv run --directory crates/mint-python --group dev maturin develop --manifest-path Cargo.toml
nix develop -c uv run --directory crates/mint-python --group dev pytest tests
```

### Documentation

- [CLI reference](doc/cli.md)
- [Python bindings](doc/python.md)
- [Layout files](doc/layout.md)
- [Data sources](doc/sources.md)
- [Example layouts & data](doc/examples/)

### Quick Start

```bash
# Excel data source
mint block.toml --xlsx data.xlsx -v Default --stats

# JSON data source
mint layout.toml -j data.json -v Debug/Default

# Multiple blocks with options
mint layout.toml#config layout.toml#data --xlsx data.xlsx -v Default --stats
```

### Layout Example

```toml
[mint]
endianness = "little"

[config.header]
start_address = 0x8000
length = 0x100

[config.data]
device.id = { value = 0x1234, type = "u32" }
device.name = { name = "DeviceName", type = "u8", size = 16 }
version = { name = "Version", type = "u16" }
gain = { value = 1.5, type = "uq8.8" }
coefficients = { name = "Coefficients", type = "f32", size = 4 }
matrix = { name = "Matrix", type = "i16", size = [2, 2] }

[data.header]
start_address = 0x8100
length = 0x100

[data.data]
counter = { name = "Counter", type = "u64" }
message = { value = "Hello", type = "u8", size = 16 }
```

See [`doc/examples/block.toml`](doc/examples/block.toml) for full examples.
