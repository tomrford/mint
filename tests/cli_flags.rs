use clap::{Parser, error::ErrorKind};
use mint_cli::args::Args;
use mint_cli::data::create_data_source;

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

#[test]
fn deprecated_postgres_flag_fails_with_migration_hint() {
    let args = Args::try_parse_from([
        "mint",
        "layout.toml",
        "--postgres",
        "{}",
        "--versions",
        "Default",
    ])
    .expect("args should still parse for compatibility");

    let err = match create_data_source(&args.data) {
        Ok(_) => panic!("deprecated postgres flag should fail"),
        Err(err) => err,
    };
    assert!(
        err.to_string()
            .contains("fetch first and pass JSON via --json")
    );
}

#[test]
fn deprecated_http_flag_fails_with_migration_hint() {
    let args = Args::try_parse_from([
        "mint",
        "layout.toml",
        "--http",
        "{}",
        "--versions",
        "Default",
    ])
    .expect("args should still parse for compatibility");

    let err = match create_data_source(&args.data) {
        Ok(_) => panic!("deprecated http flag should fail"),
        Err(err) => err,
    };
    assert!(
        err.to_string()
            .contains("fetch first and pass JSON via --json")
    );
}
