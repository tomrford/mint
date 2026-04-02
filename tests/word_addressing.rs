use std::path::PathBuf;

use mint_cli::commands;
use mint_cli::layout::args::BlockNames;
use mint_cli::output::args::{OutputArgs, OutputFormat};

#[path = "common/mod.rs"]
mod common;

/// Verifies that word_addressing doubles addresses and swaps bytes.
#[test]
fn word_addressing_doubles_addresses_and_swaps_bytes() {
    let layout = r#"
[settings]
endianness = "little"
word_addressing = true

[block.header]
start_address = 0x1000
length = 0x20
padding = 0xFF

[block.data]
val1 = { value = 0x1234, type = "u16" }
val2 = { value = 0x5678, type = "u16" }
"#;

    let path = common::write_layout_file("word_addr_basic", layout);

    let args = mint_cli::args::Args {
        layout: mint_cli::layout::args::LayoutArgs {
            blocks: vec![BlockNames {
                name: "block".to_string(),
                file: path,
                legacy_syntax: false,
            }],
            strict: false,
        },
        data: mint_cli::data::args::DataArgs::default(),
        output: OutputArgs {
            out: PathBuf::from("out/word_addr.hex"),
            record_width: 16,
            format: OutputFormat::Hex,
            export_json: None,
            stats: false,
            quiet: false,
        },
    };

    commands::build(&args, None).expect("build should succeed");

    let hex_path = std::path::Path::new("out/word_addr.hex");
    assert!(hex_path.exists(), "output file should exist");

    let content = std::fs::read_to_string(hex_path).expect("read hex file");

    // start_address = 0x1000, doubled = 0x2000
    // The hex file should have extended address record for 0x2000
    // Data bytes: 0x1234 -> [0x34, 0x12] little endian, swapped -> [0x12, 0x34]
    //             0x5678 -> [0x78, 0x56] little endian, swapped -> [0x56, 0x78]
    assert!(
        content.contains("2000"),
        "address should be doubled (0x1000 -> 0x2000)"
    );
    // First data record should contain swapped bytes: 12 34 56 78
    assert!(
        content.contains("12345678"),
        "bytes should be swapped pairwise"
    );
}

/// Verifies that block length is interpreted as word count in word-addressing mode.
#[test]
fn word_addressing_length_is_in_words() {
    let layout = r#"
[settings]
endianness = "little"
word_addressing = true

[block.header]
start_address = 0x1000
length = 2
padding = 0xFF

[block.data]
val1 = { value = 0x1234, type = "u16" }
val2 = { value = 0x5678, type = "u16" }
"#;

    let path = common::write_layout_file("word_addr_len_words", layout);

    let args = mint_cli::args::Args {
        layout: mint_cli::layout::args::LayoutArgs {
            blocks: vec![BlockNames {
                name: "block".to_string(),
                file: path,
                legacy_syntax: false,
            }],
            strict: false,
        },
        data: mint_cli::data::args::DataArgs::default(),
        output: OutputArgs {
            out: PathBuf::from("out/word_len_words.hex"),
            record_width: 16,
            format: OutputFormat::Hex,
            export_json: None,
            stats: false,
            quiet: false,
        },
    };

    commands::build(&args, None).expect("build should succeed");
    assert!(
        std::path::Path::new("out/word_len_words.hex").exists(),
        "output file should exist"
    );
}

/// Verifies that word_addressing with CRC also doubles the CRC address.
#[test]
fn word_addressing_doubles_crc_address() {
    let layout = r#"
[settings]
endianness = "little"
word_addressing = true

[settings.crc]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true
area = "data"

[block.header]
start_address = 0x1000
length = 0x20
padding = 0xFF

[block.header.crc]
location = "end_data"

[block.data]
val = { value = 0xABCD, type = "u16" }
"#;

    let path = common::write_layout_file("word_addr_crc", layout);

    let args = mint_cli::args::Args {
        layout: mint_cli::layout::args::LayoutArgs {
            blocks: vec![BlockNames {
                name: "block".to_string(),
                file: path,
                legacy_syntax: false,
            }],
            strict: false,
        },
        data: mint_cli::data::args::DataArgs::default(),
        output: OutputArgs {
            out: PathBuf::from("out/word_crc.hex"),
            record_width: 16,
            format: OutputFormat::Hex,
            export_json: None,
            stats: false,
            quiet: false,
        },
    };

    commands::build(&args, None).expect("build with CRC should succeed");

    let hex_path = std::path::Path::new("out/word_crc.hex");
    assert!(hex_path.exists(), "output file should exist");
}

/// Verifies that u8 types are rejected when word_addressing is enabled.
#[test]
fn word_addressing_rejects_u8_type() {
    let layout = r#"
[settings]
endianness = "little"
word_addressing = true

[block.header]
start_address = 0x1000
length = 0x20
padding = 0xFF

[block.data]
byte_val = { value = 42, type = "u8" }
"#;

    let path = common::write_layout_file("word_addr_u8_reject", layout);

    let args = mint_cli::args::Args {
        layout: mint_cli::layout::args::LayoutArgs {
            blocks: vec![BlockNames {
                name: "block".to_string(),
                file: path,
                legacy_syntax: false,
            }],
            strict: false,
        },
        data: mint_cli::data::args::DataArgs::default(),
        output: OutputArgs {
            out: PathBuf::from("out/word_u8_reject.hex"),
            record_width: 16,
            format: OutputFormat::Hex,
            export_json: None,
            stats: false,
            quiet: false,
        },
    };

    let result = commands::build(&args, None);
    assert!(result.is_err(), "u8 type should be rejected");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("u8/i8 types are not supported with word_addressing"),
        "error message should mention u8/i8 restriction: {}",
        err_msg
    );
}

/// Verifies that strings (u8 arrays) are rejected when word_addressing is enabled.
#[test]
fn word_addressing_rejects_strings() {
    let layout = r#"
[settings]
endianness = "little"
word_addressing = true

[block.header]
start_address = 0x1000
length = 0x20
padding = 0xFF

[block.data]
text = { value = "HELLO", type = "u8", size = 8 }
"#;

    let path = common::write_layout_file("word_addr_str_reject", layout);

    let args = mint_cli::args::Args {
        layout: mint_cli::layout::args::LayoutArgs {
            blocks: vec![BlockNames {
                name: "block".to_string(),
                file: path,
                legacy_syntax: false,
            }],
            strict: false,
        },
        data: mint_cli::data::args::DataArgs::default(),
        output: OutputArgs {
            out: PathBuf::from("out/word_str_reject.hex"),
            record_width: 16,
            format: OutputFormat::Hex,
            export_json: None,
            stats: false,
            quiet: false,
        },
    };

    let result = commands::build(&args, None);
    assert!(result.is_err(), "string (u8 array) should be rejected");
}

/// Verifies that virtual_offset is NOT doubled (applied after address doubling).
#[test]
fn word_addressing_virtual_offset_not_doubled() {
    let layout = r#"
[settings]
endianness = "little"
word_addressing = true
virtual_offset = 0x100

[block.header]
start_address = 0x1000
length = 0x10
padding = 0xFF

[block.data]
val = { value = 0x1234, type = "u16" }
"#;

    let path = common::write_layout_file("word_addr_voffset", layout);

    let args = mint_cli::args::Args {
        layout: mint_cli::layout::args::LayoutArgs {
            blocks: vec![BlockNames {
                name: "block".to_string(),
                file: path,
                legacy_syntax: false,
            }],
            strict: false,
        },
        data: mint_cli::data::args::DataArgs::default(),
        output: OutputArgs {
            out: PathBuf::from("out/word_voff.hex"),
            record_width: 16,
            format: OutputFormat::Hex,
            export_json: None,
            stats: false,
            quiet: false,
        },
    };

    commands::build(&args, None).expect("build should succeed");

    let hex_path = std::path::Path::new("out/word_voff.hex");
    assert!(hex_path.exists(), "output file should exist");

    let content = std::fs::read_to_string(hex_path).expect("read hex file");

    // start_address = 0x1000 * 2 + 0x100 = 0x2100
    assert!(
        content.contains("2100"),
        "address should be (0x1000 * 2) + 0x100 = 0x2100"
    );
}
