#[path = "common/mod.rs"]
mod common;

fn load_config(name: &str, toml: &str) -> mint_cli::layout::block::Config {
    let path = common::write_layout_file(name, toml);
    mint_cli::layout::load_layout(&path).expect("layout loads")
}

fn build_block(name: &str, toml: &str) -> Vec<u8> {
    let config = load_config(name, toml);
    let block = &config.blocks["block"];
    let (bytes, _) = common::build_block(block, &config.mint, false, None).expect("build succeeds");
    bytes
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
    let config = load_config(
        "const_unknown",
        &const_layout(
            r#"
field = { const = "missing", type = "u32" }
"#,
        ),
    );
    let block = &config.blocks["block"];
    let err = common::build_block(block, &config.mint, false, None).unwrap_err();
    let message = err.to_string();

    assert!(message.contains("Const 'missing' not found"), "{message}");
    assert!(message.contains("magic"), "{message}");
    assert!(message.contains("block.start_address"), "{message}");
}

#[test]
fn const_rejects_scalar_size() {
    let config = load_config(
        "const_scalar_size",
        &const_layout(
            r#"
field = { const = "magic", type = "u8", size = 4 }
"#,
        ),
    );
    let block = &config.blocks["block"];
    let err = common::build_block(block, &config.mint, false, None).unwrap_err();

    assert!(err.to_string().contains("scalar const"), "{err}");
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
    let err = mint_cli::layout::load_layout(&path).expect_err("layout should be rejected");

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
    let err = mint_cli::layout::load_layout(&path).expect_err("layout should be rejected");

    assert!(err.to_string().contains("failed to parse file"), "{err}");
}
