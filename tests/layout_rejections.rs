#[path = "common/mod.rs"]
mod common;

use std::fs;

fn write_layout(file_stem: &str, ext: &str, contents: &str) -> String {
    common::ensure_out_dir();
    let path = format!("out/{}.{}", file_stem, ext);
    fs::write(&path, contents).expect("write layout file");
    path
}

#[test]
fn toml_rejects_unknown_mint_key() {
    let path = write_layout(
        "unknown-mint-key",
        "toml",
        r#"
[mint]
endianness = "little"
unknown = true

[block.header]
start_address = 0x1000
length = 0x20

[block.data]
value = { value = 1, type = "u16" }
"#,
    );

    let err = mint_cli::layout::load_layout(&path).expect_err("layout should be rejected");
    let message = err.to_string();
    assert!(
        message.contains("unknown field") && message.contains("unknown"),
        "expected unknown-field error, got: {}",
        message
    );
}

#[test]
fn toml_rejects_malformed_fixed_point_type() {
    let path = write_layout(
        "malformed-fixed-point",
        "toml",
        r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x1000
length = 0x20

[block.data]
value = { value = 1, type = "q8.8.8" }
"#,
    );

    let err = mint_cli::layout::load_layout(&path).expect_err("layout should be rejected");
    let message = err.to_string();
    assert!(
        message.contains("invalid fixed-point type 'q8.8.8'"),
        "expected fixed-point parse hint, got: {}",
        message
    );
}

#[test]
fn yaml_rejects_unsupported_fixed_point_width() {
    let path = write_layout(
        "unsupported-fixed-point-width",
        "yaml",
        r#"
mint:
  endianness: little

block:
  header:
    start_address: 0x1000
    length: 0x20
  data:
    value:
      value: 1
      type: q3.10
"#,
    );

    let err = mint_cli::layout::load_layout(&path).expect_err("layout should be rejected");
    let message = err.to_string();
    assert!(
        message.contains("unsupported fixed-point width in type 'q3.10'"),
        "expected fixed-point width hint, got: {}",
        message
    );
}
