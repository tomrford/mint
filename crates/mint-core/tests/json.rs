#[path = "common/mod.rs"]
mod common;

use mint_core::data::JsonDataSource;

fn variants(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn layout(data: &str) -> String {
    common::write_layout_file(
        "json",
        &format!(
            r#"
[mint]
abi = "generic-le"

[block.header]
start_address = 0x1000
length = 0x40
padding = 0xFF

[block.data]
{data}
"#
        ),
    )
}

#[test]
fn json_file_values_follow_variant_priority_and_skip_nulls() {
    let data = common::unique_out_path("json-variants", "json");
    std::fs::write(
        &data,
        r#"{
            "Default": {
                "TemperatureMax": 50,
                "Value 2": 2,
                "boolean": true,
                "Label": "dflt",
                "Samples": [1, 2, 3]
            },
            "Debug": {
                "TemperatureMax": 60,
                "Value 2": null,
                "debugMode": true,
                "Samples": [4, 5, 6]
            },
            "VarA": {
                "TemperatureMax": 55,
                "Value 2": null,
                "boolean": false,
                "Label": "varA"
            }
        }"#,
    )
    .expect("write JSON data");
    let source = JsonDataSource::from_path(&data, &variants(&["VarA", "Debug", "Default"]))
        .expect("JSON file loads");

    let path = layout(
        r#"
temperature = { name = "TemperatureMax", type = "u8" }
value2 = { name = "Value 2", type = "u8" }
flag = { name = "boolean", type = "u8" }
debug = { name = "debugMode", type = "u8" }
label = { name = "Label", type = "u8", size = 6 }
samples = { name = "Samples", type = "u8", size = 3 }
"#,
    );
    let bytes = common::build_block(&path, "block", true, Some(&source)).expect("block builds");

    assert_eq!(
        bytes,
        [
            55, // VarA wins over Debug and Default
            2,  // null in VarA and Debug falls through to Default
            0,  // VarA false overrides Default true
            1,  // only Debug defines it
            b'v', b'a', b'r', b'A', 0xFF, 0xFF, // VarA string, padded
            4, 5, 6, // VarA has none, Debug array wins over Default
        ]
    );
}

#[test]
fn json_rejects_unknown_variants_and_missing_keys() {
    let data = r#"{"Default": {"TemperatureMax": 50}}"#;

    let error = JsonDataSource::from_str(data, &variants(&["Default", "Release"]))
        .err()
        .expect("unknown variant is rejected");
    assert!(
        error
            .to_string()
            .contains("variant 'Release' not found in JSON data"),
        "{error}"
    );

    let source = JsonDataSource::from_str(data, &variants(&["Default"])).expect("JSON loads");
    let path = layout(r#"missing = { name = "TemperatureMin", type = "u8" }"#);
    let error = common::build_block(&path, "block", false, Some(&source))
        .expect_err("missing key is rejected");
    let message = common::error_chain(&error);
    assert!(
        message.contains("TemperatureMin") && message.contains("key not found in any variant"),
        "{message}"
    );
}
