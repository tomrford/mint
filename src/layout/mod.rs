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

fn validate_removed_toml_keys(value: &toml::Value) -> Result<(), LayoutError> {
    let Some(root) = value.as_table() else {
        return Ok(());
    };

    if root.contains_key("settings") {
        return Err(LayoutError::FileError(settings_removed_message()));
    }

    if let Some(mint) = root.get("mint").and_then(toml::Value::as_table)
        && mint.contains_key("crc")
    {
        return Err(LayoutError::FileError(mint_crc_removed_message()));
    }

    for (block_name, block_value) in root {
        if matches!(block_name.as_str(), "mint" | "settings") {
            continue;
        }
        let Some(block) = block_value.as_table() else {
            continue;
        };
        let Some(header) = block.get("header").and_then(toml::Value::as_table) else {
            continue;
        };
        if header.contains_key("crc") {
            return Err(LayoutError::FileError(header_crc_removed_message(
                block_name,
            )));
        }
        if header.contains_key("crc_location") {
            return Err(LayoutError::FileError(crc_location_removed_message(
                block_name,
            )));
        }
    }

    Ok(())
}

fn validate_removed_yaml_keys(value: &serde_yaml::Value) -> Result<(), LayoutError> {
    let Some(root) = value.as_mapping() else {
        return Ok(());
    };

    if yaml_mapping_get(root, "settings").is_some() {
        return Err(LayoutError::FileError(settings_removed_message()));
    }

    if let Some(mint) = yaml_mapping_get(root, "mint").and_then(serde_yaml::Value::as_mapping)
        && yaml_mapping_get(mint, "crc").is_some()
    {
        return Err(LayoutError::FileError(mint_crc_removed_message()));
    }

    for (key, block_value) in root {
        let Some(block_name) = key.as_str() else {
            continue;
        };
        if matches!(block_name, "mint" | "settings") {
            continue;
        }
        let Some(block) = block_value.as_mapping() else {
            continue;
        };
        let Some(header) =
            yaml_mapping_get(block, "header").and_then(serde_yaml::Value::as_mapping)
        else {
            continue;
        };
        if yaml_mapping_get(header, "crc").is_some() {
            return Err(LayoutError::FileError(header_crc_removed_message(
                block_name,
            )));
        }
        if yaml_mapping_get(header, "crc_location").is_some() {
            return Err(LayoutError::FileError(crc_location_removed_message(
                block_name,
            )));
        }
    }

    Ok(())
}

fn yaml_mapping_get<'a>(map: &'a serde_yaml::Mapping, key: &str) -> Option<&'a serde_yaml::Value> {
    map.get(serde_yaml::Value::String(key.to_string()))
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
