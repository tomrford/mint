use clap::{Parser, error::ErrorKind};
use mint_cli::args::{Args, Cli, Command};
use mint_cli::data::create_data_source;
use mint_core::layout::value::DataValue;

use std::fs;
use std::path::Path;

mod common;

fn parse_build_args<const N: usize>(argv: [&str; N]) -> Result<Args, clap::Error> {
    let cli = Cli::try_parse_from(argv)?;
    let Command::Build(args) = cli.command else {
        panic!("expected build command");
    };
    Ok(args)
}

#[test]
fn parses_file_hash_block_selector() {
    let args = parse_build_args(["mint", "build", "layout.toml#config"])
        .expect("args should parse with file#block syntax");

    assert_eq!(args.layout.blocks.len(), 1);
    assert_eq!(args.layout.blocks[0].layout, Path::new("layout.toml"));
    assert_eq!(args.layout.blocks[0].block.as_deref(), Some("config"));
}

#[test]
fn rejects_empty_hash_selector() {
    let err = parse_build_args(["mint", "build", "layout.toml#"])
        .expect_err("empty block selector should fail");
    assert_eq!(err.kind(), ErrorKind::ValueValidation);
}

#[test]
fn parses_short_xlsx_flag() {
    let args = parse_build_args([
        "mint",
        "build",
        "layout.toml",
        "-x",
        "tests/data/data.xlsx",
        "--variants",
        "Debug/Default",
    ])
    .expect("args should parse with -x");

    assert_eq!(args.data.xlsx.as_deref(), Some("tests/data/data.xlsx"));
}

#[test]
fn parses_short_json_flag() {
    let args = parse_build_args([
        "mint",
        "build",
        "layout.toml",
        "-j",
        "tests/data.json",
        "--variants",
        "Debug/Default",
    ])
    .expect("args should parse with -j");

    assert_eq!(args.data.json.as_deref(), Some("tests/data.json"));
}

#[test]
fn rejects_main_sheet_without_xlsx() {
    let err = parse_build_args([
        "mint",
        "build",
        "layout.toml",
        "--json",
        "{}",
        "--main-sheet",
        "Config",
        "--variants",
        "Default",
    ])
    .expect_err("--main-sheet should require --xlsx");

    assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
}

#[test]
fn rejects_main_sheet_without_data_source() {
    let err = parse_build_args(["mint", "build", "layout.toml", "--main-sheet", "Config"])
        .expect_err("--main-sheet should require a data source");

    assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
}

#[test]
fn json_path_without_json_suffix_is_read_from_file() {
    let json_path = common::unique_out_path("data-without-suffix", "txt");
    fs::write(&json_path, r#"{"Default":{"Counter":7}}"#).expect("write json file");

    let args = mint_cli::data_args::DataArgs {
        json: Some(json_path.to_string_lossy().into_owned()),
        variants: vec!["Default".to_owned()],
        ..Default::default()
    };
    let ds = create_data_source(&args)
        .expect("data source should load")
        .expect("json data source");

    assert!(matches!(
        ds.retrieve_single_value("Counter")
            .expect("counter should be read"),
        DataValue::U64(7)
    ));
}

#[test]
fn inline_json_starting_with_brace_still_works() {
    let args = mint_cli::data_args::DataArgs {
        json: Some(r#"{"Default":{"Counter":9}}"#.to_owned()),
        variants: vec!["Default".to_owned()],
        ..Default::default()
    };
    let ds = create_data_source(&args)
        .expect("data source should load")
        .expect("json data source");

    assert!(matches!(
        ds.retrieve_single_value("Counter")
            .expect("counter should be read"),
        DataValue::U64(9)
    ));
}

#[test]
fn parses_versions_selector_flag() {
    let args = parse_build_args([
        "mint",
        "build",
        "layout.toml",
        "--xlsx",
        "tests/data/data.xlsx",
        "--variants",
        "Debug/Default",
    ])
    .expect("args should parse with --versions");

    assert_eq!(args.data.variants, vec!["Debug", "Default"]);
}

#[test]
fn retains_builtin_version_flag() {
    let err = Cli::try_parse_from(["mint", "--version"]).expect_err("should emit version output");
    assert_eq!(err.kind(), ErrorKind::DisplayVersion);
}

#[test]
fn preserves_explicit_build_invocation() {
    let cli = Cli::try_parse_from([
        "mint",
        "build",
        "layout.toml",
        "--json",
        "{}",
        "--variants",
        "Default",
    ])
    .expect("explicit build should parse");

    let Command::Build(args) = cli.command else {
        panic!("expected build command");
    };

    assert_eq!(args.layout.blocks.len(), 1);
    assert_eq!(args.data.json.as_deref(), Some("{}"));
}

#[test]
fn rejects_build_invocation_without_subcommand() {
    let err = Cli::try_parse_from([
        "mint",
        "layout.toml",
        "--json",
        "{}",
        "--variants",
        "Default",
    ])
    .expect_err("build requires the build subcommand");

    assert_eq!(err.kind(), ErrorKind::InvalidSubcommand);
}

#[test]
fn parses_skill_command() {
    let cli = Cli::try_parse_from(["mint", "skill"]).expect("skill should parse");

    assert!(matches!(cli.command, Command::Skill));
}

#[test]
fn parses_header_command_without_build_options() {
    let cli = Cli::try_parse_from([
        "mint",
        "header",
        "layout.toml#config",
        "layout.toml#data",
        "-o",
        "layout.h",
    ])
    .expect("header command should parse");

    let Command::Header(args) = cli.command else {
        panic!("expected header command");
    };
    assert_eq!(args.blocks.len(), 2);
    assert_eq!(args.blocks[0].block.as_deref(), Some("config"));
    assert_eq!(args.blocks[1].block.as_deref(), Some("data"));
    assert_eq!(args.out, Path::new("layout.h"));
}
