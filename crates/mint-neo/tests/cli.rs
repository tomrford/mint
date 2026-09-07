use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

const HEADER: &str = "/**\n * @mint block\n * @mint abi generic-le\n * @mint start-address 0x10\n */\ntypedef struct { uint32_t id; } config_t;\n";
const JSON: &str = r#"{"id": 1}"#;
const HEX: &str = ":020000040000FA\n:0400100001000000EB\n:00000001FF\n";

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("schema.h"), HEADER).unwrap();
    fs::write(dir.path().join("values.json"), JSON).unwrap();
    dir
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mint-neo"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn build(dir: &Path, out: &str) -> Output {
    run(
        dir,
        &["build", "schema.h", "--json", "values.json", "--out", out],
    )
}

fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout).unwrap()
}

fn inputs_intact(dir: &Path) {
    assert_eq!(fs::read_to_string(dir.join("schema.h")).unwrap(), HEADER);
    assert_eq!(fs::read_to_string(dir.join("values.json")).unwrap(), JSON);
}

#[test]
fn abi_list_and_show() {
    let dir = fixture();
    let list = success(run(dir.path(), &["abi", "list"]));
    assert!(list.contains("generic-le") && list.contains("ti-c28x-eabi"));
    let show = success(run(dir.path(), &["abi", "show", "tricore-eabi-le"]));
    assert!(show.contains("name: tricore-eabi-le") && show.contains("u64"));
}

#[test]
fn fingerprint_prints_hex_newline() {
    let dir = fixture();
    let stdout = success(run(dir.path(), &["fingerprint", "schema.h"]));
    assert_eq!(stdout.len(), 17);
    assert!(stdout.ends_with('\n'));
    assert!(stdout[..16].chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn build_writes_hex() {
    let dir = fixture();
    assert_eq!(success(build(dir.path(), "image.hex")), "");
    assert_eq!(
        fs::read_to_string(dir.path().join("image.hex")).unwrap(),
        HEX
    );
}

#[test]
fn build_rejects_output_paths_that_resolve_to_an_input() {
    let dir = fixture();
    for (out, name) in [("schema.h", "header"), ("values.json", "JSON input")] {
        let output = build(dir.path(), out);
        assert_eq!(output.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains(&format!("--out resolves to the {name} path"))
        );
    }
    inputs_intact(dir.path());
}

#[test]
#[cfg(unix)]
fn output_symlinks_to_inputs_are_rejected() {
    let dir = fixture();
    for input in ["schema.h", "values.json"] {
        let out = format!("{input}.hex");
        std::os::unix::fs::symlink(input, dir.path().join(&out)).unwrap();
        let output = build(dir.path(), &out);
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("--out resolves to"));
    }
    inputs_intact(dir.path());
}

#[test]
#[cfg(unix)]
fn output_permissions_follow_creation_umask_and_preserve_existing_modes() {
    use std::os::unix::fs::PermissionsExt;
    let dir = fixture();
    let out = dir.path().join("image.hex");
    let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    success(build(dir.path(), "image.hex"));
    assert_eq!(mode(&out), mode(&dir.path().join("values.json")));
    fs::set_permissions(&out, fs::Permissions::from_mode(0o640)).unwrap();
    success(build(dir.path(), "image.hex"));
    assert_eq!(mode(&out), 0o640);
}

#[test]
fn replacing_hard_linked_outputs_preserves_inputs() {
    let dir = fixture();
    for input in ["schema.h", "values.json"] {
        let out = format!("{input}.hex");
        fs::hard_link(dir.path().join(input), dir.path().join(&out)).unwrap();
        success(build(dir.path(), &out));
        assert_eq!(fs::read_to_string(dir.path().join(out)).unwrap(), HEX);
    }
    inputs_intact(dir.path());
}

#[test]
fn failed_build_preserves_existing_output_and_cleans_temporary_files() {
    let dir = fixture();
    let out = dir.path().join("image.hex");
    fs::write(dir.path().join("values.json"), r#"{"id": 4294967296}"#).unwrap();
    fs::write(&out, "previous image").unwrap();
    assert_eq!(build(dir.path(), "image.hex").status.code(), Some(1));
    assert_eq!(fs::read_to_string(out).unwrap(), "previous image");

    // Force replacement to fail after writing the temporary file.
    fs::write(dir.path().join("values.json"), JSON).unwrap();
    fs::create_dir(dir.path().join("directory")).unwrap();
    assert_eq!(build(dir.path(), "directory").status.code(), Some(1));
    assert!(dir.path().join("directory").is_dir());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 4);
}

#[test]
fn schema_failure_is_exit_1() {
    let dir = fixture();
    fs::write(dir.path().join("schema.h"), "#include <stdio.h>\n").unwrap();
    let output = run(dir.path(), &["fingerprint", "schema.h"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(!output.stderr.is_empty());
    assert!(output.stdout.is_empty());
}
