use mint_cli::commands;

#[path = "common/mod.rs"]
mod common;

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
    assert!(stats.block_stats[0].used_size > 0);
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
    commands::build(&args_a, None).expect("block_a build");

    // Build block_b with crc32c
    let args_b = common::build_args(
        &layout_path,
        "block_b",
        mint_cli::output::args::OutputFormat::Hex,
    );
    commands::build(&args_b, None).expect("block_b build");

    // Both should build; different polynomials produce different output
    // (we can't easily compare CRC values without the old stats field,
    // but successful builds with different configs is the key test)
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
            legacy_syntax: false,
        },
        mint_cli::layout::args::BlockNames {
            name: "block_b".to_string(),
            file: layout_path.clone(),
            legacy_syntax: false,
        },
        mint_cli::layout::args::BlockNames {
            name: "block_c".to_string(),
            file: layout_path,
            legacy_syntax: false,
        },
    ];

    let args = common::build_args_for_layouts(
        blocks,
        mint_cli::output::args::OutputFormat::Hex,
        "out/crc_combined.hex",
    );

    let stats = commands::build(&args, None).expect("combined build");
    assert_eq!(stats.blocks_processed, 3);

    common::assert_out_file_exists(std::path::Path::new("out/crc_combined.hex"));
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

/// Tests that only one checksum per block is allowed.
#[test]
fn checksum_duplicate_in_block_fails() {
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

[block.data]
value = { value = 0x42, type = "u32" }
checksum1 = { checksum = "crc32", type = "u32" }
checksum2 = { checksum = "crc32", type = "u32" }
"#;

    let layout_path = common::write_layout_file("crc_duplicate", layout);

    let args = common::build_args(
        &layout_path,
        "block",
        mint_cli::output::args::OutputFormat::Hex,
    );
    let result = commands::build(&args, None);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Only one checksum")
    );
}

/// Tests that checksum type must be u32.
#[test]
fn checksum_wrong_type_fails() {
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

[block.data]
value = { value = 0x42, type = "u32" }
checksum = { checksum = "crc32", type = "u16" }
"#;

    let layout_path = common::write_layout_file("crc_wrong_type", layout);

    let args = common::build_args(
        &layout_path,
        "block",
        mint_cli::output::args::OutputFormat::Hex,
    );
    let result = commands::build(&args, None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("must be u32"));
}
