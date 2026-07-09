use mint_cli::commands;
use mint_core::build::BlockSelector;

#[path = "common/mod.rs"]
mod common;

#[test]
fn test_deduplication_file_and_specific() {
    common::ensure_out_dir();

    let layout_path = "../mint-core/tests/data/blocks.toml";

    let ds = common::find_working_datasource();

    let args = mint_cli::args::Args {
        layout: mint_cli::layout_args::LayoutArgs {
            blocks: vec![
                BlockSelector::all(layout_path),
                // Request specific block that exists in the combined file
                BlockSelector::named(layout_path, "block"),
            ],
            strict: false,
        },
        data: Default::default(),
        output: mint_cli::output_args::OutputArgs {
            out: common::unique_out_path("dedup_test", "hex"),
            record_width: 32,
            format: mint_core::output::OutputFormat::Hex,
            export_json: None,
            stats: false,
            quiet: true,
        },
    };

    let stats = commands::build(&args, Some(ds.as_ref())).expect("build should succeed");

    let cfg = mint_core::layout::load_layout(layout_path).expect("layout loads");
    assert_eq!(
        stats.blocks_processed,
        cfg.blocks.len(),
        "Should deduplicate and only build each block once"
    );
}

#[test]
fn test_file_expansion_builds_all_blocks() {
    common::ensure_out_dir();

    let layout_path = "../mint-core/tests/data/blocks.toml";

    let ds = common::find_working_datasource();

    let args = mint_cli::args::Args {
        layout: mint_cli::layout_args::LayoutArgs {
            blocks: vec![BlockSelector::all(layout_path)],
            strict: false,
        },
        data: Default::default(),
        output: mint_cli::output_args::OutputArgs {
            out: common::unique_out_path("all_blocks", "hex"),
            record_width: 32,
            format: mint_core::output::OutputFormat::Hex,
            export_json: None,
            stats: false,
            quiet: true,
        },
    };

    let stats = commands::build(&args, Some(ds.as_ref())).expect("build should succeed");

    let cfg = mint_core::layout::load_layout(layout_path).expect("layout loads");
    assert_eq!(
        stats.blocks_processed,
        cfg.blocks.len(),
        "Should build all blocks"
    );

    common::assert_out_file_exists(&args.output.out);
}
