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

    let err = mint_core::layout::load_layout(&path).expect_err("layout should be rejected");
    let message = err.to_string();
    assert!(
        message.contains("unknown field") && message.contains("unknown"),
        "expected unknown-field error, got: {}",
        message
    );
}

#[test]
fn toml_rejects_virtual_offset() {
    let path = write_layout(
        "virtual-offset",
        "toml",
        r#"
[mint]
endianness = "little"
virtual_offset = 0

[block.header]
start_address = 0x1000
length = 0x20

[block.data]
value = { value = 1, type = "u16" }
"#,
    );

    let err = mint_core::layout::load_layout(&path).expect_err("layout should be rejected");
    let message = err.to_string();
    assert!(
        message.contains("unknown field") && message.contains("virtual_offset"),
        "expected virtual_offset unknown-field error, got: {}",
        message
    );
}
