#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use mint_cli::args::Args;
use mint_cli::data::{self, DataSource};
use mint_cli::layout::args::{BlockNames, LayoutArgs};
use mint_cli::layout::used_values::{NoopValueSink, ValueCollector};
use mint_cli::output::args::{OutputArgs, OutputFormat};

static UNIQUE_FILE_ID: AtomicU64 = AtomicU64::new(0);

pub fn ensure_out_dir() {
    fs::create_dir_all("out").unwrap();
}

pub fn write_layout_file(file_stem: &str, contents: &str) -> String {
    ensure_out_dir();
    let unique_id = UNIQUE_FILE_ID.fetch_add(1, Ordering::Relaxed);
    let path = format!(
        "out/{}-{}-{}.toml",
        file_stem,
        std::process::id(),
        unique_id
    );
    std::fs::write(&path, contents).expect("write layout file");
    path
}

/// Generate a unique output path under `out/`.
pub fn unique_out_path(stem: &str, ext: &str) -> PathBuf {
    ensure_out_dir();
    let unique_id = UNIQUE_FILE_ID.fetch_add(1, Ordering::Relaxed);
    PathBuf::from(format!(
        "out/{}-{}-{}.{}",
        stem,
        std::process::id(),
        unique_id,
        ext
    ))
}

/// Build test args with a unique output path.
pub fn build_args(layout_path: &str, block_name: &str, format: OutputFormat) -> Args {
    let ext = match format {
        OutputFormat::Hex => "hex",
        OutputFormat::Mot => "mot",
    };
    Args {
        layout: LayoutArgs {
            blocks: vec![BlockNames {
                name: block_name.to_owned(),
                file: layout_path.to_owned(),
            }],
            strict: false,
        },
        data: data::args::DataArgs {
            xlsx: Some("tests/data/data.xlsx".to_owned()),
            versions: Some("Default".to_owned()),
            ..Default::default()
        },
        output: OutputArgs {
            out: unique_out_path(block_name, ext),
            record_width: 32,
            format,
            export_json: None,
            stats: false,
            quiet: false,
        },
    }
}

pub fn find_working_datasource() -> Option<Box<dyn DataSource>> {
    let version_candidates: [&str; 2] = ["Default", "VarA/Default"];

    for ver in &version_candidates {
        let ver_args = data::args::DataArgs {
            xlsx: Some("tests/data/data.xlsx".to_owned()),
            versions: Some((*ver).to_owned()),
            ..Default::default()
        };
        if let Ok(Some(ds)) = data::create_data_source(&ver_args) {
            return Some(ds);
        }
    }
    None
}

/// Assert that the output file exists at the given path
pub fn assert_out_file_exists(out_path: &Path) {
    assert!(
        out_path.exists(),
        "expected output file to exist: {}",
        out_path.display()
    );
}

/// Build a block's bytestream, returning `(bytes, padding_count)`.
pub fn build_block(
    block: &mint_cli::layout::block::Block,
    settings: &mint_cli::layout::settings::MintConfig,
    strict: bool,
    data_source: Option<&dyn DataSource>,
) -> Result<(Vec<u8>, u32), mint_cli::layout::error::LayoutError> {
    let mut noop = NoopValueSink;
    let output = block.build_bytestream(data_source, settings, strict, &mut noop)?;
    Ok((output.bytestream, output.padding_count))
}

/// Build a block's bytestream and collect exported values.
pub fn build_block_with_values(
    block: &mint_cli::layout::block::Block,
    settings: &mint_cli::layout::settings::MintConfig,
) -> Result<((Vec<u8>, u32), serde_json::Value), mint_cli::layout::error::LayoutError> {
    let mut collector = ValueCollector::new();
    let output = block.build_bytestream(None, settings, false, &mut collector)?;
    Ok((
        (output.bytestream, output.padding_count),
        collector.into_value(),
    ))
}

/// Build test args for multiple layouts with a unique output path.
pub fn build_args_for_layouts(layouts: Vec<BlockNames>, format: OutputFormat, stem: &str) -> Args {
    let ext = match format {
        OutputFormat::Hex => "hex",
        OutputFormat::Mot => "mot",
    };
    Args {
        layout: LayoutArgs {
            blocks: layouts,
            strict: false,
        },
        data: data::args::DataArgs {
            xlsx: Some("tests/data/data.xlsx".to_owned()),
            versions: Some("Default".to_owned()),
            ..Default::default()
        },
        output: OutputArgs {
            out: unique_out_path(stem, ext),
            record_width: 32,
            format,
            export_json: None,
            stats: false,
            quiet: false,
        },
    }
}
