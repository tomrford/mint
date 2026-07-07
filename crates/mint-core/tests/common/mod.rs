#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use mint_core::data::{DataSource, ExcelDataSource, ExcelDataSourceOptions};
use mint_core::layout::used_values::{NoopValueSink, ValueCollector};

static UNIQUE_FILE_ID: AtomicU64 = AtomicU64::new(0);

fn test_out_dir() -> PathBuf {
    std::env::temp_dir()
        .join("mint-core-tests")
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

pub fn find_working_datasource() -> Option<Box<dyn DataSource>> {
    let variant_candidates: [&str; 2] = ["Default", "VarA/Default"];

    for ver in &variant_candidates {
        let variants = ver.split('/').map(str::to_owned).collect();
        let options = ExcelDataSourceOptions::new(variants);
        if let Ok(ds) = ExcelDataSource::from_path("tests/data/data.xlsx", options) {
            return Some(Box::new(ds));
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
    block: &mint_core::layout::block::Block,
    settings: &mint_core::layout::settings::MintConfig,
    strict: bool,
    data_source: Option<&dyn DataSource>,
) -> Result<(Vec<u8>, u32), mint_core::layout::error::LayoutError> {
    let mut noop = NoopValueSink;
    let output = block.build_bytestream(data_source, settings, strict, &mut noop)?;
    Ok((output.bytestream, output.padding_count))
}

/// Build a block's bytestream and collect exported values.
pub fn build_block_with_values(
    block: &mint_core::layout::block::Block,
    settings: &mint_core::layout::settings::MintConfig,
) -> Result<((Vec<u8>, u32), serde_json::Value), mint_core::layout::error::LayoutError> {
    let mut collector = ValueCollector::new();
    let output = block.build_bytestream(None, settings, false, &mut collector)?;
    Ok((
        (output.bytestream, output.padding_count),
        collector.into_value(),
    ))
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
