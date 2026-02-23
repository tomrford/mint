use std::io::Write;

use mint_cli::layout::used_values::NoopValueSink;

#[path = "common/mod.rs"]
mod common;

fn build_block(
    block: &mint_cli::layout::block::Block,
    settings: &mint_cli::layout::settings::Settings,
    strict: bool,
    data_source: Option<&dyn mint_cli::data::DataSource>,
) -> Result<(Vec<u8>, u32), mint_cli::layout::error::LayoutError> {
    let mut noop = NoopValueSink;
    block.build_bytestream(data_source, settings, strict, &mut noop)
}

#[test]
fn lowercase_size_allows_padding() {
    common::ensure_out_dir();

    let layout_toml = r#"
[settings]
endianness = "little"
virtual_offset = 0

[settings.crc]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true
area = "data"

[block.header]
start_address = 0x80000
length = 0x100
crc_location = "end"
padding = 0xFF

[block.data]
short_array = { value = [1, 2, 3], type = "u16", size = 10 }
"#;

    let path = std::path::Path::new("out").join("test_lowercase_size.toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse layout");
    let block = cfg.blocks.get("block").expect("block present");

    let (bytes, _padding) = build_block(block, &cfg.settings, false, None)
        .expect("lowercase size should allow padding");

    assert!(bytes.len() >= 20);
}

#[test]
fn uppercase_size_rejects_underfilled_1d() {
    common::ensure_out_dir();

    let layout_toml = r#"
[settings]
endianness = "little"
virtual_offset = 0

[settings.crc]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true
area = "data"

[block.header]
start_address = 0x80000
length = 0x100
crc_location = "end"
padding = 0xFF

[block.data]
short_array = { value = [1, 2, 3], type = "u16", SIZE = 10 }
"#;

    let path = std::path::Path::new("out").join("test_uppercase_size_1d.toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse layout");
    let block = cfg.blocks.get("block").expect("block present");

    let res = build_block(block, &cfg.settings, false, None);
    assert!(res.is_err(), "SIZE should reject underfilled array");
    let err_msg = format!("{:?}", res.unwrap_err());
    assert!(err_msg.contains("smaller than defined size"));
}

#[test]
fn uppercase_size_rejects_underfilled_2d() {
    common::ensure_out_dir();

    let layout_toml = r#"
[settings]
endianness = "little"
virtual_offset = 0

[settings.crc]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true
area = "data"

[block.header]
start_address = 0x80000
length = 0x1000
crc_location = "end"
padding = 0xFF

[block.data]
matrix = { name = "CalibrationMatrix", type = "i16", SIZE = [5, 3] }
"#;

    let path = std::path::Path::new("out").join("test_uppercase_size_2d.toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse layout");
    let block = cfg.blocks.get("block").expect("block present");

    let ver_args = mint_cli::data::args::DataArgs {
        xlsx: Some("tests/data/data.xlsx".to_string()),
        versions: Some("Default".to_string()),
        ..Default::default()
    };
    let ds = mint_cli::data::create_data_source(&ver_args).expect("datasource loads");

    let res = build_block(block, &cfg.settings, false, ds.as_deref());
    assert!(res.is_err(), "SIZE should reject underfilled 2D array");
    let err_msg = format!("{:?}", res.unwrap_err());
    assert!(err_msg.contains("smaller than defined size"));
}

#[test]
fn both_size_and_uppercase_size_errors() {
    common::ensure_out_dir();

    let layout_toml = r#"
[settings]
endianness = "little"
virtual_offset = 0

[settings.crc]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true
area = "data"

[block.header]
start_address = 0x80000
length = 0x100
crc_location = "end"
padding = 0xFF

[block.data]
both = { value = [1, 2, 3], type = "u16", size = 5, SIZE = 10 }
"#;

    let path = std::path::Path::new("out").join("test_both_sizes.toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse layout");
    let block = cfg.blocks.get("block").expect("block present");

    let res = build_block(block, &cfg.settings, false, None);
    assert!(res.is_err(), "Using both size and SIZE should error");
    let err_msg = format!("{:?}", res.unwrap_err());
    assert!(err_msg.contains("Use either 'size' or 'SIZE', not both"));
}

#[test]
fn uppercase_size_accepts_exact_match() {
    common::ensure_out_dir();

    let layout_toml = r#"
[settings]
endianness = "little"
virtual_offset = 0

[settings.crc]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true
area = "data"

[block.header]
start_address = 0x80000
length = 0x100
crc_location = "end"
padding = 0xFF

[block.data]
exact_array = { value = [1, 2, 3, 4, 5], type = "u16", SIZE = 5 }
"#;

    let path = std::path::Path::new("out").join("test_uppercase_size_exact.toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse layout");
    let block = cfg.blocks.get("block").expect("block present");

    let (bytes, _padding) =
        build_block(block, &cfg.settings, false, None).expect("SIZE should accept exact match");

    assert!(bytes.len() >= 10);
}
