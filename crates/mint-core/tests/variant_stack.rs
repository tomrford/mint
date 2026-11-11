#[path = "common/mod.rs"]
mod common;

use mint_core::data::{ExcelDataSource, ExcelDataSourceOptions};

#[test]
fn excel_values_follow_variant_priority_and_skip_empty_cells() {
    let source = ExcelDataSource::from_path(
        "tests/data/data.xlsx",
        ExcelDataSourceOptions::new(vec!["VarA".into(), "Debug".into(), "Default".into()]),
    )
    .expect("datasource load");
    let path = common::write_layout_file(
        "excel-variants",
        r#"
[mint]
abi = "generic-le"

[block.header]
start_address = 0x1000
length = 0x40
padding = 0xFF

[block.data]
temperature = { name = "TemperatureMax", type = "u8" }
value2 = { name = "Value 2", type = "u8" }
flag = { name = "boolean", type = "u8" }
name = { name = "DeviceName", type = "u8", size = 9 }
"#,
    );

    let bytes = common::build_block(&path, "block", true, Some(&source)).expect("block builds");

    let mut expected = vec![
        55, // VarA wins over Debug (60) and Default (50)
        2,  // VarA cell is empty, so Debug (2) wins over Default (1)
        0,  // VarA false overrides Default true
    ];
    expected.extend(b"A Variant");
    assert_eq!(bytes, expected);
}
