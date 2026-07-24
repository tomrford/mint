use mint_core::build::{self, BlockSelector, BuildFromLayoutsRequest, BuildRequest, NamedLayout};
use mint_core::data::{DataSource, ExcelDataSource, ExcelDataSourceOptions, JsonDataSource};
use mint_core::layout;
use mint_core::layout::value::DataValue;
use mint_core::output::OutputFormat;
use std::path::PathBuf;

#[path = "common/mod.rs"]
mod common;

fn simple_block_selector(file: &str) -> BlockSelector {
    BlockSelector::named(file, "simple_block")
}

#[test]
fn build_api_returns_intermediate_ranges_and_rendered_output() {
    let artifact = build::build(BuildRequest {
        blocks: vec![simple_block_selector("tests/data/blocks.toml")],
        data_source: None,
        strict: false,
        capture_values: false,
    })
    .expect("build should succeed");

    assert_eq!(artifact.stats.blocks_processed, 1);
    assert_eq!(artifact.ranges.len(), 1);
    assert_eq!(artifact.ranges[0].start_address, 0x8000);
    assert!(artifact.used_values.is_none());

    let hex = artifact
        .render(OutputFormat::Hex, 32)
        .expect("artifact should render to hex");
    assert!(hex.starts_with(':'));
}

#[test]
fn build_deduplicates_equivalent_layout_paths() {
    let first_path = "tests/data/blocks.toml";
    let artifact = build::build(BuildRequest {
        blocks: vec![
            simple_block_selector(first_path),
            simple_block_selector("tests/data/./blocks.toml"),
        ],
        data_source: None,
        strict: false,
        capture_values: false,
    })
    .expect("equivalent layout paths should be deduplicated");

    assert_eq!(artifact.stats.blocks_processed, 1);
    assert_eq!(
        artifact.stats.block_stats[0].layout,
        PathBuf::from(first_path)
    );
}

#[test]
fn build_from_layouts_accepts_parsed_toml_layouts() {
    let config = layout::parse_toml_layout(include_str!("data/blocks.toml"))
        .expect("layout string should parse");
    let artifact = build::build_from_layouts(BuildFromLayoutsRequest {
        layouts: vec![NamedLayout {
            name: PathBuf::from("memory-layout"),
            config,
        }],
        blocks: vec![simple_block_selector("memory-layout")],
        data_source: None,
        strict: false,
        capture_values: true,
    })
    .expect("in-memory layout build should succeed");

    assert_eq!(artifact.stats.blocks_processed, 1);
    assert!(artifact.used_values.is_some());
}

#[test]
fn build_rejects_range_that_exceeds_address_space() {
    let config = layout::parse_toml_layout(
        r#"
[mint]
abi = "generic-le"

[oversized.header]
start_address = 0xFFFFFFF0
length = 0x20

[oversized.data]
value = { value = 1, type = "u8" }
"#,
    )
    .expect("layout string should parse");

    let error = build::build_from_layouts(BuildFromLayoutsRequest {
        layouts: vec![NamedLayout {
            name: PathBuf::from("overflow.toml"),
            config,
        }],
        blocks: vec![BlockSelector::named("overflow.toml", "oversized")],
        data_source: None,
        strict: false,
        capture_values: false,
    })
    .expect_err("oversized range should be rejected");

    let message = common::error_chain(&error);
    assert!(
        message.contains("block 'oversized' from 'overflow.toml'")
            && message.contains("exceeds the 32-bit address space"),
        "unexpected error: {message}"
    );
}

#[test]
fn c28x_build_renders_standard_octet_addresses() {
    let config = layout::parse_toml_layout(
        r#"
[mint]
abi = "ti-c28x-eabi"

[block.header]
start_address = 0x1000
length = 8

[block.data]
first = { value = 0x1234, type = "u16" }
label = { value = "OK", type = "u16", size = 2 }
second = { value = 0x5678, type = "u16" }
"#,
    )
    .expect("layout string should parse");

    let artifact = build::build_from_layouts(BuildFromLayoutsRequest {
        layouts: vec![NamedLayout {
            name: PathBuf::from("c28x.toml"),
            config,
        }],
        blocks: vec![BlockSelector::named("c28x.toml", "block")],
        data_source: None,
        strict: false,
        capture_values: false,
    })
    .expect("C28x block should build");

    assert_eq!(artifact.ranges[0].start_address, 0x1000);
    assert_eq!(artifact.ranges[0].output_start_address().unwrap(), 0x2000);
    assert_eq!(
        artifact.ranges[0].bytestream,
        [0x34, 0x12, 0x4F, 0x00, 0x4B, 0x00, 0x78, 0x56]
    );
    let output = artifact.render(OutputFormat::Hex, 16).expect("hex renders");
    assert!(
        output.lines().any(|line| line.starts_with(":08200000")),
        "{output}"
    );
}

#[test]
fn u16_strings_encode_utf8_bytes_in_abi_byte_order() {
    for (abi, expected) in [
        ("generic-le", [0x41, 0x00, 0xC3, 0x00, 0xA9, 0x00]),
        ("generic-be", [0x00, 0x41, 0x00, 0xC3, 0x00, 0xA9]),
    ] {
        let source = format!(
            r#"
[mint]
abi = "{abi}"

[block.header]
start_address = 0
length = 6

[block.data]
label = {{ value = "Aé", type = "u16", size = 3 }}
"#
        );
        let config = layout::parse_toml_layout(&source).expect("layout string should parse");
        let artifact = build::build_from_layouts(BuildFromLayoutsRequest {
            layouts: vec![NamedLayout {
                name: PathBuf::from("string.toml"),
                config,
            }],
            blocks: vec![BlockSelector::named("string.toml", "block")],
            data_source: None,
            strict: false,
            capture_values: false,
        })
        .expect("u16 string should build");

        assert_eq!(artifact.ranges[0].bytestream, expected, "ABI {abi}");
    }
}

#[test]
fn strings_reject_non_byte_storage_types() {
    let config = layout::parse_toml_layout(
        r#"
[mint]
abi = "generic-le"

[block.header]
start_address = 0
length = 2

[block.data]
label = { value = "A", type = "i16", size = 1 }
"#,
    )
    .expect("layout string should parse");

    let error = build::build_from_layouts(BuildFromLayoutsRequest {
        layouts: vec![NamedLayout {
            name: PathBuf::from("invalid-string.toml"),
            config,
        }],
        blocks: vec![BlockSelector::named("invalid-string.toml", "block")],
        data_source: None,
        strict: false,
        capture_values: false,
    })
    .expect_err("non-u8/u16 string storage should fail");

    assert!(
        common::error_chain(&error).contains("Strings should have type u8 or u16."),
        "{error}"
    );
}

#[test]
fn c28x_rejects_scaled_output_address_overflow() {
    let config = layout::parse_toml_layout(
        r#"
[mint]
abi = "ti-c28x-eabi"

[overflow.header]
start_address = 0x80000000
length = 2

[overflow.data]
value = { value = 1, type = "u16" }
"#,
    )
    .expect("layout string should parse");

    let error = build::build_from_layouts(BuildFromLayoutsRequest {
        layouts: vec![NamedLayout {
            name: PathBuf::from("c28x-overflow.toml"),
            config,
        }],
        blocks: vec![BlockSelector::named("c28x-overflow.toml", "overflow")],
        data_source: None,
        strict: false,
        capture_values: false,
    })
    .expect_err("scaled C28x range should be rejected");

    assert!(
        common::error_chain(&error).contains("octet-addressed output range"),
        "{error}"
    );
}

#[test]
fn data_sources_can_be_constructed_without_cli_args() {
    let variants = vec!["Default".to_owned()];
    let json_source = JsonDataSource::from_value(
        serde_json::json!({"Default": {"Flag": true, "Value": 7}}),
        &variants,
    )
    .expect("json data source should load");

    match json_source
        .retrieve_single_value("Value")
        .expect("json value should resolve")
    {
        DataValue::U64(value) => assert_eq!(value, 7),
        other => panic!("unexpected JSON value: {other:?}"),
    }

    let excel_source = ExcelDataSource::from_path(
        "tests/data/data.xlsx",
        ExcelDataSourceOptions::new(variants),
    )
    .expect("excel data source should load");

    assert!(excel_source.retrieve_single_value("TemperatureMax").is_ok());
}
