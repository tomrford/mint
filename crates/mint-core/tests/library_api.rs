use mint_core::build::{self, BlockNames, BuildFromLayoutsRequest, BuildRequest, NamedLayout};
use mint_core::data::{DataSource, ExcelDataSource, ExcelDataSourceOptions, JsonDataSource};
use mint_core::layout;
use mint_core::layout::value::DataValue;
use mint_core::output::OutputFormat;

fn simple_block_selector(file: &str) -> BlockNames {
    BlockNames {
        name: "simple_block".to_owned(),
        file: file.to_owned(),
    }
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
            name: "memory-layout".to_owned(),
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
fn data_sources_can_be_constructed_without_cli_args() {
    let versions = vec!["Default".to_owned()];
    let json_source = JsonDataSource::from_value(
        serde_json::json!({"Default": {"Flag": true, "Value": 7}}),
        &versions,
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
        ExcelDataSourceOptions::new(versions),
    )
    .expect("excel data source should load");

    assert!(excel_source.retrieve_single_value("TemperatureMax").is_ok());
}
