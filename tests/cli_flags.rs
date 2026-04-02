use clap::{Parser, error::ErrorKind};
use mint_cli::args::Args;

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
