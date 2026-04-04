use std::io::Write;

use mint_cli::layout::used_values::NoopValueSink;

#[path = "common/mod.rs"]
mod common;

/// Helper to create a minimal layout with bitmap in data section
fn bitmap_layout(data_content: &str) -> String {
    format!(
        r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
{data_content}
"#
    )
}

fn build_block(
    block: &mint_cli::layout::block::Block,
    settings: &mint_cli::layout::settings::MintConfig,
    strict: bool,
) -> Result<(Vec<u8>, u32), mint_cli::layout::error::LayoutError> {
    let mut noop = NoopValueSink;
    let output = block.build_bytestream(None, settings, strict, &mut noop)?;
    Ok((output.bytestream, output.padding_count))
}

#[test]
fn bitmap_u8_literal_values() {
    common::ensure_out_dir();

    // u8 bitmap: bit0=1, bits1-2=3, bits3-7=0x15 (21)
    // Expected: 0b10101_11_1 = 0xAF
    let layout = bitmap_layout(
        r#"flags = { type = "u8", bitmap = [
    { bits = 1, value = 1 },
    { bits = 2, value = 3 },
    { bits = 5, value = 21 },
] }"#,
    );

    let path = std::path::Path::new("out").join("test_bitmap_u8.toml");
    std::fs::File::create(&path)
        .unwrap()
        .write_all(layout.as_bytes())
        .unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse");
    let block = cfg.blocks.get("block").expect("block");

    let (bytes, _) = build_block(block, &cfg.mint, false).expect("build");

    assert_eq!(bytes[0], 0xAF, "u8 bitmap packing: got {:#04x}", bytes[0]);
}

#[test]
fn bitmap_u16_little_endian() {
    common::ensure_out_dir();

    // u16 bitmap: bits0-7=0xAB, bits8-15=0xCD
    // Little endian: [0xAB, 0xCD]
    let layout = bitmap_layout(
        r#"val = { type = "u16", bitmap = [
    { bits = 8, value = 0xAB },
    { bits = 8, value = 0xCD },
] }"#,
    );

    let path = std::path::Path::new("out").join("test_bitmap_u16_le.toml");
    std::fs::File::create(&path)
        .unwrap()
        .write_all(layout.as_bytes())
        .unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse");
    let block = cfg.blocks.get("block").expect("block");

    let (bytes, _) = build_block(block, &cfg.mint, false).expect("build");

    assert_eq!(&bytes[0..2], &[0xAB, 0xCD], "u16 LE bitmap");
}

#[test]
fn bitmap_i16_signed_negative_values() {
    common::ensure_out_dir();

    // i16 bitmap with signed interpretation
    // bits0-3: -1 (4-bit signed = 0xF)
    // bits4-7: -8 (4-bit signed = 0x8, which is -8 in 4-bit two's complement)
    // bits8-15: 0
    // Result: 0x008F in little endian = [0x8F, 0x00]
    let layout = bitmap_layout(
        r#"flags = { type = "i16", bitmap = [
    { bits = 4, value = -1 },
    { bits = 4, value = -8 },
    { bits = 8, value = 0 },
] }"#,
    );

    let path = std::path::Path::new("out").join("test_bitmap_i16_signed.toml");
    std::fs::File::create(&path)
        .unwrap()
        .write_all(layout.as_bytes())
        .unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse");
    let block = cfg.blocks.get("block").expect("block");

    let (bytes, _) = build_block(block, &cfg.mint, false).expect("build");

    // -1 in 4 bits = 0xF, -8 in 4 bits = 0x8
    // Combined: 0x8F in low byte, 0x00 in high byte
    assert_eq!(
        &bytes[0..2],
        &[0x8F, 0x00],
        "i16 signed bitmap: got {:02x?}",
        &bytes[0..2]
    );
}

#[test]
fn bitmap_u32_mixed_fields() {
    common::ensure_out_dir();

    // u32: bit0=true(1), bits1-8=0xFF, bits9-31=0
    // Result: 0x000001FF in little endian
    let layout = bitmap_layout(
        r#"status = { type = "u32", bitmap = [
    { bits = 1, value = true },
    { bits = 8, value = 255 },
    { bits = 23, value = 0 },
] }"#,
    );

    let path = std::path::Path::new("out").join("test_bitmap_u32.toml");
    std::fs::File::create(&path)
        .unwrap()
        .write_all(layout.as_bytes())
        .unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse");
    let block = cfg.blocks.get("block").expect("block");

    let (bytes, _) = build_block(block, &cfg.mint, false).expect("build");

    assert_eq!(
        &bytes[0..4],
        &[0xFF, 0x01, 0x00, 0x00],
        "u32 bitmap: got {:02x?}",
        &bytes[0..4]
    );
}

#[test]
fn bitmap_saturation_non_strict() {
    common::ensure_out_dir();

    // 3-bit unsigned field, value 10 should saturate to 7
    let layout = bitmap_layout(
        r#"sat = { type = "u8", bitmap = [
    { bits = 3, value = 10 },
    { bits = 5, value = 0 },
] }"#,
    );

    let path = std::path::Path::new("out").join("test_bitmap_saturate.toml");
    std::fs::File::create(&path)
        .unwrap()
        .write_all(layout.as_bytes())
        .unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse");
    let block = cfg.blocks.get("block").expect("block");

    let (bytes, _) =
        build_block(block, &cfg.mint, false).expect("saturation should succeed in non-strict");

    assert_eq!(bytes[0], 7, "3-bit field saturates 10 to 7");
}

#[test]
fn bitmap_strict_rejects_out_of_range() {
    common::ensure_out_dir();

    // 3-bit unsigned field, value 10 is out of range (max 7)
    let layout = bitmap_layout(
        r#"bad = { type = "u8", bitmap = [
    { bits = 3, value = 10 },
    { bits = 5, value = 0 },
] }"#,
    );

    let path = std::path::Path::new("out").join("test_bitmap_strict_range.toml");
    std::fs::File::create(&path)
        .unwrap()
        .write_all(layout.as_bytes())
        .unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse");
    let block = cfg.blocks.get("block").expect("block");

    let res = build_block(block, &cfg.mint, true);
    assert!(res.is_err(), "strict mode rejects out-of-range value");
}

#[test]
fn bitmap_rejects_wrong_bit_sum() {
    common::ensure_out_dir();

    // u8 needs 8 bits, but we only provide 7
    let layout = bitmap_layout(
        r#"bad = { type = "u8", bitmap = [
    { bits = 3, value = 0 },
    { bits = 4, value = 0 },
] }"#,
    );

    let path = std::path::Path::new("out").join("test_bitmap_bad_sum.toml");
    std::fs::File::create(&path)
        .unwrap()
        .write_all(layout.as_bytes())
        .unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse");
    let block = cfg.blocks.get("block").expect("block");

    let res = build_block(block, &cfg.mint, false);
    assert!(res.is_err(), "bitmap with wrong bit sum should error");
}

#[test]
fn bitmap_rejects_zero_bits() {
    common::ensure_out_dir();

    let layout = bitmap_layout(
        r#"bad = { type = "u8", bitmap = [
    { bits = 0, value = 0 },
    { bits = 8, value = 0 },
] }"#,
    );

    let path = std::path::Path::new("out").join("test_bitmap_zero_bits.toml");
    std::fs::File::create(&path)
        .unwrap()
        .write_all(layout.as_bytes())
        .unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse");
    let block = cfg.blocks.get("block").expect("block");

    let res = build_block(block, &cfg.mint, false);
    assert!(res.is_err(), "bitmap with zero-bit field should error");
}

#[test]
fn bitmap_rejects_float_storage() {
    common::ensure_out_dir();

    let layout = bitmap_layout(
        r#"bad = { type = "f32", bitmap = [
    { bits = 16, value = 0 },
    { bits = 16, value = 0 },
] }"#,
    );

    let path = std::path::Path::new("out").join("test_bitmap_float.toml");
    std::fs::File::create(&path)
        .unwrap()
        .write_all(layout.as_bytes())
        .unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse");
    let block = cfg.blocks.get("block").expect("block");

    let res = build_block(block, &cfg.mint, false);
    assert!(res.is_err(), "bitmap with float storage should error");
}

#[test]
fn bitmap_rejects_size_key() {
    common::ensure_out_dir();

    let layout = bitmap_layout(
        r#"bad = { type = "u8", size = 2, bitmap = [
    { bits = 8, value = 0 },
] }"#,
    );

    let path = std::path::Path::new("out").join("test_bitmap_size_key.toml");
    std::fs::File::create(&path)
        .unwrap()
        .write_all(layout.as_bytes())
        .unwrap();

    let cfg = mint_cli::layout::load_layout(path.to_str().unwrap()).expect("parse");
    let block = cfg.blocks.get("block").expect("block");

    let res = build_block(block, &cfg.mint, false);
    assert!(res.is_err(), "bitmap with size key should error");
}
