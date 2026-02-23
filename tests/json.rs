//! Integration tests for JsonDataSource.

use mint_cli::data::args::DataArgs;
use mint_cli::data::create_data_source;
use mint_cli::layout::value::{DataValue, ValueSource};

fn build_json_args(version: &str, json_data: &str) -> DataArgs {
    DataArgs {
        json: Some(json_data.to_string()),
        versions: Some(version.to_string()),
        ..Default::default()
    }
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

    let args = build_json_args("VarA/Debug/Default", json_data);
    let ds = create_data_source(&args)
        .expect("datasource load")
        .expect("datasource exists");

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

    let args = build_json_args("Debug/Default", json_data);
    let ds = create_data_source(&args)
        .expect("datasource load")
        .expect("datasource exists");

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

    let args = build_json_args("Default", json_data);
    let ds = create_data_source(&args)
        .expect("datasource load")
        .expect("datasource exists");

    let result = ds.retrieve_single_value("NonExistent");
    assert!(result.is_err());
    println!("Missing key error: {:?}", result.unwrap_err());
}

#[test]
fn json_retrieve_missing_version_errors() {
    let json_data = r#"{
        "Default": {
            "TemperatureMax": 50
        }
    }"#;

    let args = build_json_args("NonExistent", json_data);
    let result = create_data_source(&args);
    assert!(result.is_err());
    if let Err(e) = result {
        println!("Missing version error: {:?}", e);
    }
}

#[test]
fn json_retrieve_1d_array_space_delimited() {
    let json_data = r#"{
        "Default": {
            "arraySpaces": "0 100 200 300"
        }
    }"#;

    let args = build_json_args("Default", json_data);
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
fn json_retrieve_1d_array_comma_delimited() {
    let json_data = r#"{
        "Default": {
            "arrayCommas": "1,2,3,4"
        }
    }"#;

    let args = build_json_args("Default", json_data);
    let ds = create_data_source(&args).unwrap().unwrap();

    let value = ds.retrieve_1d_array_or_string("arrayCommas").unwrap();
    println!("arrayCommas: {:?}", value);
    let ValueSource::Array(arr) = value else {
        panic!("expected array");
    };
    assert_eq!(arr.len(), 4);
}

#[test]
fn json_retrieve_1d_array_mixed_delimiters() {
    let json_data = r#"{
        "Default": {
            "arrayMixed": "5, 10; 15 20"
        }
    }"#;

    let args = build_json_args("Default", json_data);
    let ds = create_data_source(&args).unwrap().unwrap();

    let value = ds.retrieve_1d_array_or_string("arrayMixed").unwrap();
    println!("arrayMixed: {:?}", value);
    let ValueSource::Array(arr) = value else {
        panic!("expected array");
    };
    assert_eq!(arr.len(), 4);
}

#[test]
fn json_retrieve_1d_array_single_value() {
    let json_data = r#"{
        "Default": {
            "arraySingle": "42"
        }
    }"#;

    let args = build_json_args("Default", json_data);
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
fn json_retrieve_1d_array_floats() {
    let json_data = r#"{
        "Default": {
            "arrayFloats": "1.5 2.5 3.5"
        }
    }"#;

    let args = build_json_args("Default", json_data);
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
fn json_retrieve_1d_array_negative() {
    let json_data = r#"{
        "Default": {
            "arrayNegative": "-1 -2 -3"
        }
    }"#;

    let args = build_json_args("Default", json_data);
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
fn json_retrieve_1d_literal_string() {
    let json_data = r#"{
        "Default": {
            "literalString": "hello world"
        }
    }"#;

    let args = build_json_args("Default", json_data);
    let ds = create_data_source(&args).unwrap().unwrap();

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

    let args = build_json_args("Default", json_data);
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
fn json_retrieve_2d_native_json_array() {
    let json_data = r#"{
        "Default": {
            "nativeArray2d": [[1, 2], [3, 4], [5, 6]]
        }
    }"#;

    let args = build_json_args("Default", json_data);
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

#[test]
fn json_from_file() {
    use std::fs;
    use std::path::Path;

    let json_data = r#"{
        "Default": {
            "TemperatureMax": 50
        }
    }"#;

    let test_file = Path::new("/tmp/mint_test_json.json");
    fs::write(test_file, json_data).expect("write test file");

    let args = DataArgs {
        json: Some(test_file.to_str().unwrap().to_string()),
        versions: Some("Default".to_string()),
        ..Default::default()
    };

    let ds = create_data_source(&args)
        .expect("datasource load")
        .expect("datasource exists");

    let value = ds.retrieve_single_value("TemperatureMax").unwrap();
    assert!(matches!(value, DataValue::U64(50)));

    fs::remove_file(test_file).ok();
}
