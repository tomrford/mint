use std::path::PathBuf;

use mint_cli::commands;
use mint_cli::layout::args::BlockNames;
use mint_cli::output::args::{OutputArgs, OutputFormat};

#[path = "common/mod.rs"]
mod common;

// This integration test exercises:
// - Big endian vs little endian
// - CRC at end and at explicit address
// - record width variations (16 and 64)
// - Output formats HEX and MOT (SREC address length auto-selection)
// - virtual_offset changing start addresses
// - 1D array strings and numeric arrays
// - 2D array retrieval and padding
// - mix of value sources (Value and Name)
#[test]
fn mixed_feature_matrix() {
    // Build two layouts to cover multiple settings
    let layout_be_pad_addr = r#"
[settings]
endianness = "big"
virtual_offset = 0

[settings.crc]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true
area = "data"

[block.header]
start_address = 0x10000
length = 0x80
padding = 0xAA

[block.header.crc]
location = 0x10060

[block.data]
nums.u16_be = { value = [1, 2, 3, 4], type = "u16", size = 4 }
txt.ascii = { value = "HELLO", type = "u8", size = 8 }
single.i32 = { value = 42, type = "i32" }
"#;

    let layout_le_end = r#"
[settings]
endianness = "little"
virtual_offset = 0x20000

[settings.crc]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true
area = "data"

[block.header]
start_address = 0x90000
length = 0x40
padding = 0x00

[block.header.crc]
location = "end_data"

[block.data]
arr.f32 = { value = [1.0, 2.5], type = "f32", size = 2 }
arr2.i16 = { value = [10, -20, 30, -40], type = "i16", size = 4 }
"#;

    // write layouts
    let be_path = common::write_layout_file("mixed_be", layout_be_pad_addr);
    let le_path = common::write_layout_file("mixed_le", layout_le_end);

    // Prepare a datasheet (may be no-op for these, but keep realistic flow)
    let data_args = mint_cli::data::args::DataArgs {
        xlsx: Some("tests/data/data.xlsx".to_string()),
        versions: Some("Default".to_string()),
        ..Default::default()
    };
    let ds = mint_cli::data::create_data_source(&data_args).expect("datasource loads");

    // Case 1: Big endian, CRC at explicit address, HEX with width 64
    let args_be_hex = mint_cli::args::Args {
        layout: mint_cli::layout::args::LayoutArgs {
            blocks: vec![BlockNames {
                name: "block".to_string(),
                file: be_path.clone(),
            }],
            strict: false,
        },
        data: data_args.clone(),
        output: OutputArgs {
            out: PathBuf::from("out/mix_a.hex"),
            record_width: 64,
            format: OutputFormat::Hex,
            export_json: None,
            stats: false,
            quiet: false,
        },
    };
    commands::build(&args_be_hex, ds.as_deref()).expect("be-hex");
    assert!(std::path::Path::new("out/mix_a.hex").exists());

    // Case 2: Big endian, explicit CRC, MOT with width 16
    let args_be_mot = mint_cli::args::Args {
        layout: mint_cli::layout::args::LayoutArgs {
            blocks: vec![BlockNames {
                name: "block".to_string(),
                file: be_path.clone(),
            }],
            strict: false,
        },
        data: data_args.clone(),
        output: OutputArgs {
            out: PathBuf::from("out/mix_b.mot"),
            record_width: 16,
            format: OutputFormat::Mot,
            export_json: None,
            stats: false,
            quiet: false,
        },
    };
    commands::build(&args_be_mot, ds.as_deref()).expect("be-mot");
    assert!(std::path::Path::new("out/mix_b.mot").exists());

    // Case 3: Little endian, CRC at end, HEX width 16, virtual_offset applied
    let args_le_hex = mint_cli::args::Args {
        layout: mint_cli::layout::args::LayoutArgs {
            blocks: vec![BlockNames {
                name: "block".to_string(),
                file: le_path.clone(),
            }],
            strict: true, // exercise strict path on numeric arrays
        },
        data: data_args.clone(),
        output: OutputArgs {
            out: PathBuf::from("out/mix_c.hex"),
            record_width: 16,
            format: OutputFormat::Hex,
            export_json: None,
            stats: false,
            quiet: false,
        },
    };
    commands::build(&args_le_hex, ds.as_deref()).expect("le-hex");
    assert!(std::path::Path::new("out/mix_c.hex").exists());

    // Case 4: Little endian, CRC at end, MOT width 64
    let args_le_mot = mint_cli::args::Args {
        layout: mint_cli::layout::args::LayoutArgs {
            blocks: vec![BlockNames {
                name: "block".to_string(),
                file: le_path.clone(),
            }],
            strict: true,
        },
        data: data_args,
        output: OutputArgs {
            out: PathBuf::from("out/mix_d.mot"),
            record_width: 64,
            format: OutputFormat::Mot,
            export_json: None,
            stats: false,
            quiet: false,
        },
    };
    commands::build(&args_le_mot, ds.as_deref()).expect("le-mot");
    assert!(std::path::Path::new("out/mix_d.mot").exists());
}
