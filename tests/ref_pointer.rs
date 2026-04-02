use mint_cli::layout::used_values::{NoopValueSink, ValueCollector};

#[path = "common/mod.rs"]
mod common;

/// Helper to create a minimal layout with given data content.
fn ref_layout(start_address: u32, data_content: &str) -> String {
    ref_layout_with_endian(start_address, "little", data_content)
}

fn ref_layout_with_endian(start_address: u32, endianness: &str, data_content: &str) -> String {
    format!(
        r#"
[settings]
endianness = "{endianness}"
virtual_offset = 0

[block.header]
start_address = 0x{start_address:X}
length = 0x1000
padding = 0xFF

[block.data]
{data_content}
"#
    )
}

fn ref_layout_with_virtual_offset(
    start_address: u32,
    virtual_offset: u32,
    data_content: &str,
) -> String {
    format!(
        r#"
[settings]
endianness = "little"
virtual_offset = 0x{virtual_offset:X}

[block.header]
start_address = 0x{start_address:X}
length = 0x1000
padding = 0xFF

[block.data]
{data_content}
"#
    )
}

fn build_block(
    block: &mint_cli::layout::block::Block,
    settings: &mint_cli::layout::settings::Settings,
) -> Result<(Vec<u8>, u32), mint_cli::layout::error::LayoutError> {
    let mut noop = NoopValueSink;
    block.build_bytestream(None, settings, false, &mut noop)
}

fn build_block_with_values(
    block: &mint_cli::layout::block::Block,
    settings: &mint_cli::layout::settings::Settings,
) -> Result<((Vec<u8>, u32), serde_json::Value), mint_cli::layout::error::LayoutError> {
    let mut collector = ValueCollector::new();
    let result = block.build_bytestream(None, settings, false, &mut collector)?;
    Ok((result, collector.into_value()))
}

fn load_and_build(name: &str, toml_str: &str) -> (Vec<u8>, u32) {
    common::ensure_out_dir();
    let path = common::write_layout_file(name, toml_str);
    let config = mint_cli::layout::load_layout(&path).expect("layout loads");
    let block = &config.blocks["block"];
    build_block(block, &config.settings).expect("build succeeds")
}

fn load_and_build_with_values(
    name: &str,
    toml_str: &str,
) -> ((Vec<u8>, u32), serde_json::Value) {
    common::ensure_out_dir();
    let path = common::write_layout_file(name, toml_str);
    let config = mint_cli::layout::load_layout(&path).expect("layout loads");
    let block = &config.blocks["block"];
    build_block_with_values(block, &config.settings).expect("build succeeds")
}

fn load_and_fail(name: &str, toml_str: &str) -> String {
    common::ensure_out_dir();
    let path = common::write_layout_file(name, toml_str);
    let config = mint_cli::layout::load_layout(&path).expect("layout loads");
    let block = &config.blocks["block"];
    let err = build_block(block, &config.settings).unwrap_err();
    format!("{}", err)
}

// --- Happy path tests ---

#[test]
fn ref_resolves_forward_pointer_u32_little_endian() {
    let toml = ref_layout(
        0x8000,
        r#"
ptr = { ref = "target", type = "u32" }
target = { value = 0xDEADBEEF, type = "u32" }
"#,
    );

    let (bytes, _) = load_and_build("ref_forward", &toml);
    assert_eq!(bytes.len(), 8);
    assert_eq!(&bytes[0..4], &0x8004u32.to_le_bytes());
    assert_eq!(&bytes[4..8], &0xDEADBEEFu32.to_le_bytes());
}

#[test]
fn ref_resolves_backward_pointer() {
    let toml = ref_layout(
        0x1000,
        r#"
target = { value = 0x42, type = "u32" }
ptr = { ref = "target", type = "u32" }
"#,
    );

    let (bytes, _) = load_and_build("ref_backward", &toml);
    assert_eq!(bytes.len(), 8);
    assert_eq!(&bytes[0..4], &0x42u32.to_le_bytes());
    assert_eq!(&bytes[4..8], &0x1000u32.to_le_bytes());
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

    let (bytes, _) = load_and_build("ref_u16", &toml);
    assert_eq!(bytes.len(), 6);
    assert_eq!(&bytes[4..6], &0x102u16.to_le_bytes());
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

    let (bytes, _) = load_and_build("ref_u64", &toml);
    // ptr: 8 bytes at offset 0, target: at offset 8
    assert_eq!(bytes.len(), 12);
    let expected_addr: u64 = 0x2000 + 8;
    assert_eq!(&bytes[0..8], &expected_addr.to_le_bytes());
}

#[test]
fn ref_big_endian() {
    let toml = ref_layout_with_endian(
        0x4000,
        "big",
        r#"
ptr = { ref = "target", type = "u32" }
target = { value = 0xAB, type = "u32" }
"#,
    );

    let (bytes, _) = load_and_build("ref_big_endian", &toml);
    assert_eq!(bytes.len(), 8);
    assert_eq!(&bytes[0..4], &0x4004u32.to_be_bytes());
    assert_eq!(&bytes[4..8], &0xABu32.to_be_bytes());
}

#[test]
fn ref_with_virtual_offset() {
    let toml = ref_layout_with_virtual_offset(
        0x1000,
        0x2000,
        r#"
ptr = { ref = "target", type = "u32" }
target = { value = 0xBB, type = "u32" }
"#,
    );

    let (bytes, _) = load_and_build("ref_voffset", &toml);
    assert_eq!(bytes.len(), 8);
    // address = start_address + virtual_offset + offset = 0x1000 + 0x2000 + 4 = 0x3004
    assert_eq!(&bytes[0..4], &0x3004u32.to_le_bytes());
}

#[test]
fn ref_to_branch_node() {
    // start_address = 0x0
    // header_field: u32 at offset 0 (4 bytes)
    // nested.a: u16 at offset 4
    // nested.b: u16 at offset 6
    // ptr: u32 at offset 8, pointing to "nested" at offset 4
    let toml = ref_layout(
        0x0,
        r#"
header_field = { value = 0x01, type = "u32" }
nested.a = { value = 0x0A, type = "u16" }
nested.b = { value = 0x0B, type = "u16" }
ptr = { ref = "nested", type = "u32" }
"#,
    );

    let (bytes, _) = load_and_build("ref_branch", &toml);
    assert_eq!(bytes.len(), 12);
    assert_eq!(&bytes[8..12], &0x4u32.to_le_bytes());
}

#[test]
fn ref_to_nested_leaf() {
    // group.x: u16 at offset 0
    // group.y: u16 at offset 2
    // ptr: u32 at offset 4, pointing to "group.y" = 0x100 + 2
    let toml = ref_layout(
        0x100,
        r#"
group.x = { value = 1, type = "u16" }
group.y = { value = 2, type = "u16" }
ptr = { ref = "group.y", type = "u32" }
"#,
    );

    let (bytes, _) = load_and_build("ref_nested_leaf", &toml);
    assert_eq!(bytes.len(), 8);
    assert_eq!(&bytes[4..8], &0x102u32.to_le_bytes());
}

#[test]
fn ref_multiple_refs_in_same_block() {
    let toml = ref_layout(
        0x0,
        r#"
field_a = { value = 0xAA, type = "u16" }
field_b = { value = 0xBB, type = "u16" }
ptr_a = { ref = "field_a", type = "u32" }
ptr_b = { ref = "field_b", type = "u32" }
"#,
    );

    let (bytes, _) = load_and_build("ref_multi", &toml);
    assert_eq!(bytes.len(), 12);
    assert_eq!(&bytes[4..8], &0x0u32.to_le_bytes());
    assert_eq!(&bytes[8..12], &0x2u32.to_le_bytes());
}

#[test]
fn ref_two_refs_same_target() {
    let toml = ref_layout(
        0x0,
        r#"
target = { value = 0x42, type = "u32" }
ptr1 = { ref = "target", type = "u32" }
ptr2 = { ref = "target", type = "u32" }
"#,
    );

    let (bytes, _) = load_and_build("ref_same_target", &toml);
    assert_eq!(bytes.len(), 12);
    assert_eq!(&bytes[4..8], &0x0u32.to_le_bytes());
    assert_eq!(&bytes[8..12], &0x0u32.to_le_bytes());
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

    let ((bytes, _), values) = load_and_build_with_values("ref_json_export", &toml);
    assert_eq!(&bytes[4..8], &0x1000u32.to_le_bytes());
    assert_eq!(&values["ptr"], &serde_json::json!(0x1000u64));
    assert_eq!(&values["target"], &serde_json::json!(0x42u64));
}

#[test]
fn ref_with_alignment_padding() {
    // u8 at offset 0, padding 3 bytes, u32 target at offset 4, u32 ptr at offset 8
    let toml = ref_layout(
        0x0,
        r#"
small = { value = 0x01, type = "u8" }
target = { value = 0xDEAD, type = "u32" }
ptr = { ref = "target", type = "u32" }
"#,
    );

    let (bytes, padding) = load_and_build("ref_align", &toml);
    assert_eq!(bytes.len(), 12);
    assert_eq!(padding, 3);
    assert_eq!(&bytes[8..12], &0x4u32.to_le_bytes());
}

// --- Error case tests ---

#[test]
fn ref_rejects_unknown_target() {
    let toml = ref_layout(
        0x0,
        r#"
ptr = { ref = "nonexistent", type = "u32" }
"#,
    );

    let err = load_and_fail("ref_err_unknown", &toml);
    assert!(err.contains("not found"), "Expected 'not found' error, got: {}", err);
}

#[test]
fn ref_rejects_size_key() {
    let toml = ref_layout(
        0x0,
        r#"
target = { value = 0x42, type = "u32" }
ptr = { ref = "target", type = "u32", size = 4 }
"#,
    );

    let err = load_and_fail("ref_err_size", &toml);
    assert!(err.contains("size") || err.contains("SIZE"), "Expected size error, got: {}", err);
}

#[test]
fn ref_rejects_float_type() {
    let toml = ref_layout(
        0x0,
        r#"
target = { value = 0x42, type = "u32" }
ptr = { ref = "target", type = "f32" }
"#,
    );

    let err = load_and_fail("ref_err_float", &toml);
    assert!(err.contains("integer"), "Expected integer type error, got: {}", err);
}

#[test]
fn ref_rejects_empty_target() {
    let toml = ref_layout(
        0x0,
        r#"
ptr = { ref = "", type = "u32" }
"#,
    );

    let err = load_and_fail("ref_err_empty", &toml);
    assert!(err.contains("empty"), "Expected empty path error, got: {}", err);
}

#[test]
fn ref_no_overhead_without_refs() {
    let toml = ref_layout(
        0x8000,
        r#"
field_a = { value = 0xAAAA, type = "u16" }
field_b = { value = 0xBBBB, type = "u16" }
"#,
    );

    let (bytes, _) = load_and_build("ref_no_refs", &toml);
    assert_eq!(bytes.len(), 4);
    assert_eq!(&bytes[0..2], &0xAAAAu16.to_le_bytes());
    assert_eq!(&bytes[2..4], &0xBBBBu16.to_le_bytes());
}
