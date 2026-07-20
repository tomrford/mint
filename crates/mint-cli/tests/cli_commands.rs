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
    assert!(stdout.contains("header"));
    assert!(stdout.contains("fingerprint"));
    assert!(stdout.contains("abi"));
    assert!(stdout.contains("skill"));
}

#[test]
fn abi_list_prints_supported_profiles() {
    let output = mint_command()
        .args(["abi", "list"])
        .output()
        .expect("mint abi list should run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("generic-le"));
    assert!(stdout.contains("generic-be"));
    assert!(stdout.contains("little-endian"));
    assert!(stdout.contains("big-endian"));
    assert!(output.stderr.is_empty());
}

#[test]
fn abi_show_describes_layout_rules_without_selecting_an_output_format() {
    let output = mint_command()
        .args(["abi", "show", "generic-le"])
        .output()
        .expect("mint abi show should run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("name: generic-le"));
    assert!(stdout.contains("family: generic-natural"));
    assert!(stdout.contains("byte order: little"));
    assert!(stdout.contains("addressable unit: 8 bits"));
    assert!(stdout.contains("output formats: hex, mot (selected independently)"));
    assert!(output.stderr.is_empty());
}

#[test]
fn abi_show_rejects_unknown_profiles_with_supported_names() {
    let output = mint_command()
        .args(["abi", "show", "unknown"])
        .output()
        .expect("mint abi show should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    assert!(stderr.contains("unknown ABI 'unknown'"));
    assert!(stderr.contains("generic-le, generic-be"));
}

#[test]
fn fingerprint_prints_only_hex_for_one_block_and_named_lines_for_a_file() {
    let layout = common::write_layout_file(
        "fingerprint-output",
        r#"
[mint]
abi = "generic-le"

[config.header]
start_address = 0x1000
length = 0x20

[config.data]
value = { value = 1, type = "u32" }

[data.header]
start_address = 0x2000
length = 0x20

[data.data]
value = { value = [1, 2], type = "u16", size = 2 }
"#,
    );
    let selector = mint_core::build::BlockSelector::all(&layout);
    let fingerprints = mint_core::fingerprint::load(&selector).expect("fingerprints load");

    let one = mint_command()
        .arg("fingerprint")
        .arg(format!("{layout}#config"))
        .output()
        .expect("fingerprint command runs");
    assert!(
        one.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&one.stderr)
    );
    assert_eq!(
        String::from_utf8(one.stdout).expect("stdout is utf8"),
        format!("{}\n", fingerprints[0].hex())
    );
    assert!(one.stderr.is_empty());

    let all = mint_command()
        .arg("fingerprint")
        .arg(&layout)
        .output()
        .expect("fingerprint command runs");
    assert!(
        all.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&all.stderr)
    );
    let expected = fingerprints
        .iter()
        .map(|fingerprint| format!("{} {}\n", fingerprint.block, fingerprint.hex()))
        .collect::<String>();
    assert_eq!(
        String::from_utf8(all.stdout).expect("stdout is utf8"),
        expected
    );
    assert!(all.stderr.is_empty());
}

#[test]
fn fingerprint_validation_is_scoped_to_the_selected_block() {
    let layout = common::write_layout_file(
        "fingerprint-scope",
        r#"
[mint]
abi = "generic-le"

[good.header]
start_address = 0x1000
length = 0x20

[good.data]
value = { value = 1, type = "u32" }

[bad_ref.header]
start_address = 0x2000
length = 0x20

[bad_ref.data]
pointer = { ref = "missing", type = "u32" }

[bad_const.header]
start_address = 0x3000
length = 0x20

[bad_const.data]
value = { const = "missing", type = "u32" }
"#,
    );

    let selected = mint_command()
        .arg("fingerprint")
        .arg(format!("{layout}#good"))
        .output()
        .expect("selected fingerprint command runs");
    assert!(
        selected.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&selected.stderr)
    );
    assert_eq!(
        String::from_utf8(selected.stdout)
            .expect("stdout is utf8")
            .trim()
            .len(),
        16
    );

    let whole_file = mint_command()
        .arg("fingerprint")
        .arg(&layout)
        .output()
        .expect("whole-file fingerprint command runs");
    assert!(!whole_file.status.success());

    let dangling_const = mint_command()
        .arg("fingerprint")
        .arg(format!("{layout}#bad_const"))
        .output()
        .expect("dangling-const fingerprint command runs");
    assert!(!dangling_const.status.success());
    assert!(
        String::from_utf8_lossy(&dangling_const.stderr).contains("Const 'missing' not found"),
        "stderr: {}",
        String::from_utf8_lossy(&dangling_const.stderr)
    );
}

#[test]
fn missing_command_reports_top_level_usage() {
    let output = mint_command().output().expect("mint should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");

    assert!(stderr.contains("Usage: mint <COMMAND>"));
    assert!(stderr.contains("Run `mint <COMMAND> --help` for command options."));
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
        .arg("--variants")
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

#[test]
fn format_extension_mismatch_warns_without_renaming() {
    let out = common::unique_out_path("format-mismatch", "hex");
    let output = mint_command()
        .args([
            "build",
            "../mint-core/tests/data/blocks.toml#simple_block",
            "--format",
            "mot",
            "--out",
        ])
        .arg(&out)
        .output()
        .expect("mint build should run");

    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .expect("stderr is utf8")
            .contains("warning: output extension '.hex' does not match Motorola S-Record format")
    );
    common::assert_out_file_exists(&out);
}

#[test]
fn quiet_suppresses_format_extension_warning() {
    let out = common::unique_out_path("quiet-format-mismatch", "hex");
    let output = mint_command()
        .args([
            "build",
            "../mint-core/tests/data/blocks.toml#simple_block",
            "--format",
            "mot",
            "--out",
        ])
        .arg(&out)
        .arg("--quiet")
        .output()
        .expect("mint build should run");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
}
