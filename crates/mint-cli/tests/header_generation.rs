use std::fs;

#[path = "common/mod.rs"]
mod common;

use common::mint_command;

#[test]
fn generated_example_header_is_checked_in() {
    let header_path = common::unique_out_path("generated-blocks", "h");
    let output = mint_command()
        .args(["header", "../../doc/examples/block.toml", "-o"])
        .arg(&header_path)
        .output()
        .expect("mint header should run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let generated = fs::read_to_string(&header_path).expect("generated header is readable");
    let checked_in = fs::read_to_string("../../doc/examples/blocks.h")
        .expect("checked-in generated header is readable")
        .replace("\r\n", "\n");
    assert_eq!(generated, checked_in);
}

#[test]
fn example_layout_builds_strictly_from_the_example_workbook() {
    for variants in ["Default", "Debug/Default", "VarA/Default"] {
        let output = mint_command()
            .args(["build", "../../doc/examples/block.toml", "--xlsx"])
            .args(["../../doc/examples/data.xlsx", "--variants", variants])
            .args(["--strict", "--quiet", "-o"])
            .arg(common::unique_out_path("strict-example", "hex"))
            .output()
            .expect("mint build should run");
        assert!(
            output.status.success(),
            "variants {variants}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn empty_layout_does_not_touch_build_or_header_output() {
    let layout = common::write_layout_file(
        "empty-layout",
        r#"
[mint]
abi = "generic-le"
"#,
    );
    let output_path = common::unique_out_path("preserved-header", "h");
    fs::write(&output_path, "preserve me\n").expect("sentinel header writes");

    for command in ["build", "header"] {
        let output = mint_command()
            .args([command, &layout, "-o"])
            .arg(&output_path)
            .output()
            .expect("mint command should run");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("no blocks provided"));
        assert_eq!(
            fs::read_to_string(&output_path).expect("sentinel header remains readable"),
            "preserve me\n"
        );
    }
}
