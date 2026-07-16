#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use mint_core::build::{self, BlockSelector, BuildRequest};
use mint_core::data::{DataSource, ExcelDataSource, ExcelDataSourceOptions};
use mint_core::layout::error::LayoutError;
use mint_core::layout::resolved::ResolvedLayout;

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

pub fn find_working_datasource() -> Box<dyn DataSource> {
    let variant_candidates: [&str; 2] = ["Default", "VarA/Default"];
    let mut failures = Vec::new();

    for ver in &variant_candidates {
        let variants = ver.split('/').map(str::to_owned).collect();
        let options = ExcelDataSourceOptions::new(variants);
        match ExcelDataSource::from_path("tests/data/data.xlsx", options) {
            Ok(ds) => return Box::new(ds),
            Err(error) => failures.push(format!("{ver}: {error}")),
        }
    }
    panic!(
        "expected checked-in Excel fixture at tests/data/data.xlsx to load with a known variant: {}",
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

/// Build a block's bytestream, returning `(bytes, padding_count)`.
pub fn build_block(
    layout_path: impl AsRef<Path>,
    block_name: &str,
    strict: bool,
    data_source: Option<&dyn DataSource>,
) -> Result<(Vec<u8>, u32), mint_core::error::MintError> {
    let layout_path = layout_path.as_ref();
    let config = mint_core::layout::load_layout(layout_path)?;
    let block = config.blocks.get(block_name).ok_or_else(|| {
        LayoutError::BlockNotFound(format!("test block '{block_name}' not found"))
    })?;
    let resolved = ResolvedLayout::new(&block.data)?;
    let leaf_size = resolved
        .leaves()
        .map(|leaf| leaf.coordinates.size)
        .sum::<usize>();
    let padding_count = u32::try_from(resolved.total_size() - leaf_size).map_err(|_| {
        LayoutError::DataValueExportFailed("test padding count exceeds u32".to_owned())
    })?;
    let artifact = build::build(BuildRequest {
        blocks: vec![BlockSelector::named(layout_path, block_name)],
        data_source,
        strict,
        capture_values: false,
    })?;
    let bytestream = artifact
        .ranges
        .into_iter()
        .next()
        .expect("one requested block produces one range")
        .bytestream;
    Ok((bytestream, padding_count))
}

/// Build a block's bytestream and collect exported values.
pub fn build_block_with_values(
    layout_path: impl AsRef<Path>,
    block_name: &str,
) -> Result<((Vec<u8>, u32), serde_json::Value), mint_core::error::MintError> {
    let layout_path = layout_path.as_ref();
    let config = mint_core::layout::load_layout(layout_path)?;
    let block = config.blocks.get(block_name).ok_or_else(|| {
        LayoutError::BlockNotFound(format!("test block '{block_name}' not found"))
    })?;
    let resolved = ResolvedLayout::new(&block.data)?;
    let leaf_size = resolved
        .leaves()
        .map(|leaf| leaf.coordinates.size)
        .sum::<usize>();
    let padding_count = u32::try_from(resolved.total_size() - leaf_size).map_err(|_| {
        LayoutError::DataValueExportFailed("test padding count exceeds u32".to_owned())
    })?;
    let artifact = build::build(BuildRequest {
        blocks: vec![BlockSelector::named(layout_path, block_name)],
        data_source: None,
        strict: false,
        capture_values: true,
    })?;
    let bytestream = artifact
        .ranges
        .into_iter()
        .next()
        .expect("one requested block produces one range")
        .bytestream;
    let report = artifact.used_values.expect("captured values report");
    let values = report[layout_path.to_string_lossy().as_ref()][block_name].clone();
    Ok(((bytestream, padding_count), values))
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
