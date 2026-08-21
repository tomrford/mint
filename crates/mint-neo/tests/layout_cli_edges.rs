use std::path::PathBuf;
use std::process::Command;

use mint_neo::{InspectFormat, Source, compile_header, encode_json, inspect, render_hex};

fn mint_neo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mint-neo"))
}

fn header(text: &str) -> Source {
    Source::new("config.h", text)
}

fn json(text: &str) -> Source {
    Source::new("config.json", text)
}

fn write_temp(name: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("mint-neo-edges-{name}-{}", std::process::id()));
    std::fs::write(&path, contents).expect("write temp");
    path
}

fn parse_i32hex(hex: &str) -> Vec<(u8, u16, u8, Vec<u8>)> {
    hex.lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            assert!(line.starts_with(':'), "{line}");
            let data = &line[1..];
            assert_eq!(data.len() % 2, 0, "{line}");
            let bytes: Vec<u8> = (0..data.len() / 2)
                .map(|index| {
                    u8::from_str_radix(&data[2 * index..2 * index + 2], 16).expect("hex digit")
                })
                .collect();
            let len = bytes[0];
            let address = u16::from_be_bytes([bytes[1], bytes[2]]);
            let record_type = bytes[3];
            let payload = bytes[4..4 + usize::from(len)].to_vec();
            (len, address, record_type, payload)
        })
        .collect()
}

#[test]
fn help_and_version_use_clap_success_exit() {
    let cases: &[&[&str]] = &[
        &["--help"],
        &["-h"],
        &["--version"],
        &["-V"],
        &["build", "--help"],
        &["build", "--version"],
        &["fingerprint", "--help"],
        &["inspect", "--help"],
        &["inspect", "--version"],
        &["abi", "--help"],
        &["abi", "list", "--help"],
        &["abi", "show", "--help"],
    ];
    for args in cases {
        let output = mint_neo().args(*args).output().expect("run");
        assert_eq!(
            output.status.code(),
            Some(0),
            "args={args:?} stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !output.stdout.is_empty(),
            "expected help/version on stdout for {args:?}"
        );
    }

    let help = mint_neo().args(["--help"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&help.stdout);
    assert!(stdout.contains("build"));
    assert!(stdout.contains("fingerprint"));
    assert!(stdout.contains("inspect"));
    assert!(stdout.contains("abi"));

    let version = mint_neo().args(["--version"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&version.stdout);
    assert!(stdout.contains("mint-neo"));
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn usage_errors_remain_exit_2() {
    let bare = mint_neo().output().unwrap();
    assert_eq!(bare.status.code(), Some(2));

    let missing = mint_neo().args(["build"]).output().unwrap();
    assert_eq!(missing.status.code(), Some(2));
    assert!(!missing.stderr.is_empty());

    let unknown = mint_neo().args(["not-a-command"]).output().unwrap();
    assert_eq!(unknown.status.code(), Some(2));

    let bad_format = mint_neo()
        .args(["inspect", "--format", "xml", "config.h"])
        .output()
        .unwrap();
    assert_eq!(bad_format.status.code(), Some(2));
}

#[test]
fn huge_scalar_array_compiles_in_bounded_work() {
    let schema = compile_header(header(
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct {
    uint32_t values[33554432];
} config_t;
"#,
    ))
    .expect("huge scalar array should compile");
    assert_eq!(schema.layout.root_layout().size, 33554432 * 4);
    assert!(schema.layout.padding_ranges.is_empty());
    assert_eq!(schema.layout.padding_octets(), 0);

    let text = inspect(&schema, InspectFormat::Text).unwrap();
    assert!(text.contains("values"));
    assert!(text.contains("dims [33554432]"));
    assert!(
        !text.contains("values[0]"),
        "inspect must not unroll array elements: {text}"
    );
}

#[test]
fn huge_record_array_padding_is_compact() {
    let schema = compile_header(header(
        r#"
#include <stdint.h>
typedef struct {
    uint8_t tag;
    uint32_t value;
} item_t;
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct {
    item_t items[8388608];
} config_t;
"#,
    ))
    .expect("huge record array should compile");
    assert_eq!(schema.layout.root_layout().size, 8388608 * 8);
    assert_eq!(
        schema.layout.padding_ranges.len(),
        1,
        "padding discovery must not allocate one range per element: {:?}",
        schema.layout.padding_ranges
    );
    let range = &schema.layout.padding_ranges[0];
    assert_eq!(range.offset, 1);
    assert_eq!(range.size, 3);
    assert_eq!(range.path, "items[]");
    assert_eq!(range.repeats.len(), 1);
    assert_eq!(range.repeats[0].count, 8_388_608);
    assert_eq!(range.repeats[0].stride, 8);
    assert_eq!(schema.layout.padding_octets(), 8_388_608 * 3);

    let text = inspect(&schema, InspectFormat::Text).unwrap();
    assert!(text.contains("items[] [1, 4) × 8388608 stride 8  25165824 octets"));
    assert!(text.contains("items[].tag"));
    assert!(text.contains("items[].value"));
    assert!(text.contains("padding octets: 25165824"));

    let json = inspect(&schema, InspectFormat::Json).unwrap();
    assert_eq!(json.matches("\"count\": 8388608").count(), 1);
    assert!(json.contains("\"stride\": 8"));
    assert!(json.contains("\"path\": \"items[]\""));
}

#[test]
fn mixed_and_nested_padding_stays_compact_and_inspectable() {
    let schema = compile_header(header(
        r#"
#include <stdint.h>
typedef struct {
    uint8_t a;
    uint32_t b;
} cell_t;
typedef struct {
    cell_t cells[3];
    uint8_t extra;
} group_t;
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct {
    uint8_t lead;
    uint32_t id;
    group_t groups[2];
    cell_t grid[2][3];
} config_t;
"#,
    ))
    .expect("header");

    assert_eq!(schema.layout.padding_ranges.len(), 4);
    assert_eq!(schema.layout.padding_ranges[0].offset, 1);
    assert_eq!(schema.layout.padding_ranges[0].size, 3);
    assert!(schema.layout.padding_ranges[0].repeats.is_empty());

    let cell_in_group = schema
        .layout
        .padding_ranges
        .iter()
        .find(|range| range.path == "groups[].cells[]")
        .expect("group cell padding");
    assert_eq!(cell_in_group.offset, 9);
    assert_eq!(cell_in_group.size, 3);
    assert_eq!(cell_in_group.repeats.len(), 2);
    assert_eq!(cell_in_group.repeats[0].count, 3);
    assert_eq!(cell_in_group.repeats[0].stride, 8);
    assert_eq!(cell_in_group.repeats[1].count, 2);
    assert_eq!(cell_in_group.repeats[1].stride, 28);

    let group_tail = schema
        .layout
        .padding_ranges
        .iter()
        .find(|range| range.path == "groups[]" && range.offset == 33)
        .expect("group tail padding");
    assert_eq!(group_tail.size, 3);
    assert_eq!(group_tail.repeats.len(), 1);
    assert_eq!(group_tail.repeats[0].count, 2);
    assert_eq!(group_tail.repeats[0].stride, 28);

    let grid = schema
        .layout
        .padding_ranges
        .iter()
        .find(|range| range.path == "grid[]")
        .expect("grid padding");
    assert_eq!(grid.offset, 65);
    assert_eq!(grid.size, 3);
    assert_eq!(grid.repeats.len(), 1);
    assert_eq!(grid.repeats[0].count, 6);
    assert_eq!(grid.repeats[0].stride, 8);

    let text = inspect(&schema, InspectFormat::Text).unwrap();
    assert!(text.contains("groups[].cells[]"));
    assert!(text.contains("× 3 stride 8 × 2 stride 28"));
    assert!(text.contains("grid[]"));
    assert!(text.contains("× 6 stride 8"));
}

#[test]
fn oversized_root_is_rejected_without_unrolling() {
    let error = compile_header(header(
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct {
    uint32_t values[67108865];
} config_t;
"#,
    ))
    .expect_err("256 MiB + 4 must be rejected");
    assert!(error.to_string().contains("256 MiB"));
}

#[test]
fn inspect_cli_reports_compact_array_padding() {
    let header = write_temp(
        "pad.h",
        r#"
#include <stdint.h>
typedef struct {
    uint8_t tag;
    uint32_t value;
} item_t;
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct {
    item_t items[4];
} config_t;
"#,
    );
    let output = mint_neo()
        .args(["inspect", &header.display().to_string(), "--format", "text"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("items[] [1, 4) × 4 stride 8  12 octets"));
    assert!(stdout.contains("padding octets: 12"));
}

#[test]
fn generic_hex_uses_32_octet_records_and_trailing_short() {
    let schema = compile_header(header(
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0x10
 */
typedef struct {
    uint8_t bytes[40];
} config_t;
"#,
    ))
    .unwrap();
    let values: Vec<String> = (0..40).map(|value| value.to_string()).collect();
    let bytes = encode_json(
        &schema,
        &json(&format!("{{\"bytes\":[{}]}}", values.join(","))),
    )
    .unwrap();
    assert_eq!(bytes.len(), 40);
    let hex = render_hex(&schema, &bytes).unwrap();
    let records = parse_i32hex(&hex);
    assert_eq!(records[0], (2, 0, 0x04, vec![0x00, 0x00]));
    assert_eq!(records[1].0, 32);
    assert_eq!(records[1].1, 0x0010);
    assert_eq!(records[1].2, 0x00);
    assert_eq!(records[1].3, (0u8..32).collect::<Vec<_>>());
    assert_eq!(records[2].0, 8);
    assert_eq!(records[2].1, 0x0030);
    assert_eq!(records[2].3, (32u8..40).collect::<Vec<_>>());
    assert_eq!(records.last(), Some(&(0, 0, 0x01, Vec::new())));
    assert!(records.iter().all(|record| record.0 <= 32));
    assert!(hex.ends_with(":00000001FF\n"));
    assert!(!hex.contains("\r\n"));
}

#[test]
fn c28x_hex_converts_word_address_and_emits_32_octet_records() {
    let schema = compile_header(header(
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi ti-c28x-eabi
 * @mint start-address 0x100
 */
typedef struct {
    uint16_t words[20];
} config_t;
"#,
    ))
    .unwrap();
    assert_eq!(schema.layout.octet_start().unwrap(), 0x200);
    assert_eq!(schema.layout.root_layout().size, 40);

    let values: Vec<String> = (1..=20).map(|value| value.to_string()).collect();
    let bytes = encode_json(
        &schema,
        &json(&format!("{{\"words\":[{}]}}", values.join(","))),
    )
    .unwrap();
    let expected: Vec<u8> = (1u16..=20).flat_map(u16::to_le_bytes).collect();
    assert_eq!(bytes, expected);

    let hex = render_hex(&schema, &bytes).unwrap();
    let records = parse_i32hex(&hex);
    assert_eq!(records[0], (2, 0, 0x04, vec![0x00, 0x00]));
    assert_eq!(records[1].0, 32);
    assert_eq!(records[1].1, 0x0200);
    assert_eq!(records[1].3, expected[..32]);
    assert_eq!(records[2].0, 8);
    assert_eq!(records[2].1, 0x0220);
    assert_eq!(records[2].3, expected[32..]);
    assert_eq!(records.last(), Some(&(0, 0, 0x01, Vec::new())));
    assert!(records.iter().all(|record| record.0 <= 32));
}

#[test]
fn c28x_hex_emits_ela_for_high_word_address_and_segment_cross() {
    let high = compile_header(header(
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi ti-c28x-eabi
 * @mint start-address 0x8000
 */
typedef struct {
    uint16_t words[16];
} config_t;
"#,
    ))
    .unwrap();
    assert_eq!(high.layout.octet_start().unwrap(), 0x1_0000);
    let values: Vec<String> = (0..16).map(|value| value.to_string()).collect();
    let bytes = encode_json(
        &high,
        &json(&format!("{{\"words\":[{}]}}", values.join(","))),
    )
    .unwrap();
    let hex = render_hex(&high, &bytes).unwrap();
    let records = parse_i32hex(&hex);
    assert_eq!(records[0], (2, 0, 0x04, vec![0x00, 0x01]));
    assert_eq!(records[1].0, 32);
    assert_eq!(records[1].1, 0x0000);
    assert!(hex.contains(":020000040001F9\n"));

    let cross = compile_header(header(
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi ti-c28x-eabi
 * @mint start-address 0x7FF8
 */
typedef struct {
    uint16_t words[20];
} config_t;
"#,
    ))
    .unwrap();
    assert_eq!(cross.layout.octet_start().unwrap(), 0xFFF0);
    let values: Vec<String> = (1..=20).map(|value| value.to_string()).collect();
    let bytes = encode_json(
        &cross,
        &json(&format!("{{\"words\":[{}]}}", values.join(","))),
    )
    .unwrap();
    let hex = render_hex(&cross, &bytes).unwrap();
    let records = parse_i32hex(&hex);
    assert_eq!(records[0], (2, 0, 0x04, vec![0x00, 0x00]));
    assert_eq!(records[1].0, 16);
    assert_eq!(records[1].1, 0xFFF0);
    assert_eq!(records[2], (2, 0, 0x04, vec![0x00, 0x01]));
    assert_eq!(records[3].0, 24);
    assert_eq!(records[3].1, 0x0000);
    assert!(records.iter().all(|record| record.0 <= 32));
    assert!(hex.contains(":020000040000FA\n"));
    assert!(hex.contains(":020000040001F9\n"));
}
