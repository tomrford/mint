pub mod args;
pub mod block;
mod conversions;
mod entry;
pub mod error;
pub mod header;
pub mod settings;
pub mod used_values;
pub mod value;

use block::Config;
use error::LayoutError;
use std::path::Path;

pub fn load_layout(filename: &str) -> Result<Config, LayoutError> {
    let text = std::fs::read_to_string(filename)
        .map_err(|_| LayoutError::FileError(format!("failed to open file: {}", filename)))?;

    let ext = Path::new(filename)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    let cfg: Config = match ext.as_str() {
        "toml" => toml::from_str(&text).map_err(|e| {
            LayoutError::FileError(format!("failed to parse file {}: {}", filename, e))
        })?,
        "yaml" | "yml" => serde_yaml::from_str(&text).map_err(|e| {
            LayoutError::FileError(format!("failed to parse file {}: {}", filename, e))
        })?,
        "json" => {
            return Err(LayoutError::FileError(format!(
                "JSON layout files are no longer supported; migrate {} to TOML",
                filename
            )));
        }
        _ => {
            return Err(LayoutError::FileError(
                "Unsupported layout file format; use .toml or .yaml".to_string(),
            ));
        }
    };

    Ok(cfg)
}
