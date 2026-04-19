use std::io::Write;

#[path = "common/mod.rs"]
mod common;

#[test]
fn strict_conversions_success() {
    common::ensure_out_dir();

    let layout_toml = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
ok.float_exact_to_i16 = { value = 42.0, type = "i16" }
ok.int_exact_to_f32   = { value = 16777216, type = "f32" }
"#;

    let path = std::path::Path::new("out").join("test_strict_ok.toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse ok layout");
    let block = cfg.blocks.get("block").expect("block present");

    let ver_args = mint_cli::data::args::DataArgs {
        xlsx: Some("tests/data/data.xlsx".to_owned()),
        versions: Some("Default".to_owned()),
        ..Default::default()
    };
    let ds = mint_cli::data::create_data_source(&ver_args).expect("datasource loads");

    let (bytes, _) = common::build_block(block, &cfg.mint, true, ds.as_deref())
        .expect("strict conversions should succeed");
    assert!(!bytes.is_empty());
}

#[test]
fn strict_conversions_fail_fractional_float_to_int() {
    common::ensure_out_dir();

    let layout_toml = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
bad.frac_to_u8 = { value = 1.5, type = "u8" }
"#;

    let path = std::path::Path::new("out").join("test_strict_bad_frac.toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse bad layout");
    let block = cfg.blocks.get("block").expect("block present");

    let ver_args = mint_cli::data::args::DataArgs {
        xlsx: Some("tests/data/data.xlsx".to_owned()),
        versions: Some("Default".to_owned()),
        ..Default::default()
    };
    let ds = mint_cli::data::create_data_source(&ver_args).expect("datasource loads");

    let res = common::build_block(block, &cfg.mint, true, ds.as_deref());
    assert!(
        res.is_err(),
        "strict mode should reject fractional float to int"
    );
}

#[test]
fn strict_conversions_fail_large_int_to_f64_lossy() {
    common::ensure_out_dir();

    let layout_toml = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
bad.large_int_to_f64 = { value = 9007199254740993, type = "f64" }
"#;

    let path = std::path::Path::new("out").join("test_strict_bad_large.toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse bad layout");
    let block = cfg.blocks.get("block").expect("block present");

    let ver_args = mint_cli::data::args::DataArgs {
        xlsx: Some("tests/data/data.xlsx".to_owned()),
        versions: Some("Default".to_owned()),
        ..Default::default()
    };
    let ds = mint_cli::data::create_data_source(&ver_args).expect("datasource loads");

    let res = common::build_block(block, &cfg.mint, true, ds.as_deref());
    assert!(
        res.is_err(),
        "strict mode should reject lossy int to f64 conversion"
    );
}

#[test]
fn strict_conversions_accept_bool_literals() {
    common::ensure_out_dir();

    let layout_toml = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
bools.true_flag = { value = true, type = "u8" }
bools.false_flag = { value = false, type = "u8" }
bools.array_flags = { value = [true, false, true], type = "u8", size = 3 }
"#;

    let path = std::path::Path::new("out").join("test_bool_literals.toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse bool layout");
    let block = cfg.blocks.get("block").expect("block present");

    let (bytes, _) =
        common::build_block(block, &cfg.mint, true, None).expect("bool literals convert");
    assert!(
        bytes.starts_with(&[1, 0, 1, 0, 1]),
        "bool values should map to 0/1, got {:?}",
        &bytes[..5.min(bytes.len())]
    );
}
