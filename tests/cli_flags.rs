use clap::{Parser, error::ErrorKind};
use mint_cli::args::Args;

#[test]
fn parses_file_hash_block_selector() {
    let args = Args::try_parse_from(["mint", "layout.toml#config"])
        .expect("args should parse with file#block syntax");

    assert_eq!(args.layout.blocks.len(), 1);
    assert_eq!(args.layout.blocks[0].file, "layout.toml");
    assert_eq!(args.layout.blocks[0].name, "config");
    assert!(!args.layout.blocks[0].legacy_syntax);
}

#[test]
fn parses_legacy_block_at_file_selector() {
    let args = Args::try_parse_from(["mint", "config@layout.toml"])
        .expect("args should parse with legacy block@file syntax");

    assert_eq!(args.layout.blocks.len(), 1);
    assert_eq!(args.layout.blocks[0].file, "layout.toml");
    assert_eq!(args.layout.blocks[0].name, "config");
    assert!(args.layout.blocks[0].legacy_syntax);
}

#[test]
fn rejects_empty_hash_selector() {
    let err = Args::try_parse_from(["mint", "layout.toml#"])
        .expect_err("empty block selector should fail");
    assert_eq!(err.kind(), ErrorKind::ValueValidation);
}

#[test]
fn rejects_empty_legacy_selector() {
    let err = Args::try_parse_from(["mint", "@layout.toml"])
        .expect_err("empty legacy selector should fail");
    assert_eq!(err.kind(), ErrorKind::ValueValidation);
}

#[test]
fn parses_versions_selector_flag() {
    let args = Args::try_parse_from([
        "mint",
        "layout.toml",
        "--xlsx",
        "tests/data/data.xlsx",
        "--versions",
        "Debug/Default",
    ])
    .expect("args should parse with --versions");

    assert_eq!(args.data.versions.as_deref(), Some("Debug/Default"));
    assert_eq!(args.data.get_version_list(), vec!["Debug", "Default"]);
}

#[test]
fn retains_builtin_version_flag() {
    let err = Args::try_parse_from(["mint", "--version"]).expect_err("should emit version output");
    assert_eq!(err.kind(), ErrorKind::DisplayVersion);
}
