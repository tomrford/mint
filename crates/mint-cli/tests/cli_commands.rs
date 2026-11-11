#[path = "common/mod.rs"]
mod common;

use common::mint_command;

/// An agent with only the binary starts at `mint skill` and must be able to
/// reach every reference topic from it.
#[test]
fn skill_guide_names_every_topic_and_each_topic_prints_its_own_page() {
    let unknown = mint_command()
        .args(["skill", "no-such-topic"])
        .output()
        .expect("mint skill should run");
    assert!(!unknown.status.success());
    assert!(unknown.stdout.is_empty());
    let stderr = String::from_utf8(unknown.stderr).expect("stderr is utf8");
    let topics = stderr
        .split_once("[possible values: ")
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(list, _)| list.split(", ").collect::<Vec<_>>())
        .unwrap_or_else(|| panic!("unknown topic error lists no valid topics: {stderr}"));

    let print = |args: &[&str]| {
        let output = mint_command()
            .args(args)
            .output()
            .expect("mint skill should run");
        assert!(
            output.status.success(),
            "{args:?} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty(), "{args:?} wrote to stderr");
        String::from_utf8(output.stdout).expect("stdout is utf8")
    };

    let guide = print(&["skill"]);
    let named = guide
        .split("mint skill ")
        .skip(1)
        .filter_map(|rest| {
            let topic = rest
                .split(|character: char| !character.is_ascii_alphanumeric() && character != '-')
                .next()?;
            (!topic.is_empty()).then_some(topic)
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        named,
        topics.iter().copied().collect(),
        "the guide must name exactly the topics `mint skill` accepts"
    );

    let mut pages = vec![guide];
    for topic in topics {
        let page = print(&["skill", topic]);
        assert!(!page.trim().is_empty(), "topic {topic} printed nothing");
        assert!(
            !pages.contains(&page),
            "topic {topic} repeats the guide or another topic"
        );
        pages.push(page);
    }
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
    let is_lowercase_hex = |value: &str| {
        value.len() == 16
            && value
                .chars()
                .all(|character| matches!(character, '0'..='9' | 'a'..='f'))
    };
    let one_stdout = String::from_utf8(one.stdout).expect("stdout is utf8");
    let config_fingerprint = one_stdout
        .strip_suffix('\n')
        .expect("one newline-terminated value");
    assert!(is_lowercase_hex(config_fingerprint), "{one_stdout:?}");
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
    let lines = stdout
        .lines()
        .map(|line| line.split_once(' ').expect("named fingerprint line"))
        .collect::<Vec<_>>();
    assert_eq!(
        lines.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        ["config", "data"],
        "{stdout}"
    );
    assert!(
        lines.iter().all(|(_, value)| is_lowercase_hex(value)),
        "{stdout}"
    );
    assert_eq!(lines[0].1, config_fingerprint, "{stdout}");
    assert_ne!(lines[0].1, lines[1].1, "{stdout}");
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
    assert!(
        String::from_utf8_lossy(&whole_file.stderr).contains("ref target 'missing' not found"),
        "stderr: {}",
        String::from_utf8_lossy(&whole_file.stderr)
    );

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
    assert!(stderr.contains("Run `mint <COMMAND> --help` for command options"));
    assert!(stderr.contains("`mint skill`"), "{stderr}");
}

#[test]
fn explicit_build_invocation_writes_excel_values() {
    let out = common::unique_out_path("build", "hex");

    for source_args in [vec![], vec!["--json", "{}", "--variants", "Default"]] {
        let rejected = mint_command()
            .args(["build", "../mint-core/tests/data/blocks.toml#block"])
            .args(source_args)
            .args(["--main-sheet", "Main", "--out"])
            .arg(&out)
            .output()
            .expect("mint build should run");
        assert!(!rejected.status.success());
        assert!(String::from_utf8_lossy(&rejected.stderr).contains("--main-sheet"));
        assert!(!out.exists());
    }

    let output = mint_command()
        .arg("build")
        .arg("../mint-core/tests/data/blocks.toml#block")
        .arg("--xlsx")
        .arg("../mint-core/tests/data/data.xlsx")
        .args(["--main-sheet", "Main"])
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
    let artifact = std::fs::read_to_string(&out).expect("output artifact is readable");
    let first_data_record = artifact
        .lines()
        .find(|record| record.get(7..9) == Some("00"))
        .expect("Intel HEX data record");
    assert_eq!(first_data_record.get(9..25), Some("3412000001000000"));
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

/// The selected blocks are checked before the data source is read, so a
/// layout mistake is reported even when the data source is also unusable.
#[test]
fn build_rejects_invalid_selected_blocks_before_reading_the_data_source() {
    let layout = common::write_layout_file(
        "preflight-before-data",
        r#"
[mint]
abi = "generic-le"

[block.header]
start_address = 0
length = 4

[block.data]
value = { name = "Value", type = "u64" }
"#,
    );
    let out = common::unique_out_path("preflight-before-data", "hex");
    let missing_data = common::unique_out_path("preflight-before-data", "json");

    let output = mint_command()
        .args(["build", &format!("{layout}#block"), "--json"])
        .arg(&missing_data)
        .args(["--variants", "Default", "--out"])
        .arg(&out)
        .output()
        .expect("mint build should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("while building block 'block'")
            && stderr.contains(
                "resolved layout size (8 octets) exceeds configured block length (4 octets)"
            ),
        "stderr: {stderr}"
    );
    assert!(!out.exists(), "preflight failure wrote output");
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
