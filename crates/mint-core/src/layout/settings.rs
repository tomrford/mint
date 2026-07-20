use super::abi::Abi;
use super::error::LayoutError;
use super::value::ValueSource;
use serde::Deserialize;
use std::collections::HashMap;

/// Top-level `[mint]` configuration section.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MintConfig {
    pub abi: Abi,
    #[serde(default)]
    pub checksum: HashMap<String, ChecksumConfig>,
    #[serde(rename = "const", default)]
    pub consts: HashMap<String, ValueSource>,
}

impl MintConfig {
    pub(crate) fn checksum_config(&self, name: &str) -> Result<&ChecksumConfig, LayoutError> {
        if name.is_empty() {
            return Err(LayoutError::DataValueExportFailed(
                "Checksum config name must not be empty.".to_owned(),
            ));
        }
        self.checksum.get(name).ok_or_else(|| {
            let available = self.checksum.keys().cloned().collect::<Vec<_>>().join(", ");
            LayoutError::DataValueExportFailed(format!(
                "Checksum config '{name}' not found in [mint.checksum]. Available: [{available}]"
            ))
        })
    }
}

/// Named checksum algorithm configuration, referenced by leaf entries via `checksum = "name"`.
/// All fields are required — no inheritance or merging.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ChecksumConfig {
    pub polynomial: u32,
    pub start: u32,
    pub xor_out: u32,
    pub ref_in: bool,
    pub ref_out: bool,
}
