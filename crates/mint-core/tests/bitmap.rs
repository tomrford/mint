#[path = "common/mod.rs"]
mod common;

fn bitmap_layout(data_content: &str) -> String {
    format!(
        r#"
[mint]
abi = "generic-le"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
{data_content}
"#
    )
}

fn build_bitmap(
    name: &str,
    data_content: &str,
    strict: bool,
) -> Result<Vec<u8>, mint_core::error::MintError> {
    let layout = bitmap_layout(data_content);
    let path = common::write_layout_file(name, &layout);
    common::build_block(path, "block", strict, None)
}

#[test]
fn bitmap_storage_packs_literal_signed_and_high_bit_values() {
    let bytes = build_bitmap(
        "bitmap-storage",
        r#"u8_fields = { type = "u8", bitmap = [
    { bits = 1, value = 1 },
    { bits = 2, value = 3 },
    { bits = 5, value = 21 },
] }
u16_fields = { type = "u16", bitmap = [
    { bits = 8, value = 0xAB },
    { bits = 8, value = 0xCD },
] }
signed_fields = { type = "i16", bitmap = [
    { bits = 4, value = -1 },
    { bits = 4, value = -8 },
    { bits = 8, value = 0 },
] }
u32_fields = { type = "u32", bitmap = [
    { bits = 1, value = true },
    { bits = 8, value = 255 },
    { bits = 23, value = 0 },
] }
i8_high_bit = { type = "i8", bitmap = [
    { bits = 8, value = -1 },
] }
i16_high_bit = { type = "i16", bitmap = [
    { bits = 16, value = -32768 },
] }"#,
        false,
    )
    .expect("bitmap storage should build");

    assert_eq!(
        &bytes[..16],
        &[
            0xAF, 0x00, 0xAB, 0xCD, 0x8F, 0x00, 0x00, 0x00, 0xFF, 0x01, 0x00, 0x00, 0xFF, 0x00,
            0x00, 0x80,
        ]
    );
}

#[test]
fn bitmap_out_of_range_value_saturates_or_errors_by_strictness() {
    let data_content = r#"field = { type = "u8", bitmap = [
    { bits = 3, value = 10 },
    { bits = 5, value = 0 },
] }"#;

    let bytes = build_bitmap("bitmap-saturation", data_content, false)
        .expect("non-strict bitmap value should saturate");
    assert_eq!(bytes[0], 7);

    let error = build_bitmap("bitmap-strict", data_content, true)
        .expect_err("strict bitmap value should be rejected");
    let chain = common::error_chain(&error);
    assert!(
        chain.contains("bitfield value 10 out of range for 3-bit unsigned field (0..=7)"),
        "unexpected error: {chain}"
    );
}

#[test]
fn bitmap_validation_rejects_invalid_shapes() {
    let cases = [
        (
            "wrong-bit-sum",
            r#"field = { type = "u8", bitmap = [
    { bits = 3, value = 0 },
    { bits = 4, value = 0 },
] }"#,
            "Bitmap total bits (7) must equal storage width (8).",
        ),
        (
            "zero-bit-field",
            r#"field = { type = "u8", bitmap = [
    { bits = 0, value = 0 },
    { bits = 8, value = 0 },
] }"#,
            "Bitmap field bits must be > 0.",
        ),
        (
            "float-storage",
            r#"field = { type = "f32", bitmap = [
    { bits = 16, value = 0 },
    { bits = 16, value = 0 },
] }"#,
            "Bitmap requires integer storage type.",
        ),
        (
            "size-key",
            r#"field = { type = "u8", size = 2, bitmap = [
    { bits = 8, value = 0 },
] }"#,
            "size/SIZE keys are forbidden with bitmap.",
        ),
        (
            "field-wider-than-storage",
            r#"field = { type = "u64", bitmap = [
    { bits = 9223372036854775807, value = 0 },
    { bits = 9223372036854775807, value = 0 },
    { bits = 66, value = 0 },
] }"#,
            "Bitmap field bits (9223372036854775807) exceed storage width (64).",
        ),
    ];

    for (name, data_content, expected) in cases {
        let error = build_bitmap(name, data_content, false)
            .expect_err("invalid bitmap shape should be rejected");
        let chain = common::error_chain(&error);
        assert!(
            chain.contains(expected),
            "{name}: unexpected error: {chain}"
        );
    }
}
