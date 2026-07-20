#[path = "common/mod.rs"]
mod common;

use std::fs;

fn write_layout(file_stem: &str, ext: &str, contents: &str) -> String {
    let path = common::unique_out_path(file_stem, ext);
    fs::write(&path, contents).expect("write layout file");
    path.to_string_lossy().into_owned()
}

fn layout_error(file_stem: &str, block_name: &str, data: &str) -> String {
    let layout = format!(
        r#"
[mint]
endianness = "little"

[{block_name}.header]
start_address = 0x1000
length = 0x20

[{block_name}.data]
{data}
"#
    );
    let path = write_layout(file_stem, "toml", &layout);
    mint_core::layout::load_layout(&path)
        .expect_err("layout should be rejected")
        .to_string()
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
fn toml_rejects_unknown_block_key() {
    let path = write_layout(
        "unknown-block-key",
        "toml",
        r#"
[mint]
endianness = "little"

[block]
unexpected = true

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
        message.contains("unknown field") && message.contains("unexpected"),
        "expected unknown-field error, got: {message}"
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
fn leaf_errors_preserve_the_field_and_location() {
    let error = layout_error(
        "unknown-scalar-type",
        "block",
        r#"bad_field = { value = 1, type = "u33" }"#,
    );
    assert!(error.contains("unknown scalar type 'u33'"), "{error}");
    assert!(error.contains("bad_field"), "{error}");
    assert!(
        error.contains("line") && error.contains("column"),
        "{error}"
    );
}

#[test]
fn leaf_rejects_unknown_missing_and_multiple_keys() {
    let unknown = layout_error(
        "unknown-leaf-key",
        "block",
        r#"field = { value = 1, type = "u8", sizee = 4 }"#,
    );
    assert!(
        unknown.contains("unknown leaf key") && unknown.contains("sizee"),
        "{unknown}"
    );

    let missing = layout_error("missing-leaf-source", "block", r#"field = { type = "u8" }"#);
    assert!(
        missing.contains("exactly one source key") && missing.contains("found none"),
        "{missing}"
    );

    let multiple = layout_error(
        "multiple-leaf-sources",
        "block",
        r#"field = { name = "Field", value = 1, type = "u8" }"#,
    );
    assert!(
        multiple.contains("exactly one source key")
            && multiple.contains("name")
            && multiple.contains("value"),
        "{multiple}"
    );
}

#[test]
fn bitmap_field_rejects_unknown_keys() {
    let error = layout_error(
        "unknown-bitmap-key",
        "block",
        r#"flags = { type = "u8", bitmap = [{ bit = 8, value = 0 }] }"#,
    );
    assert!(
        error.contains("unknown bitmap field key") && error.contains("bit"),
        "{error}"
    );
}

#[test]
fn nested_member_named_type_is_a_branch_child() {
    let layout = r#"
[mint]
endianness = "little"
[block.header]
start_address = 0x1000
length = 0x20
[block.data]
outer.type = { value = 1, type = "u8" }
outer.value = { value = 2, type = "u8" }
"#;
    let config = mint_core::layout::parse_toml_layout(layout).expect("layout should parse");
    let block = config.blocks.get("block").expect("block present");
    let mint_core::layout::block::Entry::Branch(root) = &block.data else {
        panic!("block data should be a branch");
    };
    let mint_core::layout::block::Entry::Branch(outer) = root.get("outer").expect("outer present")
    else {
        panic!("outer should be a branch");
    };
    assert!(matches!(
        outer.get("type"),
        Some(mint_core::layout::block::Entry::Leaf(_))
    ));
}

#[test]
fn parse_rejects_quoted_dotted_keys() {
    let error = layout_error(
        "aliased-paths",
        "block",
        r#""a.b" = { value = 0x11, type = "u32" }"#,
    );
    assert!(error.contains("quoted dotted field name 'a.b'"), "{error}");
    assert!(error.contains("use a nested table instead"), "{error}");
}

#[test]
fn parse_rejects_invalid_field_and_block_names() {
    for (file_stem, block_name, data, expected) in [
        (
            "keyword-field",
            "block",
            r#"for = { value = 1, type = "u8" }"#,
            "field name 'for' is a C keyword",
        ),
        (
            "invalid-field",
            "block",
            r#""not-valid" = { value = 1, type = "u8" }"#,
            "field name 'not-valid' is not a valid C identifier",
        ),
        (
            "invalid-block",
            "not-valid",
            r#"field = { value = 1, type = "u8" }"#,
            "block name 'not-valid' is not a valid C identifier",
        ),
        (
            "reserved-field",
            "block",
            r#"__value = { value = 1, type = "u8" }"#,
            "field name '__value' is reserved by C",
        ),
        (
            "reserved-block",
            "_config",
            r#"field = { value = 1, type = "u8" }"#,
            "block name '_config' is reserved by C",
        ),
    ] {
        let error = layout_error(file_stem, block_name, data);
        assert!(
            error.contains(expected),
            "expected {expected:?}, got: {error}"
        );
    }
}
