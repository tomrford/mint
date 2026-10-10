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
abi = "generic-le"

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
fn toml_rejects_unknown_mint_keys() {
    let cases = [
        (
            "unknown-mint-key",
            "abi = \"generic-le\"\nunknown = true",
            "unknown",
        ),
        ("legacy-endianness", "endianness = \"little\"", "endianness"),
        (
            "virtual-offset",
            "abi = \"generic-le\"\nvirtual_offset = 0",
            "virtual_offset",
        ),
    ];

    for (name, config, key) in cases {
        let path = write_layout(
            name,
            "toml",
            &format!(
                r#"
[mint]
{config}

[block.header]
start_address = 0x1000
length = 0x20

[block.data]
value = {{ value = 1, type = "u16" }}
"#
            ),
        );

        let message = mint_core::layout::load_layout(&path)
            .expect_err("layout should be rejected")
            .to_string();
        assert!(
            message.contains("unknown field") && message.contains(key),
            "expected unknown-field error for '{key}', got: {message}"
        );
    }
}

#[test]
fn toml_requires_a_supported_abi() {
    let missing = write_layout(
        "missing-abi",
        "toml",
        r#"
[mint]

[block.header]
start_address = 0x1000
length = 0x20

[block.data]
value = { value = 1, type = "u16" }
"#,
    );
    let error = mint_core::layout::load_layout(&missing)
        .expect_err("layout should be rejected")
        .to_string();
    assert!(
        error.contains("missing field") && error.contains("abi"),
        "{error}"
    );

    let unknown = write_layout(
        "unknown-abi",
        "toml",
        r#"
[mint]
abi = "unknown"

[block.header]
start_address = 0x1000
length = 0x20

[block.data]
value = { value = 1, type = "u16" }
"#,
    );
    let error = mint_core::layout::load_layout(&unknown)
        .expect_err("layout should be rejected")
        .to_string();
    assert!(
        error.contains("unknown ABI") && error.contains("generic-le"),
        "{error}"
    );
}

#[test]
fn toml_rejects_unknown_block_key() {
    let path = write_layout(
        "unknown-block-key",
        "toml",
        r#"
[mint]
abi = "generic-le"

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
fn nested_members_named_type_and_value_build() {
    let layout = r#"
[mint]
abi = "generic-le"
[block.header]
start_address = 0x1000
length = 0x20
[block.data]
outer.type = { value = 1, type = "u8" }
outer.value = { value = 2, type = "u8" }
"#;
    let path = common::write_layout_file("nested-source-names", layout);
    let bytes = common::build_block(&path, "block", false, None).expect("block builds");
    assert_eq!(bytes, [1, 2]);
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
