use std::path::PathBuf;

use mint_cli::commands;
use mint_cli::data;
use mint_cli::layout::args::{BlockNames, LayoutArgs};
use mint_cli::output::args::{OutputArgs, OutputFormat};

#[path = "common/mod.rs"]
mod common;

#[test]
fn export_json_uses_nested_block_data() {
    common::ensure_out_dir();

    let layout = r#"
[settings]
endianness = "little"

[config.header]
start_address = 0x1000
length = 0x40

[config.data]
device.id = { value = 0x1234, type = "u32" }
device.name = { name = "DeviceName", type = "u8", size = 8 }
flags = { type = "u8", bitmap = [
    { bits = 1, name = "EnableDebug" },
    { bits = 3, value = 0 },
    { bits = 4, name = "RegionCode" }
] }
coeffs = { name = "Coeffs", type = "u16", size = 3 }

[data.header]
start_address = 0x2000
length = 0x40

[data.data]
counter = { name = "Counter", type = "u32" }
message = { value = "Hi", type = "u8", size = 4 }
"#;

    let layout_path = common::write_layout_file("export_json_layout", layout);
    let layout_key = layout_path.clone();

    let data_args = data::args::DataArgs {
        json: Some(
            r#"{"Default":{"DeviceName":"UnitA","EnableDebug":1,"RegionCode":7,"Coeffs":[10,20,30],"Counter":99}}"#
                .to_string(),
        ),
        versions: Some("Default".to_string()),
        ..Default::default()
    };
    let ds = data::create_data_source(&data_args)
        .expect("datasource loads")
        .expect("datasource available");

    let args = mint_cli::args::Args {
        layout: LayoutArgs {
            blocks: vec![BlockNames {
                name: "".to_string(),
                file: layout_path,
            }],
            strict: false,
        },
        data: data_args,
        output: OutputArgs {
            out: PathBuf::from("out/export.hex"),
            record_width: 16,
            format: OutputFormat::Hex,
            export_json: Some(PathBuf::from("out/export.json")),
            stats: false,
            quiet: true,
        },
    };

    commands::build(&args, Some(ds.as_ref())).expect("build should succeed");

    let report = std::fs::read_to_string("out/export.json").expect("read json report");
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse json report");

    assert_eq!(
        json[&layout_key]["config"]["device"]["id"].as_u64(),
        Some(0x1234)
    );
    assert_eq!(
        json[&layout_key]["config"]["device"]["name"].as_str(),
        Some("UnitA")
    );
    assert_eq!(
        json[&layout_key]["config"]["flags"]["EnableDebug"].as_u64(),
        Some(1)
    );
    assert_eq!(
        json[&layout_key]["config"]["flags"]["RegionCode"].as_u64(),
        Some(7)
    );
    assert_eq!(
        json[&layout_key]["config"]["flags"]["reserved_1_3"].as_u64(),
        Some(0)
    );
    assert_eq!(json[&layout_key]["config"]["coeffs"][0].as_u64(), Some(10));
    assert_eq!(json[&layout_key]["data"]["counter"].as_u64(), Some(99));
    assert_eq!(json[&layout_key]["data"]["message"].as_str(), Some("Hi"));
}
