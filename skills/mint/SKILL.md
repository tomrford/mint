---
name: mint
description: "Guide for mint, which builds Intel HEX or Motorola S-Record flash blocks from TOML layouts and Excel or JSON data, and generates matching C headers and ABI fingerprints. Use it whenever a project uses mint or mint-cli, has TOML layouts describing flash or firmware memory blocks, needs configuration or calibration blocks built from structured data, or wants to replace a hand-written hex generator."
---

# mint

If `mint` is not present, install it via:

| Source | Install |
| --- | --- |
| GitHub releases | `mise use github:tomrford/mint` |
| crates.io | `cargo install mint-cli` |
| Nix flake | `nix profile install github:tomrford/mint` |

Prebuilt archives: https://github.com/tomrford/mint/releases

Then run `mint skill` for the guide that matches the installed version, and follow it before you write or change a layout.
