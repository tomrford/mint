use std::io::Write;

use mint_core::data::{ExcelDataSource, ExcelDataSourceOptions, JsonDataSource};

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
fn non_strict_integer_conversions_saturate() {
    common::ensure_out_dir();

    let layout_toml = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
overflow.u8_high = { value = 256, type = "u8" }
overflow.u8_low = { value = -1, type = "u8" }
overflow.i8_high = { value = 128, type = "i8" }
overflow.i8_low = { value = -129, type = "i8" }
overflow.u8_float_trunc = { value = 1.5, type = "u8" }
overflow.u8_float_high = { value = 300.0, type = "u8" }
"#;

    let path = common::unique_out_path("test_non_strict_saturation", "toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let (bytes, _) = common::build_block(&path, "block", false, None).expect("non-strict converts");
    assert_eq!(
        &bytes[..6],
        &[0xff, 0x00, 0x7f, 0x80, 0x01, 0xff],
        "non-strict integer conversions should saturate, while floats still truncate"
    );
}

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

    let path = common::unique_out_path("test_strict_ok", "toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let ds = default_excel_source();

    let (bytes, _) = common::build_block(&path, "block", true, Some(&ds))
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

    let path = common::unique_out_path("test_strict_bad_frac", "toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let ds = default_excel_source();

    let res = common::build_block(&path, "block", true, Some(&ds));
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

    let path = common::unique_out_path("test_strict_bad_large", "toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let ds = default_excel_source();

    let res = common::build_block(&path, "block", true, Some(&ds));
    assert!(
        res.is_err(),
        "strict mode should reject lossy int to f64 conversion"
    );
}

#[test]
fn strict_conversions_reject_float_integer_boundaries() {
    common::ensure_out_dir();

    for (field, scalar_type, value) in [
        ("bad.u64_boundary", "u64", "18446744073709551616.0"),
        ("bad.i64_boundary", "i64", "9223372036854775808.0"),
    ] {
        let layout_toml = format!(
            r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
{field} = {{ value = {value}, type = "{scalar_type}" }}
"#
        );

        let path = common::unique_out_path("test_strict_float_integer_boundary", "toml");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(layout_toml.as_bytes()).unwrap();

        let res = common::build_block(&path, "block", true, None);
        assert!(
            res.is_err(),
            "strict mode should reject {value} to {scalar_type}"
        );
    }
}

#[test]
fn strict_conversions_reject_lossy_u64_to_float() {
    common::ensure_out_dir();

    let layout_toml = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
bad.large_u64_to_f64 = { name = "Value", type = "f64" }
"#;

    let path = common::unique_out_path("test_strict_bad_large_u64_to_f64", "toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let variants = vec!["Default".to_owned()];
    let ds = JsonDataSource::from_str(r#"{"Default":{"Value":18446744073709551615}}"#, &variants)
        .expect("datasource load");

    let res = common::build_block(&path, "block", true, Some(&ds));
    assert!(
        res.is_err(),
        "strict mode should reject lossy u64 to f64 conversion"
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

    let path = common::unique_out_path("test_bool_literals", "toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(layout_toml.as_bytes()).unwrap();

    let (bytes, _) =
        common::build_block(&path, "block", true, None).expect("bool literals convert");
    assert!(
        bytes.starts_with(&[1, 0, 1, 0, 1]),
        "bool values should map to 0/1, got {:?}",
        &bytes[..5.min(bytes.len())]
    );
}
