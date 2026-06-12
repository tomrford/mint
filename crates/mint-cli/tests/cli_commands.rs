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
fn top_level_help_lists_commands_and_legacy_usage() {
    let output = mint_command()
        .arg("--help")
        .output()
        .expect("mint --help should run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");

    assert!(stdout.contains("Commands:"));
    assert!(stdout.contains("build"));
    assert!(stdout.contains("skill"));
    assert!(stdout.contains("without the `build` command"));
}

#[test]
fn missing_build_layout_reports_build_usage() {
    let output = mint_command().output().expect("mint should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");

    assert!(stderr.contains("required arguments were not provided"));
    assert!(stderr.contains("mint build"));
}

#[test]
fn explicit_and_legacy_build_invocations_write_outputs() {
    common::ensure_out_dir();

    for command_name in [Some("build"), None] {
        let out = common::unique_out_path(command_name.unwrap_or("legacy"), "hex");

        let mut command = mint_command();
        if let Some(command_name) = command_name {
            command.arg(command_name);
        }

        let output = command
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
}
