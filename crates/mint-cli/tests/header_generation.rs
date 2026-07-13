use std::fs;
use std::path::Path;
use std::process::Command;

#[path = "common/mod.rs"]
mod common;

fn mint_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mint"));
    command.current_dir(env!("CARGO_MANIFEST_DIR"));
    command
}

fn compile_c11(source: &Path, include: &Path, object: &Path) -> std::process::Output {
    let compiler =
        std::env::var_os("CC").unwrap_or_else(|| if cfg!(windows) { "clang" } else { "cc" }.into());
    Command::new(compiler)
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-pedantic", "-c"])
        .arg(source)
        .arg("-I")
        .arg(include)
        .arg("-o")
        .arg(object)
        .output()
        .expect("C compiler should run")
}

#[test]
fn generated_example_header_is_checked_in_and_compiles_as_c11() {
    common::ensure_out_dir();
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
        .expect("checked-in generated header is readable");
    assert_eq!(generated, checked_in);

    let source_path = header_path.with_extension("c");
    let object_path = header_path.with_extension("o");
    let header_name = header_path
        .file_name()
        .expect("header has a file name")
        .to_string_lossy();
    fs::write(
        &source_path,
        format!(
            r#"#include <stddef.h>
#include "{header_name}"
#include "{header_name}"

_Static_assert(CONFIG_DEVICE_NAME_LEN == 16u, "name extent");
_Static_assert(CONFIG_MATRIX_ROWS == 2u, "matrix rows");
_Static_assert(CONFIG_MATRIX_COLS == 2u, "matrix columns");
_Static_assert(CONFIG_FLAGS_ENABLE_DEBUG_SHIFT == 0u, "bitmap shift");
_Static_assert(CONFIG_FLAGS_REGION_CODE_MASK == UINT16_C(0x00F0), "bitmap mask");
_Static_assert(offsetof(config_t, device.id) == 0u, "device id offset");
_Static_assert(offsetof(config_t, device.name) == 4u, "device name offset");
_Static_assert(offsetof(config_t, flags) == 24u, "flags offset");
_Static_assert(offsetof(config_t, coefficients) == 28u, "coefficients offset");
_Static_assert(offsetof(config_t, matrix) == 44u, "matrix offset");
_Static_assert(offsetof(config_t, checksum) == 52u, "checksum offset");
_Static_assert(sizeof(config_t) == 56u, "config size");
_Static_assert(sizeof(data_t) == 32u, "data size");

int use_generated_header(config_t *config, data_t *data) {{
  config->device.id = 1u;
  config->device.name[0] = 2u;
  config->matrix[0][0] = 3;
  data->message[0] = config->device.name[0];
  return (int)(config->coefficients[0] + (float)data->message[0]);
}}
"#
        ),
    )
    .expect("C source writes");

    let compile = compile_c11(
        &source_path,
        header_path.parent().expect("header has a parent"),
        &object_path,
    );
    assert!(
        compile.status.success(),
        "C compiler stderr: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
}

#[test]
fn validation_failure_does_not_touch_output() {
    let layout = common::write_layout_file(
        "invalid-header",
        r#"
[mint]
endianness = "little"
[block.header]
start_address = 0
length = 16
[block.data]
for = { value = 1, type = "u8" }
"#,
    );
    let output_path = common::unique_out_path("preserved-header", "h");
    fs::write(&output_path, "preserve me\n").expect("sentinel header writes");

    let output = mint_command()
        .args(["header", &layout, "-o"])
        .arg(&output_path)
        .output()
        .expect("mint header should run");
    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(output_path).expect("sentinel header remains readable"),
        "preserve me\n"
    );
}
