use std::path::PathBuf;

use mint_cli::commands;
use mint_cli::data::create_data_source;

#[path = "common/mod.rs"]
mod common;

#[test]
fn test_build_without_excel() {
    common::ensure_out_dir();

    let layout_path = "tests/data/blocks.toml";

    // Build simple_block which has all inline values (no Excel dependency)
    let args = mint_cli::args::Args {
        layout: mint_cli::layout::args::LayoutArgs {
            blocks: vec![mint_cli::layout::args::BlockNames {
                name: "simple_block".to_string(),
                file: layout_path.to_string(),
            }],
            strict: false,
        },
        data: Default::default(),
        output: mint_cli::output::args::OutputArgs {
            out: PathBuf::from("out/simple_block.hex"),
            record_width: 32,
            format: mint_cli::output::args::OutputFormat::Hex,
            export_json: None,
            stats: false,
            quiet: true,
        },
    };

    // This should succeed since all values are inline
    let stats = commands::build(&args, None).expect("build should succeed without Excel file");

    assert!(
        stats.blocks_processed > 0,
        "Should build at least one block"
    );

    common::assert_out_file_exists(std::path::Path::new("out/simple_block.hex"));
}

#[test]
fn test_error_when_name_without_excel() {
    common::ensure_out_dir();

    // Use a block that references names from Excel
    let layout_path = "tests/data/blocks.toml";

    let input = mint_cli::layout::args::BlockNames {
        name: "block".to_string(),
        file: layout_path.to_string(),
    };

    let args = mint_cli::args::Args {
        layout: mint_cli::layout::args::LayoutArgs {
            blocks: vec![input.clone()],
            strict: false,
        },
        data: Default::default(),
        output: mint_cli::output::args::OutputArgs {
            out: PathBuf::from("out/error_test.hex"),
            record_width: 32,
            format: mint_cli::output::args::OutputFormat::Hex,
            export_json: None,
            stats: false,
            quiet: true,
        },
    };

    // This should fail with MissingDataSheet error
    let result = commands::build(&args, None);
    assert!(
        result.is_err(),
        "Expected error when using 'name' without Excel file"
    );

    let err = result.unwrap_err();
    let err_str = format!("{}", err);
    assert!(
        err_str.contains("Missing datasheet")
            || err_str.contains("requires a value from a data source"),
        "Error should mention missing data source, got: {}",
        err_str
    );
}

#[test]
fn test_factory_returns_none_without_datasource() {
    // Test that create_data_source returns None when no datasource is provided
    let args_no_datasource = mint_cli::data::args::DataArgs::default();

    let result = create_data_source(&args_no_datasource).expect("should return Ok(None)");
    assert!(
        result.is_none(),
        "create_data_source should return None when no datasource provided"
    );

    // Test with versions flag but no datasource
    let args_version_no_datasource = mint_cli::data::args::DataArgs {
        versions: Some("Default".to_string()),
        ..Default::default()
    };

    let result = create_data_source(&args_version_no_datasource).expect("should return Ok(None)");
    assert!(
        result.is_none(),
        "create_data_source should return None when no datasource provided, even with versions flag"
    );
}
