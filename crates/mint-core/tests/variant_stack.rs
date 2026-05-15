use mint_core::data::{DataSource, ExcelDataSource, ExcelDataSourceOptions};
use mint_core::layout::value::DataValue;

fn build_source(version: &str) -> ExcelDataSource {
    let versions = version
        .split('/')
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    ExcelDataSource::from_path(
        "tests/data/data.xlsx",
        ExcelDataSourceOptions::new(versions),
    )
    .expect("datasource load")
}

fn value_as_i64(value: DataValue) -> i64 {
    match value {
        DataValue::I64(v) => v,
        DataValue::U64(v) => v as i64,
        DataValue::F64(v) => v as i64,
        DataValue::Bool(v) => i64::from(v),
        DataValue::Str(s) => panic!("expected numeric value, got {}", s),
    }
}

#[test]
fn stacked_versions_respect_order() {
    let ds = build_source("VarA/Debug/Default");

    let value = ds
        .retrieve_single_value("TemperatureMax")
        .expect("value present");

    assert_eq!(value_as_i64(value), 55);
}

#[test]
fn stacked_versions_fall_back_when_empty() {
    let ds = build_source(" VarA / Debug / Default ");

    let value = ds.retrieve_single_value("Value 2").expect("value present");

    assert_eq!(value_as_i64(value), 2);
}

#[test]
fn boolean_cell_retrieves_default_true() {
    let ds = build_source("Default");

    let value = ds
        .retrieve_single_value("boolean")
        .expect("boolean present");

    assert!(matches!(value, DataValue::Bool(true)));
}

#[test]
fn boolean_cell_retrieves_debug_true() {
    let ds = build_source("Debug/Default");

    let value = ds
        .retrieve_single_value("boolean")
        .expect("boolean present");

    assert!(matches!(value, DataValue::Bool(true)));
}

#[test]
fn boolean_cell_retrieves_vara_false() {
    let ds = build_source("VarA/Default");

    let value = ds
        .retrieve_single_value("boolean")
        .expect("boolean present");

    assert!(matches!(value, DataValue::Bool(false)));
}
