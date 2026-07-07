//! Integration tests for JsonDataSource.

#[path = "common/mod.rs"]
mod common;

use mint_core::data::{DataSource, JsonDataSource};
use mint_core::layout::value::{DataValue, ValueSource};

fn build_json_source(variant: &str, json_data: &str) -> JsonDataSource {
    let variants = variant.split('/').map(str::to_owned).collect::<Vec<_>>();
    JsonDataSource::from_str(json_data, &variants).expect("datasource load")
}

#[test]
fn json_retrieve_single_value_priority_order() {
    let json_data = r#"{
        "Default": {
            "TemperatureMax": 50,
            "Value 2": 2,
            "boolean": true
        },
        "Debug": {
            "TemperatureMax": 60,
            "debugMode": true
        },
        "VarA": {
            "TemperatureMax": 55,
            "boolean": false
        }
    }"#;

    let ds = build_json_source("VarA/Debug/Default", json_data);

    // VarA has TemperatureMax=55, should take priority
    let value = ds.retrieve_single_value("TemperatureMax").unwrap();
    println!("TemperatureMax (VarA/Debug/Default): {:?}", value);
    assert!(matches!(value, DataValue::U64(55)));

    // VarA has boolean=false
    let value = ds.retrieve_single_value("boolean").unwrap();
    println!("boolean (VarA/Debug/Default): {:?}", value);
    assert!(matches!(value, DataValue::Bool(false)));

    // debugMode only in Debug
    let value = ds.retrieve_single_value("debugMode").unwrap();
    println!("debugMode (VarA/Debug/Default): {:?}", value);
    assert!(matches!(value, DataValue::Bool(true)));

    // "Value 2" only in Default
    let value = ds.retrieve_single_value("Value 2").unwrap();
    println!("Value 2 (VarA/Debug/Default): {:?}", value);
    assert!(matches!(value, DataValue::U64(2)));
}

#[test]
fn json_retrieve_single_value_fallback() {
    let json_data = r#"{
        "Default": {
            "TemperatureMax": 50,
            "boolean": true
        },
        "Debug": {
            "TemperatureMax": 60
        }
    }"#;

    let ds = build_json_source("Debug/Default", json_data);

    // Debug has TemperatureMax=60, should take priority over Default's 50
    let value = ds.retrieve_single_value("TemperatureMax").unwrap();
    println!("TemperatureMax (Debug/Default): {:?}", value);
    assert!(matches!(value, DataValue::U64(60)));

    // boolean not in Debug, falls back to Default's true
    let value = ds.retrieve_single_value("boolean").unwrap();
    println!("boolean (Debug/Default): {:?}", value);
    assert!(matches!(value, DataValue::Bool(true)));
}

#[test]
fn json_retrieve_missing_key_errors() {
    let json_data = r#"{
        "Default": {
            "TemperatureMax": 50
        }
    }"#;

    let ds = build_json_source("Default", json_data);

    let result = ds.retrieve_single_value("NonExistent");
    assert!(result.is_err());
    println!("Missing key error: {:?}", result.unwrap_err());
}

#[test]
fn json_retrieve_missing_variant_errors() {
    let json_data = r#"{
        "Default": {
            "TemperatureMax": 50
        }
    }"#;

    let variants = vec!["NonExistent".to_owned()];
    let result = JsonDataSource::from_str(json_data, &variants);
    assert!(result.is_err());
    if let Err(e) = result {
        println!("Missing variant error: {:?}", e);
    }
}

#[test]
fn json_retrieve_1d_literal_string() {
    let json_data = r#"{
        "Default": {
            "literalString": "hello world"
        }
    }"#;

    let ds = build_json_source("Default", json_data);

    let value = ds.retrieve_1d_array_or_string("literalString").unwrap();
    println!("literalString: {:?}", value);
    let ValueSource::Single(DataValue::Str(s)) = value else {
        panic!("expected single string");
    };
    assert_eq!(s, "hello world");
}
#[test]
fn json_retrieve_1d_native_json_array() {
    let json_data = r#"{
        "Default": {
            "nativeArray1d": [10, 20, 30]
        }
    }"#;

    let ds = build_json_source("Default", json_data);

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
fn json_retrieve_2d_native_json_array() {
    let json_data = r#"{
        "Default": {
            "nativeArray2d": [[1, 2], [3, 4], [5, 6]]
        }
    }"#;

    let ds = build_json_source("Default", json_data);

    let value = ds.retrieve_2d_array("nativeArray2d").unwrap();
    println!("nativeArray2d: {:?}", value);
    assert_eq!(value.len(), 3);
    assert_eq!(value[0].len(), 2);
    assert!(matches!(value[0][0], DataValue::U64(1)));
    assert!(matches!(value[0][1], DataValue::U64(2)));
    assert!(matches!(value[2][0], DataValue::U64(5)));
    assert!(matches!(value[2][1], DataValue::U64(6)));
}

#[test]
fn json_from_file() {
    use std::fs;

    let json_data = r#"{
        "Default": {
            "TemperatureMax": 50
        }
    }"#;

    let test_file = common::unique_out_path("mint_test_json", "json");
    fs::write(&test_file, json_data).expect("write test file");

    let variants = vec!["Default".to_owned()];
    let ds = JsonDataSource::from_path(&test_file, &variants).expect("datasource load");

    let value = ds.retrieve_single_value("TemperatureMax").unwrap();
    assert!(matches!(value, DataValue::U64(50)));

    fs::remove_file(test_file).ok();
}
