#[path = "common/mod.rs"]
mod common;

fn parse_err(source: &str) -> String {
    mint_core::layout::parse_toml_layout(source)
        .expect_err("layout should be rejected")
        .to_string()
}

fn layout(types: &str, data: &str) -> String {
    format!(
        r#"
[mint]
abi = "generic-le"

{types}

[block.header]
start_address = 0x1000
length = 0x40

[block.data]
{data}
"#
    )
}

#[test]
fn equal_aggregates_with_different_sources_parse_and_build() {
    let source = layout(
        r#"
[mint.const]
shared = 7

[mint.types]
sample_t = ["block.object_a", "block.object_b"]
"#,
        r#"
object_a.id = { value = 1, type = "u32" }
object_a.flags = { type = "u8", bitmap = [
    { bits = 1, value = 1 },
    { bits = 7, value = 0 },
] }
object_b.id = { const = "shared", type = "u32" }
object_b.flags = { type = "u8", bitmap = [
    { bits = 1, value = 0 },
    { bits = 7, value = 1 },
] }
"#,
    );
    let path = common::write_layout_file("named_types_build", &source);
    let bytes = common::build_block(&path, "block", false, None).expect("build succeeds");
    assert_eq!(&bytes[0..4], &1u32.to_le_bytes());
    assert_eq!(&bytes[8..12], &7u32.to_le_bytes());
}

#[test]
fn rejects_shape_mismatches() {
    let cases = [
        (
            "member-type",
            r#"sample_t = ["block.object_a", "block.object_b"]"#,
            r#"
object_a.id = { value = 1, type = "u32" }
object_b.id = { value = 1, type = "u16" }
"#,
            "different shapes",
        ),
        (
            "member-name",
            r#"sample_t = ["block.object_a", "block.object_b"]"#,
            r#"
object_a.id = { value = 1, type = "u32" }
object_b.code = { value = 1, type = "u32" }
"#,
            "member names or order",
        ),
        (
            "array-size",
            r#"sample_t = ["block.object_a", "block.object_b"]"#,
            r#"
object_a.name = { value = "A", type = "u8", size = 8 }
object_b.name = { value = "B", type = "u8", size = 4 }
"#,
            "array dimensions",
        ),
        (
            "bitmap-name",
            r#"sample_t = ["block.object_a", "block.object_b"]"#,
            r#"
object_a.flags = { type = "u8", bitmap = [{ bits = 8, name = "Enable" }] }
object_b.flags = { type = "u8", bitmap = [{ bits = 8, name = "Ready" }] }
"#,
            "bitmap regions differ",
        ),
        (
            "padding-order",
            r#"sample_t = ["block.object_a", "block.object_b"]"#,
            r#"
object_a.head = { value = 1, type = "u8" }
object_a.wide = { value = 2, type = "u32" }
object_b.wide = { value = 2, type = "u32" }
object_b.head = { value = 1, type = "u8" }
"#,
            "member names or order",
        ),
    ];

    for (name, types, data, expected) in cases {
        let message = parse_err(&layout(&format!("[mint.types]\n{types}"), data));
        assert!(
            message.contains(expected),
            "{name}: expected '{expected}' in {message}"
        );
    }
}

#[test]
fn rejects_invalid_type_paths_and_names() {
    let cases = [
        (
            "leaf",
            r#"sample_t = ["block.version"]"#,
            r#"version = { value = 1, type = "u16" }"#,
            "is a leaf",
        ),
        (
            "missing",
            r#"sample_t = ["block.missing"]"#,
            r#"object_a.id = { value = 1, type = "u32" }"#,
            "not found",
        ),
        (
            "whole-block",
            r#"sample_t = ["block"]"#,
            r#"object_a.id = { value = 1, type = "u32" }"#,
            "must be 'block.aggregate'",
        ),
        (
            "empty",
            r#"sample_t = []"#,
            r#"object_a.id = { value = 1, type = "u32" }"#,
            "at least one aggregate path",
        ),
        (
            "duplicate-path",
            r#"sample_t = ["block.object_a", "block.object_a"]"#,
            r#"object_a.id = { value = 1, type = "u32" }"#,
            "more than once",
        ),
        (
            "two-types",
            r#"sample_t = ["block.object_a"]
other_t = ["block.object_a"]"#,
            r#"object_a.id = { value = 1, type = "u32" }"#,
            "assigned to both",
        ),
        (
            "block-typedef",
            r#"block_t = ["block.object_a"]"#,
            r#"object_a.id = { value = 1, type = "u32" }"#,
            "collides with generated typedef",
        ),
        (
            "keyword",
            r#"struct = ["block.object_a"]"#,
            r#"object_a.id = { value = 1, type = "u32" }"#,
            "is a C keyword",
        ),
    ];

    for (name, types, data, expected) in cases {
        let message = parse_err(&layout(&format!("[mint.types]\n{types}"), data));
        assert!(
            message.contains(expected),
            "{name}: expected '{expected}' in {message}"
        );
    }
}

#[test]
fn accepts_cross_block_types() {
    let source = r#"
[mint]
abi = "generic-le"

[mint.types]
channel_t = ["config.left", "data.right"]

[config.header]
start_address = 0
length = 0x20

[config.data]
left.id = { value = 1, type = "u32" }

[data.header]
start_address = 0x20
length = 0x20

[data.data]
right.id = { value = 2, type = "u32" }
"#;
    mint_core::layout::parse_toml_layout(source).expect("cross-block type parses");
}
