#[path = "common/mod.rs"]
mod common;

fn build_block(name: &str, toml: &str) -> Vec<u8> {
    let path = common::write_layout_file(name, toml);
    common::build_block(&path, "block", false, None).expect("build succeeds")
}

fn const_layout(data: &str) -> String {
    format!(
        r#"
[mint]
endianness = "little"

[mint.const]
magic = 0xDEADBEEF
enabled = true
voltage = 3.5
label = "AB"
octets = [192, 168, 1, 10]

[block.header]
start_address = 0x1000
length = 0x40
padding = 0xFF

[block.data]
{data}
"#
    )
}

#[test]
fn consts_reuse_value_source_shapes() {
    let bytes = build_block(
        "const_shapes",
        &const_layout(
            r#"
magic = { const = "magic", type = "u32" }
enabled = { const = "enabled", type = "u8" }
voltage = { const = "voltage", type = "f32" }
label = { const = "label", type = "u8", size = 4 }
octets = { const = "octets", type = "u8", size = 4 }
base = { const = "block.start_address", type = "u32" }
len = { const = "block.length", type = "u32" }
"#,
        ),
    );

    let mut expected = Vec::new();
    expected.extend(0xDEADBEEFu32.to_le_bytes());
    expected.push(1);
    expected.extend([0xFF, 0xFF, 0xFF]);
    expected.extend(3.5f32.to_le_bytes());
    expected.extend([b'A', b'B', 0xFF, 0xFF]);
    expected.extend([192, 168, 1, 10]);
    expected.extend(0x1000u32.to_le_bytes());
    expected.extend(0x40u32.to_le_bytes());

    assert_eq!(bytes, expected);
}

#[test]
fn const_unknown_name_lists_available_consts() {
    let path = common::write_layout_file(
        "const_unknown",
        &const_layout(
            r#"
field = { const = "missing", type = "u32" }
"#,
        ),
    );
    let err = common::build_block(&path, "block", false, None).unwrap_err();
    let message = common::error_chain(&err);

    assert!(message.contains("Const 'missing' not found"), "{message}");
    assert!(message.contains("magic"), "{message}");
    assert!(message.contains("block.start_address"), "{message}");
}

#[test]
fn const_rejects_scalar_size() {
    let path = common::write_layout_file(
        "const_scalar_size",
        &const_layout(
            r#"
field = { const = "magic", type = "u8", size = 4 }
"#,
        ),
    );
    let err = common::build_block(&path, "block", false, None).unwrap_err();
    let chain = common::error_chain(&err);

    assert!(chain.contains("scalar const"), "{chain}");
}

#[test]
fn const_rejects_collision_with_promoted_header_name() {
    let path = common::write_layout_file(
        "const_collision",
        r#"
[mint]
endianness = "little"

[mint.const]
"block.start_address" = 0x2000

[block.header]
start_address = 0x1000
length = 0x40

[block.data]
field = { value = 1, type = "u8" }
"#,
    );
    let err = mint_core::layout::load_layout(&path).expect_err("layout should be rejected");

    assert!(err.to_string().contains("collides"), "{err}");
}

#[test]
fn const_rejects_nested_tables_during_deserialization() {
    let path = common::write_layout_file(
        "const_nested",
        r#"
[mint]
endianness = "little"

[mint.const.block]
length = 0x40

[block.header]
start_address = 0x1000
length = 0x40

[block.data]
field = { value = 1, type = "u8" }
"#,
    );
    let err = mint_core::layout::load_layout(&path).expect_err("layout should be rejected");

    assert!(err.to_string().contains("failed to parse file"), "{err}");
}
