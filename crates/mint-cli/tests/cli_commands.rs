use std::process::Command;

#[path = "common/mod.rs"]
mod common;

const SKILL_TEXT: &str = include_str!("../skill/mint/SKILL.md");

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
fn abi_list_prints_supported_profiles() {
    let output = mint_command()
        .args(["abi", "list"])
        .output()
        .expect("mint abi list should run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    for name in [
        "generic-le",
        "generic-be",
        "arm-aapcs32-le",
        "tricore-eabi-le",
        "riscv-ilp32-le",
        "ti-c28x-eabi",
    ] {
        assert!(stdout.contains(name), "missing {name}: {stdout}");
    }
    assert!(output.stderr.is_empty());
}

#[test]
fn abi_show_reports_profile_layout_rules() {
    let show = |profile| {
        let output = mint_command()
            .args(["abi", "show", profile])
            .output()
            .expect("mint abi show should run");
        assert!(
            output.status.success(),
            "{profile} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty(), "{profile} wrote to stderr");
        String::from_utf8(output.stdout).expect("stdout is utf8")
    };

    let generic = show("generic-le");
    for expected in [
        "name: generic-le",
        "family: generic-natural",
        "byte order: little",
        "target addressable unit: 8 bits",
        "output addresses: octet addresses",
        "aggregate rules:",
        "type  storage  alignment  stride  C type",
        "u64",
        "all sizes, alignments and strides are in octets",
    ] {
        assert!(generic.contains(expected), "missing {expected}: {generic}");
    }

    let tricore = show("tricore-eabi-le");
    assert_eq!(
        tricore
            .lines()
            .find(|line| line.starts_with("u64"))
            .expect("TriCore u64 row")
            .split_whitespace()
            .collect::<Vec<_>>(),
        ["u64", "8", "4", "8", "uint64_t"]
    );

    let c28x = show("ti-c28x-eabi");
    assert!(c28x.contains("target addressable unit: 16 bits"));
    assert!(c28x.contains("2 × target word address"));
    assert!(
        c28x.lines()
            .any(|line| line.starts_with("u8") && line.contains("unsupported"))
    );
    assert_eq!(
        c28x.lines()
            .find(|line| line.starts_with("u64"))
            .expect("C28x u64 row")
            .split_whitespace()
            .collect::<Vec<_>>(),
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
    let one_stdout = String::from_utf8(one.stdout).expect("stdout is utf8");
    let fingerprint = one_stdout.trim();
    assert_eq!(fingerprint.len(), 16);
    assert!(
        fingerprint
            .chars()
            .all(|character| character.is_ascii_hexdigit())
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
    let stdout = String::from_utf8(all.stdout).expect("stdout is utf8");
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2, "{stdout}");
    for (line, block) in lines.into_iter().zip(["config", "data"]) {
        let (name, fingerprint) = line.split_once(' ').expect("named fingerprint line");
        assert_eq!(name, block);
        assert_eq!(fingerprint.len(), 16);
        assert!(
            fingerprint
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        );
    }
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
    assert!(out.exists(), "expected output file: {}", out.display());
}

#[test]
fn render_failure_does_not_write_output_or_used_values() {
    let layout = common::write_layout_file(
        "invalid-record-width",
        r#"
[mint]
abi = "ti-c28x-eabi"

[block.header]
start_address = 0
length = 2

[block.data]
value = { value = 1, type = "u16" }
"#,
    );
    let out = common::unique_out_path("invalid-record-width", "hex");
    let report = common::unique_out_path("invalid-record-width", "json");

    let output = mint_command()
        .args(["build", &format!("{layout}#block"), "--record-width", "3"])
        .arg("--export-json")
        .arg(&report)
        .arg("--out")
        .arg(&out)
        .output()
        .expect("mint build should run");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains(
        "record width 3 octets is not divisible by the target's 2-octet addressable unit"
    ));
    assert!(!out.exists(), "render failure wrote output");
    assert!(!report.exists(), "render failure wrote used values");
}

#[test]
fn equivalent_output_and_report_paths_are_rejected_without_writing() {
    let raw_path = common::unique_out_path("output-report-collision", "hex");
    #[cfg(windows)]
    let path = raw_path;
    #[cfg(not(windows))]
    let path = raw_path
        .parent()
        .expect("output parent")
        .canonicalize()
        .expect("canonical output parent")
        .join(raw_path.file_name().expect("output file name"));
    let relative = path.file_name().expect("output file name");
    let block = format!(
        "{}/../mint-core/tests/data/blocks.toml#simple_block",
        env!("CARGO_MANIFEST_DIR")
    );
    for existing in [false, true] {
        if existing {
            std::fs::write(&path, "keep me").expect("write sentinel");
        }
        let output = mint_command()
            .current_dir(path.parent().expect("output parent"))
            .args(["build", &block])
            .arg("--out")
            .arg(relative)
            .arg("--export-json")
            .arg(&path)
            .output()
            .expect("mint build should run");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("overlaps input or output"));
        let contents = std::fs::read_to_string(&path).ok();
        assert_eq!(contents.as_deref(), existing.then_some("keep me"));
    }
}

#[test]
fn format_extension_mismatch_warning_respects_quiet() {
    let warning = "warning: output extension '.hex' does not match Motorola S-Record format";
    for (stem, quiet) in [("format-mismatch", false), ("quiet-format-mismatch", true)] {
        let out = common::unique_out_path(stem, "hex");
        let mut command = mint_command();
        command
            .args([
                "build",
                "../mint-core/tests/data/blocks.toml#simple_block",
                "--format",
                "mot",
                "--out",
            ])
            .arg(&out);
        if quiet {
            command.arg("--quiet");
        }
        let output = command.output().expect("mint build should run");

        assert!(
            output.status.success(),
            "quiet={quiet} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
        if quiet {
            assert!(stderr.is_empty(), "stderr: {stderr}");
        } else {
            assert!(stderr.contains(warning), "stderr: {stderr}");
        }
        assert!(out.exists(), "expected output file: {}", out.display());
    }
}

#[test]
fn outputs_cannot_replace_layout_or_data_inputs() {
    let contents = "[mint]\nabi = \"generic-le\"\n[block.header]\nstart_address = 0\nlength = 4\n[block.data]\nx = {value = 1, type = \"u8\"}\n";
    let layout = common::write_layout_file("protected-input", contents);
    for command in ["build", "header"] {
        let output = mint_command()
            .args([command, &layout, "--out", &layout])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("overlaps input or output"));
        assert_eq!(std::fs::read_to_string(&layout).unwrap(), contents);
    }
    for format in ["json", "xlsx"] {
        let data = common::unique_out_path("protected-data", format);
        if format == "json" {
            std::fs::write(&data, r#"{"Default":{"Value":1}}"#).unwrap();
        } else {
            std::fs::copy("../mint-core/tests/data/data.xlsx", &data).unwrap();
        }
        let original = std::fs::read(&data).unwrap();
        let out = common::unique_out_path("protected-output", "hex");
        let output = mint_command()
            .args(["build", &layout])
            .arg(format!("--{format}"))
            .arg(&data)
            .args(["--variants", "Default"])
            .arg("--export-json")
            .arg(&data)
            .arg("--out")
            .arg(&out)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("overlaps input or output"));
        assert_eq!(std::fs::read(&data).unwrap(), original);
        assert!(!out.exists());
    }
}

#[test]
fn output_aliases_are_rejected_before_either_file_is_written() {
    let root = common::unique_out_path("output-aliases", "dir");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    let path = root.join("out.hex");
    let mut aliases = vec![root.join("sub/../out.hex")];
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&root, root.join("alias")).unwrap();
        aliases.push(root.join("alias/out.hex"));
    }
    let block = "../mint-core/tests/data/blocks.toml#simple_block";
    for alias in aliases {
        let output = mint_command()
            .args(["build", block])
            .arg("--out")
            .arg(&path)
            .arg("--export-json")
            .arg(&alias)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("overlaps input or output"));
        assert!(!path.exists());
    }
    #[cfg(unix)]
    {
        let dangling = root.join("dangling.hex");
        std::os::unix::fs::symlink(&path, &dangling).unwrap();
        let output = mint_command()
            .args(["build", block])
            .arg("--out")
            .arg(&dangling)
            .arg("--export-json")
            .arg(&path)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!path.exists());
    }
    std::fs::write(&path, "keep me").unwrap();
    let alias = root.join("hardlink.hex");
    std::fs::hard_link(&path, &alias).unwrap();
    let output = mint_command()
        .args(["build", block])
        .arg("--out")
        .arg(&path)
        .arg("--export-json")
        .arg(&alias)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("overlaps input or output"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep me");
}
