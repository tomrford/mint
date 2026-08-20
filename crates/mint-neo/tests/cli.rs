use std::path::PathBuf;
use std::process::Command;

fn mint_neo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mint-neo"))
}

fn write_temp(name: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("mint-neo-{name}-{}", std::process::id()));
    std::fs::write(&path, contents).expect("write temp");
    path
}

#[test]
fn abi_list_and_show() {
    let list = mint_neo().args(["abi", "list"]).output().expect("list");
    assert!(list.status.success());
    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(stdout.contains("generic-le"));
    assert!(stdout.contains("ti-c28x-eabi"));
    assert!(list.stderr.is_empty());

    let show = mint_neo()
        .args(["abi", "show", "tricore-eabi-le"])
        .output()
        .expect("show");
    assert!(show.status.success());
    let stdout = String::from_utf8_lossy(&show.stdout);
    assert!(stdout.contains("name: tricore-eabi-le"));
    assert!(stdout.contains("u64"));
    assert!(show.stderr.is_empty());
}

#[test]
fn fingerprint_prints_hex_newline() {
    let header = write_temp(
        "fp.h",
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct { uint32_t id; } config_t;
"#,
    );
    let output = mint_neo()
        .args(["fingerprint", &header.display().to_string()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.len(), 17);
    assert!(stdout.ends_with('\n'));
    assert!(stdout[..16].chars().all(|c| c.is_ascii_hexdigit()));
    assert!(output.stderr.is_empty());
}

#[test]
fn build_writes_hex_and_usage_is_exit_2() {
    let header = write_temp(
        "build.h",
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0x10
 */
typedef struct { uint32_t id; } config_t;
"#,
    );
    let json = write_temp("build.json", r#"{"id": 1}"#);
    let out = std::env::temp_dir().join(format!("mint-neo-out-{}.hex", std::process::id()));
    let output = mint_neo()
        .args([
            "build",
            &header.display().to_string(),
            "--json",
            &json.display().to_string(),
            "--out",
            &out.display().to_string(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let hex = std::fs::read_to_string(&out).unwrap();
    assert!(hex.contains(":020000040000FA"));
    assert!(hex.ends_with(":00000001FF\n"));

    let usage = mint_neo().output().unwrap();
    assert_eq!(usage.status.code(), Some(2));
}

#[test]
fn schema_failure_is_exit_1() {
    let header = write_temp("bad.h", "#include <stdio.h>\n");
    let output = mint_neo()
        .args(["fingerprint", &header.display().to_string()])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(!output.stderr.is_empty());
    assert!(output.stdout.is_empty());
}
