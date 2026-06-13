use std::process::Command;

use mint_cli::args::SKILL_TEXT;

#[path = "common/mod.rs"]
mod common;

fn mint_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mint"));
    command.current_dir(env!("CARGO_MANIFEST_DIR"));
    command
}

#[test]
fn skill_prints_bundled_skill_text() {
    let output = mint_command()
        .arg("skill")
        .output()
        .expect("mint skill should run");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is utf8"),
        SKILL_TEXT
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn top_level_help_lists_commands() {
    let output = mint_command()
        .arg("--help")
        .output()
        .expect("mint --help should run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");

    assert!(stdout.contains("Commands:"));
    assert!(stdout.contains("build"));
    assert!(stdout.contains("skill"));
}

#[test]
fn missing_command_reports_top_level_usage() {
    let output = mint_command().output().expect("mint should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");

    assert!(stderr.contains("Usage: mint <COMMAND>"));
    assert!(stderr.contains("Run `mint build --help` for build options."));
}

#[test]
fn explicit_build_invocation_writes_output() {
    common::ensure_out_dir();

    let out = common::unique_out_path("build", "hex");

    let output = mint_command()
        .arg("build")
        .arg("../mint-core/tests/data/blocks.toml#block")
        .arg("--xlsx")
        .arg("../mint-core/tests/data/data.xlsx")
        .arg("--versions")
        .arg("Default")
        .arg("--out")
        .arg(&out)
        .arg("--quiet")
        .output()
        .expect("mint build should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    common::assert_out_file_exists(&out);
}
