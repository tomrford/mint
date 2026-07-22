use mint_cli::commands;
use mint_core::build::{BlockStat, BuildStats};
use std::path::PathBuf;
use std::process::Command;

#[path = "common/mod.rs"]
mod common;

#[test]
fn test_block_stat_collection() {
    common::ensure_out_dir();

    let layout_path = "../mint-core/tests/data/blocks.toml";

    let ds = common::find_working_datasource();

    let args = common::build_args(layout_path, "block", mint_core::output::OutputFormat::Hex);

    let stats = commands::build(&args, Some(ds.as_ref())).expect("build should succeed");

    assert_eq!(stats.blocks_processed, 1);
    let block_stat = &stats.block_stats[0];
    assert_eq!(block_stat.layout, PathBuf::from(layout_path));
    assert_eq!(block_stat.block, "block");
    assert!(block_stat.allocated_size > 0);
    assert!(block_stat.reserved_size > 0);
    assert!(block_stat.reserved_size <= block_stat.allocated_size);
    assert_eq!(block_stat.checksum_values.len(), 1);
}

#[test]
fn test_build_stats_aggregation() {
    common::ensure_out_dir();

    let layout_path = "../mint-core/tests/data/blocks.toml";

    let ds = common::find_working_datasource();

    let cfg = mint_core::layout::load_layout(layout_path).expect("layout loads");
    let block_inputs = cfg
        .blocks
        .keys()
        .take(2)
        .map(|name| mint_core::build::BlockSelector::named(layout_path, name))
        .collect::<Vec<_>>();

    if block_inputs.is_empty() {
        return;
    }

    let args = common::build_args_for_layouts(
        block_inputs.clone(),
        mint_core::output::OutputFormat::Hex,
        "stats_aggregation",
    );

    let stats = commands::build(&args, Some(ds.as_ref())).expect("build should succeed");

    assert_eq!(stats.blocks_processed, block_inputs.len());
    assert!(stats.total_allocated > 0);
    assert!(stats.total_reserved > 0);
    assert!(stats.total_reserved <= stats.total_allocated);
    assert_eq!(stats.block_stats.len(), block_inputs.len());

    let manual_total_allocated: u64 = stats
        .block_stats
        .iter()
        .map(|b| u64::from(b.allocated_size))
        .sum();
    let manual_total_reserved: u64 = stats
        .block_stats
        .iter()
        .map(|b| u64::from(b.reserved_size))
        .sum();

    assert_eq!(stats.total_allocated, manual_total_allocated);
    assert_eq!(stats.total_reserved, manual_total_reserved);
}

#[test]
fn test_space_reserved_pct_calculation() {
    let mut stats = BuildStats::new();

    stats.add_block(BlockStat {
        layout: PathBuf::from("layout.toml"),
        block: "test1".to_owned(),
        start_address: 0x1000,
        address_unit_bits: 8,
        allocated_size: 100,
        reserved_size: 80,
        checksum_values: vec![0x1234_5678],
    });

    stats.add_block(BlockStat {
        layout: PathBuf::from("layout.toml"),
        block: "test2".to_owned(),
        start_address: 0x2000,
        address_unit_bits: 8,
        allocated_size: 200,
        reserved_size: 120,
        checksum_values: vec![0x9ABC_DEF0],
    });

    assert_eq!(stats.blocks_processed, 2);
    assert_eq!(stats.total_allocated, 300);
    assert_eq!(stats.total_reserved, 200);

    let space_reserved_pct = stats.space_reserved_pct();
    let expected = (200.0 / 300.0) * 100.0;
    assert!((space_reserved_pct - expected).abs() < 0.01);
}

#[test]
fn test_c28x_stats_report_target_address_lengths() {
    let stat = BlockStat {
        layout: PathBuf::from("c28x.toml"),
        block: "block".to_owned(),
        start_address: 0x1000,
        address_unit_bits: 16,
        allocated_size: 0x100,
        reserved_size: 0x80,
        checksum_values: Vec::new(),
    };

    assert_eq!(stat.allocated_address_units(), 0x80);
}

#[test]
fn c28x_detailed_stats_label_units_and_use_target_address_range() {
    let layout = common::write_layout_file(
        "c28x_stats",
        r#"
[mint]
abi = "ti-c28x-eabi"

[block.header]
start_address = 0x1000
length = 0x100

[block.data]
value = { value = 1, type = "u16" }
"#,
    );
    let output_path = common::unique_out_path("c28x_stats", "hex");
    let output = Command::new(env!("CARGO_BIN_EXE_mint"))
        .args(["build", &layout, "--stats", "-o"])
        .arg(output_path)
        .output()
        .expect("mint build should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(stdout.contains("Address Range (target units)"), "{stdout}");
    assert!(stdout.contains("Reserved/Allocated (bytes)"), "{stdout}");
    assert!(stdout.contains("0x1000-0x107F"), "{stdout}");
    assert!(!stdout.contains("0x1000-0x10FF"), "{stdout}");
}

#[test]
fn test_multi_block_stats() {
    common::ensure_out_dir();

    let layout_path = "../mint-core/tests/data/blocks.toml";

    let ds = common::find_working_datasource();

    let cfg = mint_core::layout::load_layout(layout_path).expect("layout loads");
    let block_inputs = cfg
        .blocks
        .keys()
        .map(|name| mint_core::build::BlockSelector::named(layout_path, name))
        .collect::<Vec<_>>();

    if block_inputs.is_empty() {
        return;
    }

    let args = common::build_args_for_layouts(
        block_inputs.clone(),
        mint_core::output::OutputFormat::Hex,
        "multi_block_stats",
    );

    let stats = commands::build(&args, Some(ds.as_ref())).expect("build should succeed");

    assert_eq!(stats.blocks_processed, block_inputs.len());

    for block_stat in &stats.block_stats {
        assert!(block_stat.allocated_size > 0);
        assert!(block_stat.reserved_size > 0);
        assert!(block_stat.reserved_size <= block_stat.allocated_size);
    }
}

#[test]
fn test_space_reserved_pct_edge_cases() {
    let mut stats = BuildStats::new();
    assert_eq!(stats.space_reserved_pct(), 0.0);

    stats.add_block(BlockStat {
        layout: PathBuf::from("layout.toml"),
        block: "full".to_owned(),
        start_address: 0x1000,
        address_unit_bits: 8,
        allocated_size: 100,
        reserved_size: 100,
        checksum_values: Vec::new(),
    });

    let space_reserved_pct = stats.space_reserved_pct();
    assert!((space_reserved_pct - 100.0).abs() < 0.01);
}

#[test]
fn test_no_checksum_section_returns_empty_crc_values() {
    common::ensure_out_dir();

    let layout_content = r#"
[mint]
abi = "generic-le"

[block_no_crc.header]
start_address = 0x1000
length = 0x100
padding = 0xFF

[block_no_crc.data]
device.id = { value = 0x1234, type = "u32" }
device.name = { value = "TestDevice", type = "u8", size = 16 }
"#;

    let layout_path = common::write_layout_file("test_no_crc", layout_content);

    let args = common::build_args(
        &layout_path,
        "block_no_crc",
        mint_core::output::OutputFormat::Hex,
    );

    let stats = commands::build(&args, None).expect("build should succeed");

    assert_eq!(stats.blocks_processed, 1);
    let block_stat = &stats.block_stats[0];
    assert_eq!(block_stat.layout, PathBuf::from(&layout_path));
    assert_eq!(block_stat.block, "block_no_crc");
    assert!(block_stat.checksum_values.is_empty());
}

#[test]
fn test_missing_block_returns_error_instead_of_panicking() {
    common::ensure_out_dir();

    let args = common::build_args(
        "../../doc/examples/block.toml",
        "block",
        mint_core::output::OutputFormat::Hex,
    );

    let error = commands::build(&args, None).expect_err("missing block should return an error");

    assert_eq!(
        error.to_string(),
        "block not found: 'block' in '../../doc/examples/block.toml'. Available blocks: config, data"
    );
}
