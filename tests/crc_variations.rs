use mint_cli::commands;
use mint_cli::layout::settings::ChecksumConfig;
use mint_cli::output::checksum::calculate_crc;

#[path = "common/mod.rs"]
mod common;

fn standard_crc32() -> ChecksumConfig {
    ChecksumConfig {
        polynomial: 0x04C11DB7,
        start: 0xFFFFFFFF,
        xor_out: 0xFFFFFFFF,
        ref_in: true,
        ref_out: true,
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join("")
}

/// Tests inline checksum placement within block data.
#[test]
fn inline_checksum_basic() {
    common::ensure_out_dir();

    let layout = r#"
[mint]
endianness = "little"

[mint.checksum.crc32]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true

[block.header]
start_address = 0x1000
length = 0x100
padding = 0xFF

[block.data]
value1 = { value = 0x12345678, type = "u32" }
value2 = { value = "test", type = "u8", size = 8 }
checksum = { checksum = "crc32", type = "u32" }
"#;

    let layout_path = common::write_layout_file("crc_inline_basic", layout);

    let args = common::build_args(
        &layout_path,
        "block",
        mint_cli::output::args::OutputFormat::Hex,
    );
    let stats = commands::build(&args, None).expect("inline checksum build");
    assert_eq!(stats.blocks_processed, 1);
    let expected = calculate_crc(
        &[
            0x78, 0x56, 0x34, 0x12, b't', b'e', b's', b't', 0xFF, 0xFF, 0xFF, 0xFF,
        ],
        &standard_crc32(),
    );
    assert_eq!(stats.block_stats[0].checksum_values, vec![expected]);
}

/// Tests that a block without checksum builds cleanly.
#[test]
fn no_checksum_block() {
    common::ensure_out_dir();

    let layout = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x4000
length = 0x100
padding = 0xFF

[block.data]
value1 = { value = 0x11223344, type = "u32" }
"#;

    let layout_path = common::write_layout_file("crc_no_checksum", layout);

    let args = common::build_args(
        &layout_path,
        "block",
        mint_cli::output::args::OutputFormat::Hex,
    );
    let stats = commands::build(&args, None).expect("no checksum build");
    assert_eq!(stats.blocks_processed, 1);
}

/// Tests that different named checksum configs produce different CRC values.
#[test]
fn named_checksum_configs_differ() {
    common::ensure_out_dir();

    let layout = r#"
[mint]
endianness = "little"

[mint.checksum.crc32]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true

[mint.checksum.crc32c]
polynomial = 0x1EDC6F41
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true

[block_a.header]
start_address = 0x5000
length = 0x100
padding = 0xFF

[block_a.data]
value = { value = 0x12345678, type = "u32" }
checksum = { checksum = "crc32", type = "u32" }

[block_b.header]
start_address = 0x6000
length = 0x100
padding = 0xFF

[block_b.data]
value = { value = 0x12345678, type = "u32" }
checksum = { checksum = "crc32c", type = "u32" }
"#;

    let layout_path = common::write_layout_file("crc_named_configs", layout);

    // Build block_a with crc32
    let args_a = common::build_args(
        &layout_path,
        "block_a",
        mint_cli::output::args::OutputFormat::Hex,
    );
    let stats_a = commands::build(&args_a, None).expect("block_a build");

    // Build block_b with crc32c
    let args_b = common::build_args(
        &layout_path,
        "block_b",
        mint_cli::output::args::OutputFormat::Hex,
    );
    let stats_b = commands::build(&args_b, None).expect("block_b build");

    assert_ne!(
        stats_a.block_stats[0].checksum_values,
        stats_b.block_stats[0].checksum_values
    );
}

/// Tests combined output with mixed checksum configurations.
#[test]
fn checksum_combined_output() {
    common::ensure_out_dir();

    let layout = r#"
[mint]
endianness = "little"

[mint.checksum.crc32]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true

[block_a.header]
start_address = 0x10000
length = 0x100
padding = 0xFF

[block_a.data]
value = { value = 0x11111111, type = "u32" }
checksum = { checksum = "crc32", type = "u32" }

[block_b.header]
start_address = 0x11000
length = 0x100
padding = 0xAA

[block_b.data]
value = { value = 0x22222222, type = "u32" }
checksum = { checksum = "crc32", type = "u32" }

[block_c.header]
start_address = 0x12000
length = 0x100
padding = 0x00

[block_c.data]
value = { value = 0x33333333, type = "u32" }
"#;

    let layout_path = common::write_layout_file("crc_combined", layout);

    let blocks = vec![
        mint_cli::layout::args::BlockNames {
            name: "block_a".to_string(),
            file: layout_path.clone(),
        },
        mint_cli::layout::args::BlockNames {
            name: "block_b".to_string(),
            file: layout_path.clone(),
        },
        mint_cli::layout::args::BlockNames {
            name: "block_c".to_string(),
            file: layout_path,
        },
    ];

    let args = common::build_args_for_layouts(
        blocks,
        mint_cli::output::args::OutputFormat::Hex,
        "crc_combined",
    );

    let stats = commands::build(&args, None).expect("combined build");
    assert_eq!(stats.blocks_processed, 3);
    assert_eq!(stats.block_stats[0].checksum_values.len(), 1);
    assert_eq!(stats.block_stats[1].checksum_values.len(), 1);
    assert!(stats.block_stats[2].checksum_values.is_empty());

    common::assert_out_file_exists(&args.output.out);
}

/// Tests that referencing a non-existent checksum config fails.
#[test]
fn checksum_unknown_config_fails() {
    common::ensure_out_dir();

    let layout = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x1000
length = 0x100

[block.data]
value = { value = 0x42, type = "u32" }
checksum = { checksum = "nonexistent", type = "u32" }
"#;

    let layout_path = common::write_layout_file("crc_bad_config", layout);

    let args = common::build_args(
        &layout_path,
        "block",
        mint_cli::output::args::OutputFormat::Hex,
    );
    let result = commands::build(&args, None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

/// Tests that multiple checksums are resolved in field order after refs.
#[test]
fn multiple_checksums_in_block_are_resolved_in_order() {
    common::ensure_out_dir();

    let layout = r#"
[mint]
endianness = "little"

[mint.checksum.crc32]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true

[block.header]
start_address = 0x1000
length = 0x100
padding = 0xFF

[block.data]
value = { value = 0x42, type = "u32" }
checksum1 = { checksum = "crc32", type = "u32" }
tail = { value = 0x1234, type = "u16" }
checksum2 = { checksum = "crc32", type = "u32" }
"#;

    let layout_path = common::write_layout_file("crc_multiple", layout);

    let args = common::build_args(
        &layout_path,
        "block",
        mint_cli::output::args::OutputFormat::Hex,
    );
    let stats = commands::build(&args, None).expect("multiple checksum build");

    let crc1 = calculate_crc(&[0x42, 0x00, 0x00, 0x00], &standard_crc32());
    let mut crc2_input = vec![0x42, 0x00, 0x00, 0x00];
    crc2_input.extend_from_slice(&crc1.to_le_bytes());
    crc2_input.extend_from_slice(&[0x34, 0x12, 0xFF, 0xFF]);
    let crc2 = calculate_crc(&crc2_input, &standard_crc32());
    assert_eq!(stats.block_stats[0].checksum_values, vec![crc1, crc2]);

    let output = std::fs::read_to_string(&args.output.out).expect("read hex output");
    let expected_bytes = format!(
        "42000000{}3412FFFF{}",
        hex_bytes(&crc1.to_le_bytes()),
        hex_bytes(&crc2.to_le_bytes())
    );
    assert!(output.to_uppercase().contains(&expected_bytes));
}

/// Tests that checksum type must be u32.
#[test]
fn checksum_wrong_type_fails() {
    common::ensure_out_dir();

    let layout_prefix = r#"
[mint]
endianness = "little"

[mint.checksum.crc32]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true

[block.header]
start_address = 0x1000
length = 0x100
"#;

    for type_name in ["u16", "i32", "f32"] {
        let layout = format!(
            "{layout_prefix}\n[block.data]\nvalue = {{ value = 0x42, type = \"u32\" }}\nchecksum = {{ checksum = \"crc32\", type = \"{type_name}\" }}\n"
        );
        let layout_path = common::write_layout_file("crc_wrong_type", &layout);
        let args = common::build_args(
            &layout_path,
            "block",
            mint_cli::output::args::OutputFormat::Hex,
        );
        let result = commands::build(&args, None);
        assert!(result.is_err(), "{type_name} should be rejected");
        assert!(result.unwrap_err().to_string().contains("must be u32"));
    }
}
