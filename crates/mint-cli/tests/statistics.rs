use mint_cli::commands;
use mint_core::build::{BlockStat, BuildStats};
use std::path::PathBuf;

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

    let manual_total_allocated: usize = stats
        .block_stats
        .iter()
        .map(|b| b.allocated_size as usize)
        .sum();
    let manual_total_reserved: usize = stats
        .block_stats
        .iter()
        .map(|b| b.reserved_size as usize)
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
        allocated_size: 100,
        reserved_size: 80,
        checksum_values: vec![0x1234_5678],
    });

    stats.add_block(BlockStat {
        layout: PathBuf::from("layout.toml"),
        block: "test2".to_owned(),
        start_address: 0x2000,
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
endianness = "little"

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
