use mint_core::build::{self, BlockSelector, BuildArtifact, BuildFromLayoutsRequest, NamedLayout};
use mint_core::data::{DataSource, ExcelDataSource, ExcelDataSourceOptions};
use mint_core::layout;
use mint_core::output::{DataRange, OutputFormat};
use std::path::PathBuf;

#[path = "common/mod.rs"]
mod common;

#[test]
fn output_rejects_invalid_target_address_units() {
    let range = |address_unit_bits, bytestream: Vec<u8>| DataRange {
        start_address: 0x100,
        address_unit_bits,
        reserved_size: bytestream.len() as u32,
        bytestream,
        allocated_size: 4,
    };
    for (ranges, expected) in [
        (
            vec![range(8, vec![0, 0]), range(16, vec![0, 0])],
            "cannot mix",
        ),
        (vec![range(16, vec![0xff])], "bytestream length 1 octets"),
    ] {
        let artifact = BuildArtifact {
            ranges,
            stats: Default::default(),
            used_values: None,
        };
        let error = artifact
            .render(OutputFormat::Hex, 16)
            .expect_err("invalid target address units should fail");
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn allocated_ranges_reject_overlap_but_allow_adjacency() {
    for (abi, first_length, second_start, overlaps) in [
        ("generic-le", 16, 0x1010, false),
        ("generic-le", 17, 0x1010, true),
        ("ti-c28x-eabi", 4, 0x1002, false),
        ("ti-c28x-eabi", 4, 0x1001, true),
    ] {
        let config = layout::parse_toml_layout(&format!(
            r#"
[mint]
abi = "{abi}"
[first.header]
start_address = 0x1000
length = {first_length}
[first.data]
value = {{ value = 1, type = "u16" }}
[second.header]
start_address = {second_start}
length = 2
[second.data]
value = {{ value = 2, type = "u16" }}
"#
        ))
        .expect("layout parses");
        let result = build::build_from_layouts(BuildFromLayoutsRequest {
            layouts: vec![NamedLayout {
                name: PathBuf::from("ranges.toml"),
                config,
            }],
            blocks: vec![BlockSelector::all("ranges.toml")],
            data_source: None,
            strict: false,
            capture_values: false,
        });
        if overlaps {
            let error = result.expect_err("allocated ranges overlap");
            assert!(common::error_chain(&error).contains("overlaps"), "{error}");
        } else {
            let artifact = result.expect("adjacent allocated ranges are valid");
            assert_eq!(artifact.ranges.len(), 2);
            let output = artifact.render(OutputFormat::Hex, 16).expect("HEX renders");
            assert!(output.ends_with(":00000001FF"), "{output}");
        }
    }
}

#[test]
fn prefix_checksums_and_capture_follow_field_order() {
    for (abi, expected, second_crc, expected_hex) in [
        (
            "generic-le",
            [
                1, 2, 3, 0xEE, 0xAB, 0xF0, 0xE3, 0xF6, 4, 0xEE, 0xEE, 0xEE, 0x89, 0xA4, 0x73, 0xCE,
            ],
            0xCE73_A489,
            ":10000000010203EEABF0E3F604EEEEEE89A473CE4C\n:00000001FF",
        ),
        (
            "generic-be",
            [
                1, 2, 3, 0xEE, 0xF6, 0xE3, 0xF0, 0xAB, 4, 0xEE, 0xEE, 0xEE, 0x13, 0xA7, 0xBF, 0x82,
            ],
            0x13A7_BF82,
            ":10000000010203EEF6E3F0AB04EEEEEE13A7BF82BF\n:00000001FF",
        ),
    ] {
        let source = format!(
            r#"
[mint]
abi = "{abi}"
[mint.checksum.crc32]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true
[block.header]
start_address = 0
length = 16
padding = 0xEE
[block.data]
first = {{ value = [1, 2, 3], type = "u8", size = 3 }}
checksum_one = {{ checksum = "crc32", type = "u32" }}
after_checksum = {{ value = 4, type = "u8" }}
checksum_two = {{ checksum = "crc32", type = "u32" }}
"#
        );
        for capture in [false, true] {
            let artifact = build::build_from_layouts(BuildFromLayoutsRequest {
                layouts: vec![NamedLayout {
                    name: PathBuf::from("checksums.toml"),
                    config: layout::parse_toml_layout(&source).expect("layout parses"),
                }],
                blocks: vec![BlockSelector::named("checksums.toml", "block")],
                data_source: None,
                strict: false,
                capture_values: capture,
            })
            .expect("block builds");
            assert_eq!(artifact.ranges[0].bytestream, expected);
            assert_eq!(
                artifact.stats.block_stats[0].checksum_values,
                [0xF6E3_F0AB, second_crc]
            );
            assert_eq!(artifact.used_values.is_some(), capture);
            if let Some(report) = &artifact.used_values {
                let values = &report["checksums.toml"]["block"];
                assert_eq!(
                    values
                        .as_object()
                        .unwrap()
                        .keys()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                    ["first", "checksum_one", "after_checksum", "checksum_two"]
                );
                assert_eq!(values["checksum_two"], second_crc);
            }
            let hex = artifact.render(OutputFormat::Hex, 16).expect("HEX renders");
            assert_eq!(hex, expected_hex);
            std::fs::write(common::unique_out_path(abi, "hex"), hex).expect("write HEX artifact");
            if let Some(report) = artifact.used_values {
                std::fs::write(
                    common::unique_out_path(abi, "json"),
                    serde_json::to_vec_pretty(&report).unwrap(),
                )
                .expect("write captured-values artifact");
            }
        }
    }
}

#[test]
fn excel_errors_list_available_sheets_and_columns() {
    let error = |options: ExcelDataSourceOptions| match ExcelDataSource::from_path(
        "tests/data/data.xlsx",
        options,
    ) {
        Err(error) => common::error_chain(&error),
        Ok(_) => panic!("invalid Excel options should fail"),
    };

    let message = error(ExcelDataSourceOptions {
        main_sheet: "Config".to_owned(),
        variants: vec!["Default".to_owned()],
    });
    assert!(
        message.contains("main sheet 'Config' not found"),
        "{message}"
    );
    assert!(message.contains("Available sheets: Main"), "{message}");

    let message = error(ExcelDataSourceOptions::new(vec!["Typo".to_owned()]));
    assert!(
        message.contains("Excel column not found: 'Typo' in sheet 'Main'"),
        "{message}"
    );
    assert!(
        message.contains("Available columns: Name, Default, Debug, VarA"),
        "{message}"
    );
}

#[test]
fn excel_rejects_ambiguous_names_and_incomplete_matrices() {
    for (fixture, expected) in [
        ("duplicate-names", "duplicate name 'CrcSeed'"),
        ("duplicate-variants", "duplicate variant 'Debug'"),
        (
            "blank-matrix-cell",
            "Empty cell in 2D array at row 3, column 2",
        ),
    ] {
        let result = ExcelDataSource::from_path(
            format!("tests/data/{fixture}.xlsx"),
            ExcelDataSourceOptions::new(vec!["Debug".to_owned()]),
        );
        let error = match result {
            Err(error) => error,
            Ok(source) => source
                .retrieve_2d_array("Matrix")
                .expect_err("matrix with a blank interior cell is invalid"),
        };
        let message = common::error_chain(&error);
        assert!(message.contains(expected), "{fixture}: {message}");
    }
}

#[test]
fn excel_loads_array_sheets_lazily_and_surfaces_sheet_read_errors() {
    let source = ExcelDataSource::from_path(
        "tests/data/lazy-sheets.xlsx",
        ExcelDataSourceOptions::new(vec!["Default".to_owned()]),
    )
    .expect("unreferenced broken sheet should not block opening Main");

    let config = layout::parse_toml_layout(
        r#"
[mint]
abi = "generic-le"
[block.header]
start_address = 0x1000
length = 8
padding = 0x00
[block.data]
scalar = { name = "ScalarVal", type = "u16" }
first = { name = "GoodArr", type = "u8", size = 2 }
second = { name = "GoodArr", type = "u8", size = 2 }
"#,
    )
    .expect("layout parses");
    let artifact = build::build_from_layouts(BuildFromLayoutsRequest {
        layouts: vec![NamedLayout {
            name: PathBuf::from("lazy.toml"),
            config,
        }],
        blocks: vec![BlockSelector::all("lazy.toml")],
        data_source: Some(&source),
        strict: true,
        capture_values: false,
    })
    .expect("building only referenced sheets should succeed");
    assert_eq!(artifact.ranges[0].bytestream, [42, 0, 10, 20, 10, 20]);
    let hex = artifact.render(OutputFormat::Hex, 16).expect("HEX renders");
    assert_eq!(hex, ":061000002A000A140A1484\n:00000001FF");

    let bad_array = source
        .retrieve_1d_array_or_string("BadArr")
        .expect_err("corrupt referenced array sheet should surface its parse error");
    let message = common::error_chain(&bad_array);
    assert!(
        message.contains("failed to read sheet 'Broken'"),
        "expected sheet read error, got: {message}"
    );

    let main_ref = source
        .retrieve_1d_array_or_string("MainRef")
        .expect_err("main sheet should not be usable as an array sheet");
    let message = common::error_chain(&main_ref);
    assert!(
        message.contains("Sheet not found: 'Main'. Available sheets: Broken, Good"),
        "{message}"
    );

    let bad_main = match ExcelDataSource::from_path(
        "tests/data/lazy-sheets.xlsx",
        ExcelDataSourceOptions {
            main_sheet: "Broken".to_owned(),
            variants: vec!["Default".to_owned()],
        },
    ) {
        Err(error) => common::error_chain(&error),
        Ok(_) => panic!("corrupt main sheet should fail"),
    };
    assert!(
        bad_main.contains("failed to read sheet 'Broken'"),
        "expected main sheet read error, got: {bad_main}"
    );
}
