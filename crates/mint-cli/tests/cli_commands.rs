use std::process::Command;

use mint_cli::args::SKILL_TEXT;
use mint_core::layout::abi::Abi;

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
    for abi in Abi::ALL {
        assert!(
            stdout.contains(abi.name()),
            "missing {}: {stdout}",
            abi.name()
        );
        assert!(
            stdout.contains(abi.description()),
            "missing description for {}: {stdout}",
            abi.name()
        );
    }
    assert!(output.stderr.is_empty());
}

#[test]
fn abi_show_describes_layout_rules() {
    let output = mint_command()
        .args(["abi", "show", "generic-le"])
        .output()
        .expect("mint abi show should run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("name: generic-le"));
    assert!(stdout.contains("family: generic-natural"));
    assert!(stdout.contains("byte order: little"));
    assert!(stdout.contains("target addressable unit: 8 bits"));
    assert!(stdout.contains("output addresses: octet addresses"));
    assert!(stdout.contains("aggregate rules:"));
    assert!(stdout.contains("type  storage  alignment  stride  C type"));
    assert!(stdout.contains("u64"));
    assert!(stdout.contains("all sizes, alignments and strides are in octets"));
    assert!(output.stderr.is_empty());
}

#[test]
fn abi_show_reports_tricore_64_bit_alignment() {
    let output = mint_command()
        .args(["abi", "show", "tricore-eabi-le"])
        .output()
        .expect("mint abi show should run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    let u64_row = stdout
        .lines()
        .find(|line| line.starts_with("u64"))
        .expect("u64 row");
    assert_eq!(
        u64_row.split_whitespace().collect::<Vec<_>>(),
        ["u64", "8", "4", "8", "uint64_t"]
    );
}

#[test]
fn abi_show_reports_c28x_support_and_output_addressing() {
    let output = mint_command()
        .args(["abi", "show", "ti-c28x-eabi"])
        .output()
        .expect("mint abi show should run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("target addressable unit: 16 bits"));
    assert!(stdout.contains("2 × target word address"));
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("u8") && line.contains("unsupported"))
    );
    let u64_row = stdout
        .lines()
        .find(|line| line.starts_with("u64"))
        .expect("u64 row");
    assert_eq!(
        u64_row.split_whitespace().collect::<Vec<_>>(),
        ["u64", "8", "4", "8", "uint64_t"]
    );
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
