#[path = "common/mod.rs"]
mod common;

#[test]
fn c28x_detailed_stats_use_target_address_units() {
    let layout = common::write_layout_file(
        "c28x_stats",
        r#"
[mint]
abi = "ti-c28x-eabi"

[block.header]
start_address = 0x1000
length = 0x100

[block.data]
value = { value = 1, type = "u16" }
"#,
    );
    let output_path = common::unique_out_path("c28x_stats", "hex");
    let output = common::mint_command()
        .args(["build", &layout, "--stats", "-o"])
        .arg(output_path)
        .output()
        .expect("mint build should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(stdout.contains("Address Range (target units)"), "{stdout}");
    assert!(stdout.contains("Reserved/Allocated (bytes)"), "{stdout}");
    assert!(stdout.contains("0x1000-0x107F"), "{stdout}");
    assert!(!stdout.contains("0x1000-0x10FF"), "{stdout}");
}
