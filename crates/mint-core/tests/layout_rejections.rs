#[path = "common/mod.rs"]
mod common;

use std::fs;

fn write_layout(file_stem: &str, ext: &str, contents: &str) -> String {
    let path = common::unique_out_path(file_stem, ext);
    fs::write(&path, contents).expect("write layout file");
    path.to_string_lossy().into_owned()
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

#[test]
fn build_rejects_aliased_field_paths() {
    // A quoted dotted key and a nested table are distinct TOML keys but join
    // to the same field path; refs against that path would be ambiguous.
    let path = write_layout(
        "aliased-paths",
        "toml",
        r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x1000
length = 0x20

[block.data]
"a.b" = { value = 0x11, type = "u32" }
ptr = { ref = "a.b", type = "u32" }

[block.data.a]
b = { value = 0x22, type = "u32" }
"#,
    );

    let cfg = mint_core::layout::load_layout(&path).expect("layout parses");
    let block = cfg.blocks.get("block").expect("block present");
    let err = common::build_block(block, &cfg.mint, false, None)
        .expect_err("aliased field paths should be rejected");
    assert!(
        err.to_string().contains("Duplicate field path 'a.b'"),
        "unexpected error: {err}"
    );
}
