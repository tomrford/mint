#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use mint_core::build::{self, BlockSelector, BuildRequest};
use mint_core::data::DataSource;

static UNIQUE_FILE_ID: AtomicU64 = AtomicU64::new(0);

fn test_out_dir() -> PathBuf {
    std::env::temp_dir()
        .join("mint-core-tests")
        .join(std::process::id().to_string())
}

fn ensure_out_dir() {
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

pub fn build_block(
    layout_path: impl AsRef<Path>,
    block_name: &str,
    strict: bool,
    data_source: Option<&dyn DataSource>,
) -> Result<Vec<u8>, mint_core::error::MintError> {
    let layout_path = layout_path.as_ref();
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
    Ok(bytestream)
}

pub fn build_block_with_values(
    layout_path: impl AsRef<Path>,
    block_name: &str,
) -> Result<(Vec<u8>, serde_json::Value), mint_core::error::MintError> {
    let layout_path = layout_path.as_ref();
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
    Ok((bytestream, values))
}

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
