#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use mint_cli::args::Args;
use mint_cli::data;
use mint_cli::layout_args::LayoutArgs;
use mint_cli::output_args::OutputArgs;
use mint_core::build::BlockSelector;
use mint_core::data::DataSource;
use mint_core::output::OutputFormat;

static UNIQUE_FILE_ID: AtomicU64 = AtomicU64::new(0);

fn test_out_dir() -> PathBuf {
    std::env::temp_dir()
        .join("mint-cli-tests")
        .join(std::process::id().to_string())
}

pub fn ensure_out_dir() {
    fs::create_dir_all(test_out_dir()).unwrap();
}

pub fn write_layout_file(file_stem: &str, contents: &str) -> String {
    ensure_out_dir();
    let unique_id = UNIQUE_FILE_ID.fetch_add(1, Ordering::Relaxed);
    let path = test_out_dir().join(format!("{file_stem}-{unique_id}.toml"));
    std::fs::write(&path, contents).expect("write layout file");
    path.to_string_lossy().into_owned()
}

pub fn unique_out_path(stem: &str, ext: &str) -> PathBuf {
    ensure_out_dir();
    let unique_id = UNIQUE_FILE_ID.fetch_add(1, Ordering::Relaxed);
    test_out_dir().join(format!("{stem}-{unique_id}.{ext}"))
}

/// Build test args with a unique output path.
pub fn build_args(layout_path: &str, block_name: &str, format: OutputFormat) -> Args {
    let ext = match format {
        OutputFormat::Hex => "hex",
        OutputFormat::Mot => "mot",
    };
    Args {
        layout: LayoutArgs {
            blocks: vec![BlockSelector::named(layout_path, block_name)],
            strict: false,
        },
        data: mint_cli::data_args::DataArgs {
            xlsx: Some("../mint-core/tests/data/data.xlsx".to_owned()),
            variants: vec!["Default".to_owned()],
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

pub fn find_working_datasource() -> Box<dyn DataSource> {
    let variant_candidates: [&str; 2] = ["Default", "VarA/Default"];
    let mut failures = Vec::new();

    for ver in &variant_candidates {
        let ver_args = mint_cli::data_args::DataArgs {
            xlsx: Some("../mint-core/tests/data/data.xlsx".to_owned()),
            variants: ver.split('/').map(str::to_owned).collect(),
            ..Default::default()
        };
        match data::create_data_source(&ver_args) {
            Ok(Some(ds)) => return ds,
            Ok(None) => failures.push(format!("{ver}: no data source created")),
            Err(error) => failures.push(format!("{ver}: {error}")),
        }
    }
    panic!(
        "expected checked-in Excel fixture at ../mint-core/tests/data/data.xlsx to load with a known variant: {}",
        failures.join("; ")
    );
}

/// Assert that the output file exists at the given path
pub fn assert_out_file_exists(out_path: &Path) {
    assert!(
        out_path.exists(),
        "expected output file to exist: {}",
        out_path.display()
    );
}

/// Build test args for multiple layouts with a unique output path.
pub fn build_args_for_layouts(
    layouts: Vec<BlockSelector>,
    format: OutputFormat,
    stem: &str,
) -> Args {
    let ext = match format {
        OutputFormat::Hex => "hex",
        OutputFormat::Mot => "mot",
    };
    Args {
        layout: LayoutArgs {
            blocks: layouts,
            strict: false,
        },
        data: mint_cli::data_args::DataArgs {
            xlsx: Some("../mint-core/tests/data/data.xlsx".to_owned()),
            variants: vec!["Default".to_owned()],
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

/// Renders an error and its full source chain as a single string.
pub fn error_chain(err: &dyn std::error::Error) -> String {
    let mut message = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}
