use mint_cli::data::args::DataArgs;
use mint_cli::data::create_data_source;
use mint_cli::layout::value::DataValue;

fn build_args(version: &str) -> DataArgs {
    DataArgs {
        xlsx: Some("tests/data/data.xlsx".to_string()),
        versions: Some(version.to_string()),
        ..Default::default()
    }
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
    let args = build_args("VarA/Debug/Default");
    let ds = create_data_source(&args)
        .expect("datasource load")
        .expect("datasource exists");

    let value = ds
        .retrieve_single_value("TemperatureMax")
        .expect("value present");

    assert_eq!(value_as_i64(value), 55);
}

#[test]
fn stacked_versions_fall_back_when_empty() {
    let args = build_args(" VarA / Debug / Default ");
    let ds = create_data_source(&args)
        .expect("datasource load")
        .expect("datasource exists");

    let value = ds.retrieve_single_value("Value 2").expect("value present");

    assert_eq!(value_as_i64(value), 2);
}

#[test]
fn boolean_cell_retrieves_default_true() {
    let args = build_args("Default");
    let ds = create_data_source(&args)
        .expect("datasource load")
        .expect("datasource exists");

    let value = ds
        .retrieve_single_value("boolean")
        .expect("boolean present");

    assert!(matches!(value, DataValue::Bool(true)));
}

#[test]
fn boolean_cell_retrieves_debug_true() {
    let args = build_args("Debug/Default");
    let ds = create_data_source(&args)
        .expect("datasource load")
        .expect("datasource exists");

    let value = ds
        .retrieve_single_value("boolean")
        .expect("boolean present");

    assert!(matches!(value, DataValue::Bool(true)));
}

#[test]
fn boolean_cell_retrieves_vara_false() {
    let args = build_args("VarA/Default");
    let ds = create_data_source(&args)
        .expect("datasource load")
        .expect("datasource exists");

    let value = ds
        .retrieve_single_value("boolean")
        .expect("boolean present");

    assert!(matches!(value, DataValue::Bool(false)));
}
