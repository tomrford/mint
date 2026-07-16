use std::io::Write;

use mint_core::data::{ExcelDataSource, ExcelDataSourceOptions};

#[path = "common/mod.rs"]
mod common;

fn default_excel_source() -> ExcelDataSource {
    ExcelDataSource::from_path(
        "tests/data/data.xlsx",
        ExcelDataSourceOptions::new(vec!["Default".to_owned()]),
    )
    .expect("datasource loads")
}

#[test]
fn lowercase_size_allows_padding() {
    common::ensure_out_dir();

    let layout_toml = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0xFF

[block.data]
short_array = { value = [1, 2, 3], type = "u16", size = 10 }
"#;

    let path = common::unique_out_path("test_lowercase_size", "toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let cfg = mint_core::layout::load_layout(path.to_str().unwrap()).expect("parse layout");
    let block = cfg.blocks.get("block").expect("block present");

    let (bytes, _padding) = common::build_block(block, &cfg.mint, false, None)
        .expect("lowercase size should allow padding");

    assert!(bytes.len() >= 20);
}

#[test]
fn uppercase_size_rejects_underfilled_1d() {
    common::ensure_out_dir();

    let layout_toml = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0xFF

[block.data]
short_array = { value = [1, 2, 3], type = "u16", SIZE = 10 }
"#;

    let path = common::unique_out_path("test_uppercase_size_1d", "toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let cfg = mint_core::layout::load_layout(path.to_str().unwrap()).expect("parse layout");
    let block = cfg.blocks.get("block").expect("block present");

    let res = common::build_block(block, &cfg.mint, false, None);
    assert!(res.is_err(), "SIZE should reject underfilled array");
    let err_msg = format!("{:?}", res.unwrap_err());
    assert!(err_msg.contains("smaller than defined size"));
}

#[test]
fn uppercase_size_rejects_underfilled_2d() {
    common::ensure_out_dir();

    let layout_toml = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x1000
padding = 0xFF

[block.data]
matrix = { name = "CalibrationMatrix", type = "i16", SIZE = [5, 3] }
"#;

    let path = common::unique_out_path("test_uppercase_size_2d", "toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let cfg = mint_core::layout::load_layout(path.to_str().unwrap()).expect("parse layout");
    let block = cfg.blocks.get("block").expect("block present");

    let ds = default_excel_source();

    let res = common::build_block(block, &cfg.mint, false, Some(&ds));
    assert!(res.is_err(), "SIZE should reject underfilled 2D array");
    let err_msg = format!("{:?}", res.unwrap_err());
    assert!(err_msg.contains("smaller than defined size"));
}

#[test]
fn both_size_and_uppercase_size_errors() {
    common::ensure_out_dir();

    let layout_toml = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0xFF

[block.data]
both = { value = [1, 2, 3], type = "u16", size = 5, SIZE = 10 }
"#;

    let path = common::unique_out_path("test_both_sizes", "toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let error = mint_core::layout::load_layout(path.to_str().unwrap())
        .expect_err("using both size and SIZE should fail parsing")
        .to_string();
    assert!(
        error.contains("only one size key") && error.contains("'size'") && error.contains("'SIZE'"),
        "unexpected error: {error}"
    );
}

#[test]
fn uppercase_size_accepts_exact_match() {
    common::ensure_out_dir();

    let layout_toml = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0xFF

[block.data]
exact_array = { value = [1, 2, 3, 4, 5], type = "u16", SIZE = 5 }
"#;

    let path = common::unique_out_path("test_uppercase_size_exact", "toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let cfg = mint_core::layout::load_layout(path.to_str().unwrap()).expect("parse layout");
    let block = cfg.blocks.get("block").expect("block present");

    let (bytes, _padding) =
        common::build_block(block, &cfg.mint, false, None).expect("SIZE should accept exact match");

    assert!(bytes.len() >= 10);
}
