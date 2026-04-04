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
        "toml" => {
            let raw: toml::Value = toml::from_str(&text).map_err(|e| {
                LayoutError::FileError(format!("failed to parse file {}: {}", filename, e))
            })?;
            validate_removed_toml_keys(&raw)?;
            raw.try_into().map_err(|e| {
                LayoutError::FileError(format!("failed to parse file {}: {}", filename, e))
            })?
        }
        "yaml" | "yml" => {
            let raw: serde_yaml::Value = serde_yaml::from_str(&text).map_err(|e| {
                LayoutError::FileError(format!("failed to parse file {}: {}", filename, e))
            })?;
            validate_removed_yaml_keys(&raw)?;
            serde_yaml::from_value(raw).map_err(|e| {
                LayoutError::FileError(format!("failed to parse file {}: {}", filename, e))
            })?
        }
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

/// Abstraction over TOML/YAML map access for migration validation.
trait MapAccess {
    fn has_key(&self, key: &str) -> bool;
    fn get_map(&self, key: &str) -> Option<&Self>;
    /// Iterate top-level entries as `(key_name, value)` pairs.
    fn entries(&self) -> Vec<(&str, &Self)>;
}

impl MapAccess for toml::Value {
    fn has_key(&self, key: &str) -> bool {
        self.as_table().is_some_and(|t| t.contains_key(key))
    }
    fn get_map(&self, key: &str) -> Option<&Self> {
        self.as_table()?.get(key).filter(|v| v.is_table())
    }
    fn entries(&self) -> Vec<(&str, &Self)> {
        self.as_table()
            .map(|t| t.iter().map(|(k, v)| (k.as_str(), v)).collect())
            .unwrap_or_default()
    }
}

impl MapAccess for serde_yaml::Value {
    fn has_key(&self, key: &str) -> bool {
        self.as_mapping()
            .is_some_and(|m| m.contains_key(serde_yaml::Value::String(key.to_string())))
    }
    fn get_map(&self, key: &str) -> Option<&Self> {
        self.as_mapping()?
            .get(serde_yaml::Value::String(key.to_string()))
            .filter(|v| v.is_mapping())
    }
    fn entries(&self) -> Vec<(&str, &Self)> {
        self.as_mapping()
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| Some((k.as_str()?, v)))
                    .collect()
            })
            .unwrap_or_default()
    }
}

// TODO: remove once deprecated `settings`, `mint.crc`, `header.crc`, and `crc_location`
// keys are no longer in circulation.
fn validate_removed_toml_keys(value: &toml::Value) -> Result<(), LayoutError> {
    validate_removed_keys(value)
}

fn validate_removed_yaml_keys(value: &serde_yaml::Value) -> Result<(), LayoutError> {
    validate_removed_keys(value)
}

fn validate_removed_keys(root: &impl MapAccess) -> Result<(), LayoutError> {
    if root.has_key("settings") {
        return Err(LayoutError::FileError(settings_removed_message()));
    }

    if let Some(mint) = root.get_map("mint")
        && mint.has_key("crc")
    {
        return Err(LayoutError::FileError(mint_crc_removed_message()));
    }

    for (block_name, block_value) in root.entries() {
        if matches!(block_name, "mint" | "settings") {
            continue;
        }
        let Some(header) = block_value.get_map("header") else {
            continue;
        };
        if header.has_key("crc") {
            return Err(LayoutError::FileError(header_crc_removed_message(
                block_name,
            )));
        }
        if header.has_key("crc_location") {
            return Err(LayoutError::FileError(crc_location_removed_message(
                block_name,
            )));
        }
    }

    Ok(())
}

fn settings_removed_message() -> String {
    "`[settings]` was removed; rename it to `[mint]`. Define named checksum algorithms under `[mint.checksum.<name>]` and reference them from block data with `checksum = { checksum = \"<name>\", type = \"u32\" }`.".to_string()
}

fn mint_crc_removed_message() -> String {
    "`[mint.crc]` was removed. Define named checksum algorithms under `[mint.checksum.<name>]` and place checksums inline in block data with `checksum = { checksum = \"<name>\", type = \"u32\" }`.".to_string()
}

fn header_crc_removed_message(block_name: &str) -> String {
    format!(
        "`[{block_name}.header.crc]` was removed. Place the checksum inline in `[{block_name}.data]` with `checksum = {{ checksum = \"<name>\", type = \"u32\" }}` and reference a named config from `[mint.checksum.<name>]`."
    )
}

fn crc_location_removed_message(block_name: &str) -> String {
    format!(
        "`[{block_name}.header].crc_location` was removed. Place the checksum inline in `[{block_name}.data]` with `checksum = {{ checksum = \"<name>\", type = \"u32\" }}`."
    )
}
