# mint

Python bindings for `mint-core`.

This package is not published to PyPI and is not part of the release
pipeline; build it from a checkout with maturin.

For local development:

```bash
uv run --directory crates/mint-python --group dev maturin develop --manifest-path Cargo.toml
uv run --directory crates/mint-python --group dev pytest tests
```
