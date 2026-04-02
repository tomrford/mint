use mint_cli::commands;

#[path = "common/mod.rs"]
mod common;

/// Tests various CRC location modes: end_data, end_block, and absolute address.
#[test]
fn crc_location_variations() {
    common::ensure_out_dir();

    // Layout with CRC settings but no default location - blocks must specify their own
    let layout = r#"
[settings]
endianness = "little"

[settings.crc]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true
area = "data"

# Block using end_data
[block_end_data.header]
start_address = 0x1000
length = 0x100
padding = 0xFF

[block_end_data.header.crc]
location = "end_data"

[block_end_data.data]
value1 = { value = 0x12345678, type = "u32" }
value2 = { value = "test", type = "u8", size = 8 }

# Block using end_block (CRC in last 4 bytes)
[block_end_block.header]
start_address = 0x2000
length = 0x100
padding = 0xAA

[block_end_block.header.crc]
location = "end_block"

[block_end_block.data]
value1 = { value = 0xDEADBEEF, type = "u32" }
value2 = { value = [1, 2, 3, 4], type = "u8", size = 4 }

# Block using absolute address
[block_absolute.header]
start_address = 0x3000
length = 0x100
padding = 0x00

[block_absolute.header.crc]
location = 0x30F0

[block_absolute.data]
value1 = { value = 0xCAFEBABE, type = "u32" }
value2 = { value = 3.14159, type = "f32" }

# Block with no CRC (no header.crc section)
[block_no_crc.header]
start_address = 0x4000
length = 0x100
padding = 0xFF

[block_no_crc.data]
value1 = { value = 0x11223344, type = "u32" }
"#;

    let layout_path = common::write_layout_file("crc_locations", layout);

    let args = common::build_args(
        &layout_path,
        "block_end_data",
        mint_cli::output::args::OutputFormat::Hex,
    );
    let stats = commands::build(&args, None).expect("block_end_data build");
    assert!(
        stats.block_stats[0].crc_value.is_some(),
        "end_data should have CRC"
    );

    let args = common::build_args(
        &layout_path,
        "block_end_block",
        mint_cli::output::args::OutputFormat::Hex,
    );
    let stats = commands::build(&args, None).expect("block_end_block build");
    assert!(
        stats.block_stats[0].crc_value.is_some(),
        "end_block should have CRC"
    );

    let args = common::build_args(
        &layout_path,
        "block_absolute",
        mint_cli::output::args::OutputFormat::Hex,
    );
    let stats = commands::build(&args, None).expect("block_absolute build");
    assert!(
        stats.block_stats[0].crc_value.is_some(),
        "absolute should have CRC"
    );

    let args = common::build_args(
        &layout_path,
        "block_no_crc",
        mint_cli::output::args::OutputFormat::Hex,
    );
    let stats = commands::build(&args, None).expect("block_no_crc build");
    assert!(
        stats.block_stats[0].crc_value.is_none(),
        "no_crc should not have CRC"
    );
}

/// Tests per-header CRC parameter overrides.
#[test]
fn crc_per_header_overrides() {
    common::ensure_out_dir();

    let layout = r#"
[settings]
endianness = "little"

[settings.crc]
location = "end_data"
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true
area = "data"

# Block using default CRC settings
[block_default.header]
start_address = 0x5000
length = 0x100
padding = 0xFF

[block_default.header.crc]
location = "end_data"

[block_default.data]
value = { value = 0x12345678, type = "u32" }

# Block with different polynomial (CRC-32C / Castagnoli)
[block_crc32c.header]
start_address = 0x6000
length = 0x100
padding = 0xFF

[block_crc32c.header.crc]
location = "end_data"
polynomial = 0x1EDC6F41

[block_crc32c.data]
value = { value = 0x12345678, type = "u32" }

# Block with CRC-32/MPEG-2 (non-reflected)
[block_mpeg2.header]
start_address = 0x7000
length = 0x100
padding = 0xFF

[block_mpeg2.header.crc]
location = "end_data"
xor_out = 0x00000000
ref_in = false
ref_out = false

[block_mpeg2.data]
value = { value = 0x12345678, type = "u32" }

# Block with fully specified CRC (no inheritance from settings)
[block_full_override.header]
start_address = 0x8000
length = 0x100
padding = 0xFF

[block_full_override.header.crc]
location = "end_block"
polynomial = 0x04C11DB7
start = 0x00000000
xor_out = 0x00000000
ref_in = false
ref_out = false
area = "data"

[block_full_override.data]
value = { value = 0x12345678, type = "u32" }
"#;

    let layout_path = common::write_layout_file("crc_overrides", layout);

    // Build all blocks and verify they produce different CRC values for same data
    let blocks = [
        "block_default",
        "block_crc32c",
        "block_mpeg2",
        "block_full_override",
    ];
    let mut crc_values = Vec::new();

    for block_name in blocks {
        let args = common::build_args(
            &layout_path,
            block_name,
            mint_cli::output::args::OutputFormat::Hex,
        );
        let stats = commands::build(&args, None).unwrap_or_else(|_| panic!("{} build", block_name));
        let crc = stats.block_stats[0]
            .crc_value
            .unwrap_or_else(|| panic!("{} should have CRC", block_name));
        crc_values.push((block_name, crc));
    }

    // Verify that different CRC settings produce different values
    assert_ne!(
        crc_values[0].1, crc_values[1].1,
        "default vs crc32c should differ"
    );
    assert_ne!(
        crc_values[0].1, crc_values[2].1,
        "default vs mpeg2 should differ"
    );
    assert_ne!(
        crc_values[0].1, crc_values[3].1,
        "default vs full_override should differ"
    );
}

/// Tests CRC with different area modes.
#[test]
fn crc_area_modes() {
    common::ensure_out_dir();

    let layout = r#"
[settings]
endianness = "little"

[settings.crc]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true
area = "data"

# Block with area = "data" (CRC over data only)
[block_data.header]
start_address = 0x9000
length = 0x100
padding = 0xFF

[block_data.header.crc]
location = "end_data"
area = "data"

[block_data.data]
value = { value = 0xAABBCCDD, type = "u32" }

# Block with area = "block_zero_crc" (pad to full block, zero CRC location)
[block_zero_crc.header]
start_address = 0xA000
length = 0x100
padding = 0xFF

[block_zero_crc.header.crc]
location = "end_data"
area = "block_zero_crc"

[block_zero_crc.data]
value = { value = 0xAABBCCDD, type = "u32" }

# Block with area = "block_pad_crc" (pad to full block, CRC location has padding)
[block_pad_crc.header]
start_address = 0xB000
length = 0x100
padding = 0xFF

[block_pad_crc.header.crc]
location = "end_data"
area = "block_pad_crc"

[block_pad_crc.data]
value = { value = 0xAABBCCDD, type = "u32" }

# Block with area = "block_omit_crc" (pad to full block, exclude CRC bytes from calculation)
[block_omit_crc.header]
start_address = 0xC000
length = 0x100
padding = 0xFF

[block_omit_crc.header.crc]
location = "end_data"
area = "block_omit_crc"

[block_omit_crc.data]
value = { value = 0xAABBCCDD, type = "u32" }
"#;

    let layout_path = common::write_layout_file("crc_areas", layout);

    let blocks = [
        "block_data",
        "block_zero_crc",
        "block_pad_crc",
        "block_omit_crc",
    ];
    let mut crc_values = Vec::new();

    for block_name in blocks {
        let args = common::build_args(
            &layout_path,
            block_name,
            mint_cli::output::args::OutputFormat::Hex,
        );
        let stats = commands::build(&args, None).unwrap_or_else(|_| panic!("{} build", block_name));
        let crc = stats.block_stats[0]
            .crc_value
            .unwrap_or_else(|| panic!("{} should have CRC", block_name));
        crc_values.push((block_name, crc));
    }

    // Different area modes should produce different CRC values
    // (because they include different data in the calculation)
    assert_ne!(
        crc_values[0].1, crc_values[1].1,
        "data vs block_zero_crc should differ"
    );
    assert_ne!(
        crc_values[1].1, crc_values[2].1,
        "block_zero_crc vs block_pad_crc should differ"
    );
}

/// Tests that settings-level location works as default for headers.
#[test]
fn crc_settings_location_inheritance() {
    common::ensure_out_dir();

    let layout = r#"
[settings]
endianness = "little"

[settings.crc]
location = "end_data"
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true
area = "data"

# Block with no header.crc section - should inherit everything from settings
[block_inherit.header]
start_address = 0xD000
length = 0x100
padding = 0xFF

[block_inherit.data]
value = { value = 0x55667788, type = "u32" }
"#;

    let layout_path = common::write_layout_file("crc_inherit", layout);

    let args = common::build_args(
        &layout_path,
        "block_inherit",
        mint_cli::output::args::OutputFormat::Hex,
    );
    let stats = commands::build(&args, None).expect("block_inherit build");
    assert!(
        stats.block_stats[0].crc_value.is_some(),
        "inherited CRC should be computed"
    );
}

/// Tests combined output with mixed CRC configurations.
#[test]
fn crc_combined_output() {
    common::ensure_out_dir();

    // No location in settings.crc - blocks must specify their own
    let layout = r#"
[settings]
endianness = "little"

[settings.crc]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true
area = "data"

[block_a.header]
start_address = 0x10000
length = 0x100
padding = 0xFF

[block_a.header.crc]
location = "end_data"

[block_a.data]
value = { value = 0x11111111, type = "u32" }

[block_b.header]
start_address = 0x11000
length = 0x100
padding = 0xAA

[block_b.header.crc]
location = "end_block"
polynomial = 0x1EDC6F41

[block_b.data]
value = { value = 0x22222222, type = "u32" }

# No header.crc section - no CRC for this block
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

    // block_a and block_b have CRC, block_c does not
    assert!(
        stats
            .block_stats
            .iter()
            .find(|b| b.name == "block_a")
            .unwrap()
            .crc_value
            .is_some()
    );
    assert!(
        stats
            .block_stats
            .iter()
            .find(|b| b.name == "block_b")
            .unwrap()
            .crc_value
            .is_some()
    );
    assert!(
        stats
            .block_stats
            .iter()
            .find(|b| b.name == "block_c")
            .unwrap()
            .crc_value
            .is_none()
    );

    common::assert_out_file_exists(std::path::Path::new("out/crc_combined.hex"));
}
