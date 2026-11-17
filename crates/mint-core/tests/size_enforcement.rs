use mint_core::data::{ExcelDataSource, ExcelDataSourceOptions};
use mint_core::error::MintError;
use mint_core::layout::error::LayoutError;

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
fn oversized_layout_fails_during_block_build() {
    let layout = common::write_layout_file(
        "oversized_layout",
        r#"
[mint]
abi = "generic-le"

[oversized.header]
start_address = 0x80000
length = 4

[oversized.data]
value = { value = 1, type = "u64" }
"#,
    );

    let error = common::build_block(&layout, "oversized", false, None)
        .expect_err("resolved layout should not exceed the block length");
    let chain = common::error_chain(&error);
    assert!(matches!(
        &error,
        MintError::InBlock { source, .. }
            if matches!(source.as_ref(), MintError::Layout(LayoutError::InvalidLayout(_)))
    ));
    assert!(
        chain
            .contains("resolved layout size (8 octets) exceeds configured block length (4 octets)"),
        "{chain}"
    );
}

#[test]
fn materialized_block_limit_fails_before_allocation() {
    let size = mint_core::layout::MAX_RESOLVED_BLOCK_SIZE + 1;
    let layout = common::write_layout_file(
        "materialized_block_limit",
        &format!(
            r#"
[mint]
abi = "generic-le"

[block.header]
start_address = 0
length = {size}

[block.data]
value = {{ value = "", type = "u8", size = {size} }}
"#
        ),
    );

    let error = common::build_block(&layout, "block", false, None)
        .expect_err("oversized materialized block should be rejected before allocation");
    let chain = common::error_chain(&error);
    assert!(
        chain.contains("exceeds Mint's materialized block limit"),
        "{chain}"
    );
}

#[test]
fn zero_extent_arrays_fail_during_block_build() {
    let layout = common::write_layout_file(
        "zero_extent_layout",
        r#"
[mint]
abi = "generic-le"

[block.header]
start_address = 0x1000
length = 0x20

[block.data]
values = { value = [], type = "u8", size = 0 }
"#,
    );

    let error = common::build_block(&layout, "block", false, None)
        .expect_err("zero-extent arrays are invalid layouts");
    let chain = common::error_chain(&error);
    assert!(
        chain.contains("array 'values' has a zero extent"),
        "{chain}"
    );
}

#[test]
fn two_dimensional_literals_fail_during_block_build() {
    let layout = common::write_layout_file(
        "two_dimensional_literal",
        r#"
[mint]
abi = "generic-le"

[block.header]
start_address = 0x1000
length = 0x20

[block.data]
matrix = { value = [1, 2, 3, 4], type = "u8", size = [2, 2] }
"#,
    );

    let error = common::build_block(&layout, "block", false, None)
        .expect_err("two-dimensional literals are invalid layouts");
    let chain = common::error_chain(&error);
    assert!(
        chain.contains("2D arrays within the layout file are not supported"),
        "{chain}"
    );
}

#[test]
fn uppercase_size_accepts_exact_and_rejects_underfilled_1d() {
    let write_layout = |name: &str, values: &str| {
        common::write_layout_file(
            name,
            &format!(
                r#"
[mint]
abi = "generic-le"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0xFF

[block.data]
values = {{ value = [{values}], type = "u16", SIZE = 5 }}
"#
            ),
        )
    };

    let exact = write_layout("uppercase_size_exact", "1, 2, 3, 4, 5");
    let bytes = common::build_block(&exact, "block", false, None)
        .expect("SIZE should accept an exact array length");
    assert_eq!(&bytes[..10], &[1, 0, 2, 0, 3, 0, 4, 0, 5, 0]);

    let underfilled = write_layout("uppercase_size_underfilled", "1, 2, 3");

    let error = common::build_block(&underfilled, "block", false, None)
        .expect_err("SIZE should reject underfilled array");
    assert!(matches!(
        &error,
        MintError::InBlock { source, .. }
            if matches!(source.as_ref(), MintError::Layout(LayoutError::InField { source, .. })
                if matches!(source.as_ref(), LayoutError::DataValueExportFailed(_)))
    ));
    let err_msg = format!("{error:?}");
    assert!(err_msg.contains("smaller than defined size"));
}

#[test]
fn uppercase_size_rejects_underfilled_2d() {
    let layout_toml = r#"
[mint]
abi = "generic-le"

[block.header]
start_address = 0x80000
length = 0x1000
padding = 0xFF

[block.data]
matrix = { name = "CalibrationMatrix", type = "i16", SIZE = [5, 3] }
"#;

    let path = common::write_layout_file("test_uppercase_size_2d", layout_toml);

    let ds = default_excel_source();

    let error = common::build_block(&path, "block", false, Some(&ds))
        .expect_err("SIZE should reject underfilled 2D array");
    let chain = common::error_chain(&error);
    assert!(
        chain.contains(
            "in field 'matrix': data value export failed: 2D array row count smaller than defined size (strict SIZE)"
        ),
        "{chain}"
    );
}

#[test]
fn both_size_and_uppercase_size_errors() {
    let layout_toml = r#"
[mint]
abi = "generic-le"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0xFF

[block.data]
both = { value = [1, 2, 3], type = "u16", size = 5, SIZE = 10 }
"#;

    let path = common::write_layout_file("test_both_sizes", layout_toml);

    let error = mint_core::layout::load_layout(&path)
        .expect_err("using both size and SIZE should fail parsing")
        .to_string();
    assert!(
        error.contains("only one size key") && error.contains("'size'") && error.contains("'SIZE'"),
        "unexpected error: {error}"
    );
}
