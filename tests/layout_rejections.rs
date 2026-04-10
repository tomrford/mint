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
fn toml_rejects_removed_word_addressing_key() {
    let path = write_layout(
        "removed-word-addressing",
        "toml",
        r#"
[mint]
endianness = "little"
word_addressing = true

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
        message.contains("[mint].word_addressing") && message.contains("removed"),
        "expected removal hint, got: {}",
        message
    );
}

#[test]
fn yaml_rejects_removed_word_addressing_key() {
    let path = write_layout(
        "removed-word-addressing",
        "yaml",
        r#"
mint:
  endianness: little
  word_addressing: true

block:
  header:
    start_address: 0x1000
    length: 0x20
  data:
    value:
      value: 1
      type: u16
"#,
    );

    let err = mint_cli::layout::load_layout(&path).expect_err("layout should be rejected");
    let message = err.to_string();
    assert!(
        message.contains("[mint].word_addressing") && message.contains("removed"),
        "expected removal hint, got: {}",
        message
    );
}
