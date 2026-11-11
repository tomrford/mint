use mint_core::build::BlockSelector;
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
fn c28x_rejects_odd_octet_lengths() {
    let layout = common::write_layout_file(
        "c28x_odd_length",
        r#"
[mint]
abi = "ti-c28x-eabi"

[block.header]
start_address = 0
length = 3

[block.data]
value = { value = 0, type = "u16" }
"#,
    );

    let error = common::build_block(&layout, "block", false, None)
        .expect_err("odd C28x block length should fail");
    assert!(common::error_chain(&error).contains("not divisible"));
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

fn literal_layout(data: &str) -> String {
    format!(
        r#"
[mint]
abi = "generic-le"

[mint.const]
magic = 0xDEADBEEF
label = "AB"
octets = [192, 168, 1, 10]

[block.header]
start_address = 0x1000
length = 0x40

[block.data]
{data}
"#
    )
}

/// Literal (`value`) and `const` shapes are knowable from the layout alone, so
/// `build`, `header` and `fingerprint` must accept and reject them alike.
#[test]
fn literal_shapes_are_validated_alike_by_build_header_and_fingerprint() {
    let outcomes = |name: &str, layout: &str| {
        let path = common::write_layout_file(name, layout);
        [
            common::build_block(&path, "block", false, None)
                .map(drop)
                .map_err(|error| common::error_chain(&error)),
            mint_core::header::generate(&[BlockSelector::all(&path)])
                .map(drop)
                .map_err(|error| common::error_chain(&error)),
            mint_core::fingerprint::load(&BlockSelector::all(&path))
                .map(drop)
                .map_err(|error| common::error_chain(&error)),
        ]
    };

    let valid = literal_layout(
        r#"
scalar = { const = "magic", type = "u32" }
text = { value = "AB", type = "u16", SIZE = 2 }
named = { const = "label", type = "u8", size = 4 }
octets = { const = "octets", type = "u8", SIZE = 4 }
padded = { value = [1, 2], type = "u8", size = 4 }
"#,
    );
    for outcome in outcomes("literal_shape_valid", &valid) {
        outcome.expect("valid literal shapes pass every command");
    }

    let root_leaf = r#"
[mint]
abi = "generic-le"

[block.header]
start_address = 0x1000
length = 0x40

[block.data]
type = "u8"
value = 7
"#;
    let mut cases = vec![(root_leaf.to_owned(), "block data must be a table of fields")];
    for (data, expected) in [
        (
            r#"x = { value = [1, 2, 3, 4], type = "u8", size = [2, 2] }"#,
            "2D arrays within the layout file are not supported",
        ),
        (
            r#"x = { value = 5, type = "u8", size = 4 }"#,
            "size/SIZE keys are forbidden with scalar value",
        ),
        (
            r#"x = { value = [1, 2], type = "u8" }"#,
            "Array value requires size or SIZE",
        ),
        (
            r#"x = { value = "AB", type = "u8" }"#,
            "String value requires size or SIZE",
        ),
        (
            r#"x = { const = "octets", type = "u8" }"#,
            "Array const requires size or SIZE",
        ),
        (
            r#"x = { const = "label", type = "u8" }"#,
            "String const requires size or SIZE",
        ),
        (
            r#"x = { value = "AB", type = "u32", size = 4 }"#,
            "Strings should have type u8 or u16",
        ),
        (
            r#"x = { const = "label", type = "i16", size = 4 }"#,
            "Strings should have type u8 or u16",
        ),
        (
            r#"x = { value = [1, 2, 3], type = "u8", size = 2 }"#,
            "Array/string is larger than defined size",
        ),
        (
            r#"x = { const = "label", type = "u8", SIZE = 3 }"#,
            "Array/string is smaller than defined size (strict SIZE)",
        ),
        (
            r#"x = { value = [1, 2, 3], type = "u16", SIZE = 5 }"#,
            "Array/string is smaller than defined size (strict SIZE)",
        ),
    ] {
        cases.push((literal_layout(data), expected));
    }

    let mut mismatches = Vec::new();
    for (layout, expected) in cases {
        for (command, outcome) in ["build", "header", "fingerprint"]
            .into_iter()
            .zip(outcomes("literal_shape_invalid", &layout))
        {
            match outcome {
                Err(message) if message.contains(expected) => {}
                other => mismatches.push(format!("{command} for {expected:?}: {other:?}")),
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
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
