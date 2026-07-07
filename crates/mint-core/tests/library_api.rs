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
endianness = "little"

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
        message.contains("overflow.toml#oversized")
            && message.contains("exceeds the 32-bit address space"),
        "unexpected error: {message}"
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
