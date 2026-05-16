# mint

Python bindings for `mint-core`.

```bash
pip install mint-python
```

For local development:

```bash
uv run --directory crates/mint-python --group dev maturin develop --manifest-path Cargo.toml
uv run --directory crates/mint-python --group dev pytest tests
```
