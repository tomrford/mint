use mint_cli::commands;
use mint_cli::data;
use mint_cli::layout::args::{BlockNames, LayoutArgs};
use mint_cli::output::args::{OutputArgs, OutputFormat};

#[path = "common/mod.rs"]
mod common;

#[test]
fn fixed_point_literals_and_arrays_encode_little_endian() {
    let layout = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
gain = { value = 1.5, type = "uq8.8" }
round_up = { value = 0.005859375, type = "uq8.8" }
round_even = { value = 0.009765625, type = "uq8.8" }
samples = { value = [0.5, 1.0], type = "uq8.8", size = 2 }
"#;

    let path = common::write_layout_file("fixed-point-literals", layout);
    let cfg = mint_cli::layout::load_layout(&path).expect("layout loads");
    let block = cfg.blocks.get("block").expect("block present");

    let (bytes, _) = common::build_block(block, &cfg.mint, true, None).expect("build succeeds");
    assert_eq!(
        bytes,
        vec![0x80, 0x01, 0x02, 0x00, 0x02, 0x00, 0x80, 0x00, 0x00, 0x01]
    );
}

#[test]
fn fixed_point_signed_big_endian_values_encode() {
    let layout = r#"
[mint]
endianness = "big"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
offset = { value = -1.25, type = "q7.8" }
unit = { value = 1.0, type = "uq8.8" }
"#;

    let path = common::write_layout_file("fixed-point-big-endian", layout);
    let cfg = mint_cli::layout::load_layout(&path).expect("layout loads");
    let block = cfg.blocks.get("block").expect("block present");

    let (bytes, _) = common::build_block(block, &cfg.mint, true, None).expect("build succeeds");
    assert_eq!(bytes, vec![0xFE, 0xC0, 0x01, 0x00]);
}

#[test]
fn fixed_point_json_values_and_2d_arrays_encode() {
    let layout = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
ratio = { name = "Ratio", type = "uq0.16" }
grid = { name = "Grid", type = "uq8.8", size = [2, 2] }
"#;

    let path = common::write_layout_file("fixed-point-json", layout);
    let cfg = mint_cli::layout::load_layout(&path).expect("layout loads");
    let block = cfg.blocks.get("block").expect("block present");

    let data_args = data::args::DataArgs {
        json: Some(r#"{"Default":{"Ratio":0.25,"Grid":[[0.5,1.0],[1.5,2.0]]}}"#.to_owned()),
        versions: Some("Default".to_owned()),
        ..Default::default()
    };
    let ds = data::create_data_source(&data_args)
        .expect("datasource loads")
        .expect("datasource available");

    let (bytes, _) =
        common::build_block(block, &cfg.mint, true, Some(ds.as_ref())).expect("build succeeds");
    assert_eq!(
        bytes,
        vec![0x00, 0x40, 0x80, 0x00, 0x00, 0x01, 0x80, 0x01, 0x00, 0x02]
    );
}

#[test]
fn fixed_point_used_values_report_resolved_numbers() {
    let layout = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
gain = { value = 1.5, type = "uq8.8" }
samples = { value = [0.5, 1.0], type = "uq8.8", size = 2 }
"#;

    let path = common::write_layout_file("fixed-point-values", layout);
    let cfg = mint_cli::layout::load_layout(&path).expect("layout loads");
    let block = cfg.blocks.get("block").expect("block present");

    let ((_, _), values) =
        common::build_block_with_values(block, &cfg.mint).expect("build succeeds");
    assert_eq!(values["gain"].as_f64(), Some(1.5));
    assert_eq!(values["samples"][0].as_f64(), Some(0.5));
    assert_eq!(values["samples"][1].as_f64(), Some(1.0));
}

#[test]
fn fixed_point_strict_rejects_overflow_and_non_strict_clamps() {
    let layout = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
gain = { value = 300.5, type = "uq8.8" }
"#;

    let path = common::write_layout_file("fixed-point-overflow", layout);
    let cfg = mint_cli::layout::load_layout(&path).expect("layout loads");
    let block = cfg.blocks.get("block").expect("block present");

    let err = common::build_block(block, &cfg.mint, true, None).expect_err("strict should fail");
    let message = err.to_string();
    assert!(
        message.contains("fixed-point type 'uq8.8'") && message.contains("300.5"),
        "unexpected error: {message}"
    );

    let (bytes, _) = common::build_block(block, &cfg.mint, false, None)
        .expect("non-strict overflow should clamp");
    assert_eq!(bytes, vec![0xFF, 0xFF]);
}

#[test]
fn fixed_point_64bit_float_overflow_clamps_in_non_strict_mode() {
    let layout = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
unsigned_limit = { value = 18446744073709551616.0, type = "uq64.0" }
signed_limit = { value = 9223372036854775808.0, type = "q63.0" }
"#;

    let path = common::write_layout_file("fixed-point-64bit-float-overflow", layout);
    let cfg = mint_cli::layout::load_layout(&path).expect("layout loads");
    let block = cfg.blocks.get("block").expect("block present");

    let err = common::build_block(block, &cfg.mint, true, None).expect_err("strict should fail");
    assert!(
        err.to_string().contains("fixed-point type 'uq64.0'"),
        "unexpected strict error: {err}"
    );

    let (bytes, _) = common::build_block(block, &cfg.mint, false, None)
        .expect("non-strict overflow should clamp");
    assert_eq!(
        bytes,
        vec![
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0x7F,
        ]
    );
}

#[test]
fn fixed_point_rejects_non_finite_input() {
    let layout = r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
gain = { value = inf, type = "uq8.8" }
"#;

    let path = common::write_layout_file("fixed-point-non-finite", layout);
    let cfg = mint_cli::layout::load_layout(&path).expect("layout loads");
    let block = cfg.blocks.get("block").expect("block present");

    let err = common::build_block(block, &cfg.mint, false, None).expect_err("build should fail");
    assert!(
        err.to_string().contains("cannot encode non-finite"),
        "unexpected error: {err}"
    );
}

#[test]
fn fixed_point_export_json_reports_resolved_numbers() {
    let layout = r#"
[mint]
endianness = "little"

[config.header]
start_address = 0x1000
length = 0x40

[config.data]
ratio = { value = 1.5, type = "uq8.8" }
phase = { name = "Phase", type = "uq0.16" }
"#;

    let layout_path = common::write_layout_file("fixed-point-export", layout);
    let layout_key = layout_path.clone();
    let data_args = data::args::DataArgs {
        json: Some(r#"{"Default":{"Phase":0.25}}"#.to_owned()),
        versions: Some("Default".to_owned()),
        ..Default::default()
    };
    let ds = data::create_data_source(&data_args)
        .expect("datasource loads")
        .expect("datasource available");

    let json_out = common::unique_out_path("fixed-point-export", "json");
    let args = mint_cli::args::Args {
        layout: LayoutArgs {
            blocks: vec![BlockNames {
                name: "".to_owned(),
                file: layout_path,
            }],
            strict: true,
        },
        data: data_args,
        output: OutputArgs {
            out: common::unique_out_path("fixed-point-export", "hex"),
            record_width: 16,
            format: OutputFormat::Hex,
            export_json: Some(json_out.clone()),
            stats: false,
            quiet: true,
        },
    };

    commands::build(&args, Some(ds.as_ref())).expect("build should succeed");

    let report = std::fs::read_to_string(&json_out).expect("read json report");
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse json report");
    assert_eq!(json[&layout_key]["config"]["ratio"].as_f64(), Some(1.5));
    assert_eq!(json[&layout_key]["config"]["phase"].as_f64(), Some(0.25));
}

#[test]
fn fixed_point_rejects_bitmap_ref_and_checksum_storage() {
    for (stem, layout, expected) in [
        (
            "fixed-point-bitmap",
            r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
flags = { type = "uq8.8", bitmap = [
    { bits = 8, value = 0 },
    { bits = 8, value = 0 }
] }
"#,
            "Bitmap does not support fixed-point",
        ),
        (
            "fixed-point-ref",
            r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
target = { value = 1, type = "u16" }
ptr = { ref = "target", type = "uq8.8" }
"#,
            "Ref does not support fixed-point",
        ),
        (
            "fixed-point-checksum",
            r#"
[mint]
endianness = "little"

[mint.checksum.crc32]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
value = { value = 1, type = "u16" }
checksum = { checksum = "crc32", type = "uq8.8" }
"#,
            "Checksum does not support fixed-point",
        ),
    ] {
        let path = common::write_layout_file(stem, layout);
        let cfg = mint_cli::layout::load_layout(&path).expect("layout loads");
        let block = cfg.blocks.get("block").expect("block present");

        let err = common::build_block(block, &cfg.mint, true, None).expect_err("build should fail");
        assert!(
            err.to_string().contains(expected),
            "expected '{expected}', got: {err}"
        );
    }
}
