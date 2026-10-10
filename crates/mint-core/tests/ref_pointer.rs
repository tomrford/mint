#[path = "common/mod.rs"]
mod common;

fn layout(start_address: u32, abi: &str, data_content: &str) -> String {
    format!(
        r#"
[mint]
abi = "{abi}"

[block.header]
start_address = 0x{start_address:X}
length = 0x1000
padding = 0xFF

[block.data]
{data_content}
"#
    )
}
fn ref_layout(start_address: u32, data_content: &str) -> String {
    layout(start_address, "generic-le", data_content)
}

fn ref_layout_with_abi(start_address: u32, abi: &str, data_content: &str) -> String {
    layout(start_address, abi, data_content)
}

fn load_and_build(name: &str, toml_str: &str) -> Vec<u8> {
    let path = common::write_layout_file(name, toml_str);
    common::build_block(&path, "block", false, None).expect("build succeeds")
}

fn load_and_build_with_values(name: &str, toml_str: &str) -> (Vec<u8>, serde_json::Value) {
    let path = common::write_layout_file(name, toml_str);
    common::build_block_with_values(&path, "block").expect("build succeeds")
}

fn load_and_fail(name: &str, toml_str: &str) -> String {
    let path = common::write_layout_file(name, toml_str);
    let err = common::build_block(&path, "block", false, None).unwrap_err();
    common::error_chain(&err)
}

#[test]
fn ref_with_u16_type() {
    let toml = ref_layout(
        0x100,
        r#"
field_a = { value = 1, type = "u16" }
field_b = { value = 2, type = "u16" }
ptr = { ref = "field_b", type = "u16" }
"#,
    );

    let bytes = load_and_build("ref_u16", &toml);
    assert_eq!(bytes.len(), 6);
    assert_eq!(&bytes[4..6], &0x102u16.to_le_bytes());
}

#[test]
fn ref_u16_rejects_target_offset_that_pushes_address_out_of_range() {
    let toml = ref_layout(
        0xFFFC,
        r#"
prefix = { value = 0x42, type = "u32" }
target = { value = 0x24, type = "u32" }
ptr = { ref = "target", type = "u16" }
"#,
    );

    let err = load_and_fail("ref_u16_offset_overflow", &toml);
    assert!(
        err.contains("invalid layout")
            && err.contains("ref 'ptr' target 'target'")
            && err.contains("does not fit storage type u16"),
        "expected static u16 range error, got: {err}"
    );
}

#[test]
fn ref_with_u64_type() {
    let toml = ref_layout(
        0x2000,
        r#"
ptr = { ref = "target", type = "u64" }
target = { value = 0xFF, type = "u32" }
"#,
    );

    let bytes = load_and_build("ref_u64", &toml);
    assert_eq!(bytes.len(), 16);
    let expected_addr: u64 = 0x2000 + 8;
    assert_eq!(&bytes[0..8], &expected_addr.to_le_bytes());
    assert_eq!(&bytes[12..16], &[0xFF; 4]);
}

#[test]
fn ref_big_endian() {
    let toml = ref_layout_with_abi(
        0x4000,
        "generic-be",
        r#"
ptr = { ref = "target", type = "u32" }
target = { value = 0xAB, type = "u32" }
"#,
    );

    let bytes = load_and_build("ref_big_endian", &toml);
    assert_eq!(bytes.len(), 8);
    assert_eq!(&bytes[0..4], &0x4004u32.to_be_bytes());
    assert_eq!(&bytes[4..8], &0xABu32.to_be_bytes());
}

#[test]
fn refs_resolve_distinct_and_repeated_targets() {
    let toml = ref_layout(
        0x0,
        r#"
field_a = { value = 0xAA, type = "u16" }
field_b = { value = 0xBB, type = "u16" }
ptr_a = { ref = "field_a", type = "u32" }
ptr_b = { ref = "field_b", type = "u32" }
ptr_a_again = { ref = "field_a", type = "u32" }
"#,
    );

    let bytes = load_and_build("ref_distinct_and_repeated", &toml);
    assert_eq!(bytes.len(), 16);
    assert_eq!(&bytes[4..8], &0x0u32.to_le_bytes());
    assert_eq!(&bytes[8..12], &0x2u32.to_le_bytes());
    assert_eq!(&bytes[12..16], &0x0u32.to_le_bytes());
}

#[test]
fn ref_value_exported_to_json() {
    let toml = ref_layout(
        0x1000,
        r#"
target = { value = 0x42, type = "u32" }
ptr = { ref = "target", type = "u32" }
"#,
    );

    let (bytes, values) = load_and_build_with_values("ref_json_export", &toml);
    assert_eq!(&bytes[4..8], &0x1000u32.to_le_bytes());
    assert_eq!(&values["ptr"], &serde_json::json!(0x1000u64));
    assert_eq!(&values["target"], &serde_json::json!(0x42u64));
}

#[test]
fn scalar_ref_accepts_zero_and_arbitrary_literal_addresses() {
    let toml = ref_layout(
        0x1000,
        r#"
null_ptr = { ref = 0, type = "u32" }
external_ptr = { ref = 0x40001000, type = "u32" }
"#,
    );

    let (bytes, values) = load_and_build_with_values("ref_scalar_literals", &toml);
    assert_eq!(&bytes[0..4], &0u32.to_le_bytes());
    assert_eq!(&bytes[4..8], &0x40001000u32.to_le_bytes());
    assert_eq!(&values["null_ptr"], &serde_json::json!(0));
    assert_eq!(&values["external_ptr"], &serde_json::json!(0x40001000u64));
}

#[test]
fn reflist_resolves_mixed_targets_and_zero_fills_lowercase_size() {
    let toml = ref_layout(
        0x1000,
        r#"
target = { value = 0x42, type = "u16" }
ptrs = { ref = ["target", 0, 0x40001000], type = "u32", size = 5 }
"#,
    );

    let (bytes, values) = load_and_build_with_values("reflist_mixed", &toml);
    assert_eq!(bytes.len(), 24);
    assert_eq!(&bytes[4..8], &0x1000u32.to_le_bytes());
    assert_eq!(&bytes[8..12], &0u32.to_le_bytes());
    assert_eq!(&bytes[12..16], &0x40001000u32.to_le_bytes());
    assert_eq!(&bytes[16..24], &[0; 8]);
    assert_eq!(
        &values["ptrs"],
        &serde_json::json!([0x1000u64, 0, 0x40001000u64])
    );
}

#[test]
fn c28x_reflist_converts_paths_but_keeps_literals_in_word_address_units() {
    let toml = ref_layout_with_abi(
        0x1000,
        "ti-c28x-eabi",
        r#"
prefix = { value = 1, type = "u16" }
target = { value = 2, type = "u16" }
ptrs = { ref = ["target", 0x1234], type = "u32", SIZE = 2 }
"#,
    );

    let bytes = load_and_build("reflist_c28x_addresses", &toml);
    assert_eq!(bytes.len(), 12);
    assert_eq!(&bytes[4..8], &0x1001u32.to_le_bytes());
    assert_eq!(&bytes[8..12], &0x1234u32.to_le_bytes());
}

#[test]
fn reflist_rejects_invalid_targets_shapes_and_lengths() {
    let cases = [
        (
            "reflist_missing_size",
            r#"ptrs = { ref = ["target"], type = "u32" }"#,
            "requires a one-dimensional size",
        ),
        (
            "reflist_scalar_size",
            r#"ptrs = { ref = "target", type = "u32", size = 1 }"#,
            "require a ref list",
        ),
        (
            "reflist_2d",
            r#"ptrs = { ref = ["target"], type = "u32", size = [1, 1] }"#,
            "only one-dimensional",
        ),
        (
            "reflist_overfill",
            r#"ptrs = { ref = ["target", 0], type = "u32", size = 1 }"#,
            "exceeds its declared size",
        ),
        (
            "reflist_strict_underfill",
            r#"ptrs = { ref = ["target"], type = "u32", SIZE = 2 }"#,
            "smaller than its declared strict SIZE",
        ),
        (
            "reflist_empty_path",
            r#"ptrs = { ref = ["target", ""], type = "u32", SIZE = 2 }"#,
            "at index 1 must not be empty",
        ),
        (
            "reflist_missing_target",
            r#"ptrs = { ref = ["target", "missing"], type = "u32", SIZE = 2 }"#,
            "at index 1 'missing' not found",
        ),
        (
            "reflist_literal_overflow",
            r#"ptrs = { ref = [0, 0x10000], type = "u16", SIZE = 2 }"#,
            "at index 1 literal address 0x10000",
        ),
        (
            "ref_negative_literal",
            r#"ptr = { ref = -1, type = "u32" }"#,
            "ref address must be an unsigned integer; got -1",
        ),
        (
            "reflist_bool_literal",
            r#"ptrs = { ref = [0, true], type = "u32", SIZE = 2 }"#,
            "invalid ref target at index 1: ref target must be a path string or unsigned integer address; got boolean",
        ),
    ];

    for (name, field, expected) in cases {
        let layout = ref_layout(
            0,
            &format!("target = {{ value = 1, type = \"u32\" }}\n{field}"),
        );
        let error = load_and_fail(name, &layout);
        assert!(
            error.contains(expected),
            "expected '{expected}' for {name}, got: {error}"
        );
    }
}

#[test]
fn ref_rejects_invalid_configs() {
    let cases = [
        (
            "ref_err_unknown",
            ref_layout(
                0x0,
                r#"
ptr = { ref = "nonexistent", type = "u32" }
"#,
            ),
            "not found",
        ),
        (
            "ref_err_size",
            ref_layout(
                0x0,
                r#"
target = { value = 0x42, type = "u32" }
ptr = { ref = "target", type = "u32", size = 4 }
"#,
            ),
            "size",
        ),
        (
            "ref_err_float",
            ref_layout(
                0x0,
                r#"
target = { value = 0x42, type = "u32" }
ptr = { ref = "target", type = "f32" }
"#,
            ),
            "integer",
        ),
        (
            "ref_err_u8",
            ref_layout(
                0x0,
                r#"
target = { value = 0x42, type = "u32" }
ptr = { ref = "target", type = "u8" }
"#,
            ),
            "u16, u32, u64",
        ),
        (
            "ref_err_empty",
            ref_layout(
                0x0,
                r#"
ptr = { ref = "", type = "u32" }
"#,
            ),
            "empty",
        ),
        (
            "empty_branch",
            ref_layout(
                0x0,
                r#"
field = { value = 0x42, type = "u32" }

[block.data.empty]
"#,
            ),
            "empty branch",
        ),
    ];

    for (name, toml, expected) in cases {
        let err = load_and_fail(name, &toml);
        assert!(
            err.contains(expected),
            "Expected '{}' error for {}, got: {}",
            expected,
            name,
            err
        );
    }
}
