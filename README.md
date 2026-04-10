## mint

Build flash blocks from a layout file (TOML) and a data source (Excel or JSON), then emit hex files.

![img](doc/img.png)

Install with `cargo install mint-cli` or via nix flakes.

### Documentation

- [CLI reference](doc/cli.md)
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

[config.data]
device.id = { value = 0x1234, type = "u32" }
device.name = { name = "DeviceName", type = "u8", size = 16 }
version = { name = "Version", type = "u16" }
coefficients = { name = "Coefficients", type = "f32", size = 4 }
matrix = { name = "Matrix", type = "i16", size = [2, 2] }

[data.data]
counter = { name = "Counter", type = "u64" }
message = { value = "Hello", type = "u8", size = 16 }
```

See [`doc/examples/block.toml`](doc/examples/block.toml) for full examples.
