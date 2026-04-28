pub mod args;
pub mod block;
mod conversions;
mod entry;
pub mod error;
pub mod header;
pub mod scalar_type;
pub mod settings;
pub mod used_values;
pub mod value;

use block::Config;
use error::LayoutError;
use scalar_type::ScalarType;
use serde_yaml::Value as YamlValue;
use std::collections::hash_map::Entry;
use std::path::Path;
use std::str::FromStr;
use toml::Value as TomlValue;

pub fn load_layout(filename: &str) -> Result<Config, LayoutError> {
    let text = std::fs::read_to_string(filename)
        .map_err(|_| LayoutError::FileError(format!("failed to open file: {}", filename)))?;

    let ext = Path::new(filename)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    let mut cfg: Config = match ext.as_str() {
        "toml" => {
            let raw: TomlValue = toml::from_str(&text).map_err(|e| {
                LayoutError::FileError(format!("failed to parse file {}: {}", filename, e))
            })?;
            validate_toml_scalar_types(&raw)?;
            raw.try_into().map_err(|e| {
                LayoutError::FileError(format!("failed to parse file {}: {}", filename, e))
            })?
        }
        "yaml" | "yml" => {
            let raw: YamlValue = serde_yaml::from_str(&text).map_err(|e| {
                LayoutError::FileError(format!("failed to parse file {}: {}", filename, e))
            })?;
            validate_yaml_scalar_types(&raw)?;
            serde_yaml::from_str(&text).map_err(|e| {
                LayoutError::FileError(format!("failed to parse file {}: {}", filename, e))
            })?
        }
        _ => {
            return Err(LayoutError::FileError(
                "Unsupported layout file format; use .toml or .yaml".to_owned(),
            ));
        }
    };

    promote_block_header_consts(&mut cfg)?;

    Ok(cfg)
}

fn promote_block_header_consts(cfg: &mut Config) -> Result<(), LayoutError> {
    for (block_name, block) in &cfg.blocks {
        insert_promoted_const(
            &mut cfg.mint.consts,
            format!("{block_name}.start_address"),
            block.header.start_address,
        )?;
        insert_promoted_const(
            &mut cfg.mint.consts,
            format!("{block_name}.length"),
            block.header.length,
        )?;
    }
    Ok(())
}

fn insert_promoted_const(
    consts: &mut std::collections::HashMap<String, value::ValueSource>,
    name: String,
    value: u32,
) -> Result<(), LayoutError> {
    match consts.entry(name) {
        Entry::Occupied(entry) => Err(LayoutError::FileError(format!(
            "[mint.const] key '{}' collides with auto-promoted block header const",
            entry.key()
        ))),
        Entry::Vacant(entry) => {
            entry.insert(value::ValueSource::Single(value::DataValue::U64(
                u64::from(value),
            )));
            Ok(())
        }
    }
}

fn validate_toml_scalar_types(raw: &TomlValue) -> Result<(), LayoutError> {
    let mut path = Vec::new();
    validate_toml_scalar_types_inner(raw, &mut path)
}

fn validate_toml_scalar_types_inner(
    value: &TomlValue,
    path: &mut Vec<String>,
) -> Result<(), LayoutError> {
    match value {
        TomlValue::Table(table) => {
            if let Some(type_name) = table.get("type").and_then(TomlValue::as_str) {
                validate_scalar_type(type_name, path)?;
            }
            for (key, child) in table {
                path.push(key.clone());
                validate_toml_scalar_types_inner(child, path)?;
                path.pop();
            }
            Ok(())
        }
        TomlValue::Array(items) => {
            for item in items {
                validate_toml_scalar_types_inner(item, path)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_yaml_scalar_types(raw: &YamlValue) -> Result<(), LayoutError> {
    let mut path = Vec::new();
    validate_yaml_scalar_types_inner(raw, &mut path)
}

fn validate_yaml_scalar_types_inner(
    value: &YamlValue,
    path: &mut Vec<String>,
) -> Result<(), LayoutError> {
    match value {
        YamlValue::Mapping(mapping) => {
            if let Some(YamlValue::String(type_name)) =
                mapping.get(YamlValue::String("type".to_owned()))
            {
                validate_scalar_type(type_name, path)?;
            }
            for (key, child) in mapping {
                if let YamlValue::String(key) = key {
                    path.push(key.clone());
                    validate_yaml_scalar_types_inner(child, path)?;
                    path.pop();
                }
            }
            Ok(())
        }
        YamlValue::Sequence(items) => {
            for item in items {
                validate_yaml_scalar_types_inner(item, path)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_scalar_type(type_name: &str, path: &[String]) -> Result<(), LayoutError> {
    ScalarType::from_str(type_name).map_err(|message| {
        let field_path = if path.is_empty() {
            "<root>".to_owned()
        } else {
            path.join(".")
        };
        LayoutError::FileError(format!("{} at '{}'", message, field_path))
    })?;
    Ok(())
}
