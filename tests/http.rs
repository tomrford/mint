//! Integration tests for HTTP DataSource.
//!
//! Requires a running HTTP server. Skip with: cargo test --test http -- --ignored
//! Or run specifically: cargo test --test http -- --include-ignored
//!
//! Expected server: serves tests/data.json at http://localhost:3000/item?version=<name>

use mint_cli::data::args::DataArgs;
use mint_cli::data::create_data_source;
use mint_cli::layout::value::{DataValue, ValueSource};

const TEST_SERVER_URL: &str = "http://localhost:3000/item?version=$VERSION";

fn build_http_args(version: &str) -> DataArgs {
    let config = format!(
        r#"{{
            "url": "{}"
        }}"#,
        TEST_SERVER_URL
    );

    DataArgs {
        http: Some(config),
        versions: Some(version.to_string()),
        ..Default::default()
    }
}

#[test]
#[ignore = "requires running HTTP server"]
fn http_retrieve_single_value_priority_order() {
    let args = build_http_args("VarA/Debug/Default");
    let ds = create_data_source(&args)
        .expect("datasource load")
        .expect("datasource exists");

    // VarA has TemperatureMax=55, should take priority
    let value = ds.retrieve_single_value("TemperatureMax").unwrap();
    println!("TemperatureMax (VarA/Debug/Default): {:?}", value);
    assert!(matches!(value, DataValue::U64(55)));

    // VarA has enabled=false
    let value = ds.retrieve_single_value("enabled").unwrap();
    println!("enabled (VarA/Debug/Default): {:?}", value);
    assert!(matches!(value, DataValue::Bool(false)));

    // debugMode only in Debug
    let value = ds.retrieve_single_value("debugMode").unwrap();
    println!("debugMode (VarA/Debug/Default): {:?}", value);
    assert!(matches!(value, DataValue::Bool(true)));

    // Value2 only in Default
    let value = ds.retrieve_single_value("Value2").unwrap();
    println!("Value2 (VarA/Debug/Default): {:?}", value);
    assert!(matches!(value, DataValue::U64(2)));
}

#[test]
#[ignore = "requires running HTTP server"]
fn http_retrieve_single_value_fallback() {
    let args = build_http_args("Debug/Default");
    let ds = create_data_source(&args)
        .expect("datasource load")
        .expect("datasource exists");

    // Debug has TemperatureMax=60, should take priority over Default's 50
    let value = ds.retrieve_single_value("TemperatureMax").unwrap();
    println!("TemperatureMax (Debug/Default): {:?}", value);
    assert!(matches!(value, DataValue::U64(60)));

    // enabled not in Debug, falls back to Default's true
    let value = ds.retrieve_single_value("enabled").unwrap();
    println!("enabled (Debug/Default): {:?}", value);
    assert!(matches!(value, DataValue::Bool(true)));
}

#[test]
#[ignore = "requires running HTTP server"]
fn http_retrieve_missing_key_errors() {
    let args = build_http_args("Default");
    let ds = create_data_source(&args)
        .expect("datasource load")
        .expect("datasource exists");

    let result = ds.retrieve_single_value("NonExistent");
    assert!(result.is_err());
    println!("Missing key error: {:?}", result.unwrap_err());
}

#[test]
#[ignore = "requires running HTTP server"]
fn http_retrieve_1d_array_space_delimited() {
    let args = build_http_args("Default");
    let ds = create_data_source(&args).unwrap().unwrap();

    let value = ds.retrieve_1d_array_or_string("arraySpaces").unwrap();
    println!("arraySpaces: {:?}", value);
    let ValueSource::Array(arr) = value else {
        panic!("expected array");
    };
    assert_eq!(arr.len(), 4);
    assert!(matches!(arr[0], DataValue::U64(0)));
    assert!(matches!(arr[3], DataValue::U64(300)));
}

#[test]
#[ignore = "requires running HTTP server"]
fn http_retrieve_1d_array_comma_delimited() {
    let args = build_http_args("Default");
    let ds = create_data_source(&args).unwrap().unwrap();

    let value = ds.retrieve_1d_array_or_string("arrayCommas").unwrap();
    println!("arrayCommas: {:?}", value);
    let ValueSource::Array(arr) = value else {
        panic!("expected array");
    };
    assert_eq!(arr.len(), 4);
}

#[test]
#[ignore = "requires running HTTP server"]
fn http_retrieve_1d_array_mixed_delimiters() {
    let args = build_http_args("Default");
    let ds = create_data_source(&args).unwrap().unwrap();

    let value = ds.retrieve_1d_array_or_string("arrayMixed").unwrap();
    println!("arrayMixed: {:?}", value);
    let ValueSource::Array(arr) = value else {
        panic!("expected array");
    };
    assert_eq!(arr.len(), 4);
}

#[test]
#[ignore = "requires running HTTP server"]
fn http_retrieve_1d_array_single_value() {
    let args = build_http_args("Default");
    let ds = create_data_source(&args).unwrap().unwrap();

    let value = ds.retrieve_1d_array_or_string("arraySingle").unwrap();
    println!("arraySingle: {:?}", value);
    let ValueSource::Array(arr) = value else {
        panic!("expected array");
    };
    assert_eq!(arr.len(), 1);
    assert!(matches!(arr[0], DataValue::U64(42)));
}

#[test]
#[ignore = "requires running HTTP server"]
fn http_retrieve_1d_array_floats() {
    let args = build_http_args("Default");
    let ds = create_data_source(&args).unwrap().unwrap();

    let value = ds.retrieve_1d_array_or_string("arrayFloats").unwrap();
    println!("arrayFloats: {:?}", value);
    let ValueSource::Array(arr) = value else {
        panic!("expected array");
    };
    assert_eq!(arr.len(), 3);
    assert!(matches!(arr[0], DataValue::F64(f) if (f - 1.5).abs() < 0.001));
}

#[test]
#[ignore = "requires running HTTP server"]
fn http_retrieve_1d_array_negative() {
    let args = build_http_args("Default");
    let ds = create_data_source(&args).unwrap().unwrap();

    let value = ds.retrieve_1d_array_or_string("arrayNegative").unwrap();
    println!("arrayNegative: {:?}", value);
    let ValueSource::Array(arr) = value else {
        panic!("expected array");
    };
    assert_eq!(arr.len(), 3);
    assert!(matches!(arr[0], DataValue::I64(-1)));
}

#[test]
#[ignore = "requires running HTTP server"]
fn http_retrieve_1d_literal_string() {
    let args = build_http_args("Default");
    let ds = create_data_source(&args).unwrap().unwrap();

    let value = ds.retrieve_1d_array_or_string("literalString").unwrap();
    println!("literalString: {:?}", value);
    let ValueSource::Single(DataValue::Str(s)) = value else {
        panic!("expected single string");
    };
    assert_eq!(s, "hello world");
}

#[test]
#[ignore = "requires running HTTP server"]
fn http_retrieve_1d_native_json_array() {
    let args = build_http_args("Default");
    let ds = create_data_source(&args).unwrap().unwrap();

    let value = ds.retrieve_1d_array_or_string("nativeArray1d").unwrap();
    println!("nativeArray1d: {:?}", value);
    let ValueSource::Array(arr) = value else {
        panic!("expected array");
    };
    assert_eq!(arr.len(), 3);
    assert!(matches!(arr[0], DataValue::U64(10)));
    assert!(matches!(arr[1], DataValue::U64(20)));
    assert!(matches!(arr[2], DataValue::U64(30)));
}

#[test]
#[ignore = "requires running HTTP server"]
fn http_retrieve_2d_native_json_array() {
    let args = build_http_args("Default");
    let ds = create_data_source(&args).unwrap().unwrap();

    let value = ds.retrieve_2d_array("nativeArray2d").unwrap();
    println!("nativeArray2d: {:?}", value);
    assert_eq!(value.len(), 3);
    assert_eq!(value[0].len(), 2);
    assert!(matches!(value[0][0], DataValue::U64(1)));
    assert!(matches!(value[0][1], DataValue::U64(2)));
    assert!(matches!(value[2][0], DataValue::U64(5)));
    assert!(matches!(value[2][1], DataValue::U64(6)));
}
