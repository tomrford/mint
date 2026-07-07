use clap::{Parser, error::ErrorKind};
use mint_cli::args::{Args, Cli, Command};

use std::path::Path;

#[test]
fn parses_file_hash_block_selector() {
    let args = Args::try_parse_from(["mint", "layout.toml#config"])
        .expect("args should parse with file#block syntax");

    assert_eq!(args.layout.blocks.len(), 1);
    assert_eq!(args.layout.blocks[0].layout, Path::new("layout.toml"));
    assert_eq!(args.layout.blocks[0].block.as_deref(), Some("config"));
}

#[test]
fn rejects_empty_hash_selector() {
    let err = Args::try_parse_from(["mint", "layout.toml#"])
        .expect_err("empty block selector should fail");
    assert_eq!(err.kind(), ErrorKind::ValueValidation);
}

#[test]
fn parses_short_xlsx_flag() {
    let args = Args::try_parse_from([
        "mint",
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
    let args = Args::try_parse_from([
        "mint",
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
fn parses_versions_selector_flag() {
    let args = Args::try_parse_from([
        "mint",
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
