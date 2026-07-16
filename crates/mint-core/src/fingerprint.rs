use crate::build::BlockSelector;
use crate::layout;
use crate::layout::block::Config;
use crate::layout::error::LayoutError;

/// A block's deterministic 64-bit ABI fingerprint.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BlockFingerprint {
    /// Block name from the layout.
    pub block: String,
    /// Numeric fingerprint written to fingerprint fields.
    pub value: u64,
}

impl BlockFingerprint {
    /// Return the fingerprint as exactly 16 lowercase hexadecimal characters.
    pub fn hex(&self) -> String {
        format!("{:016x}", self.value)
    }
}

/// Calculate every block fingerprint in declaration order.
pub fn calculate(config: &Config) -> Result<Vec<BlockFingerprint>, LayoutError> {
    Ok(layout::fingerprint::calculate(config)?
        .into_iter()
        .map(|(block, value)| BlockFingerprint { block, value })
        .collect())
}

/// Load a layout and calculate the selected block fingerprints.
pub fn load(selector: &BlockSelector) -> Result<Vec<BlockFingerprint>, LayoutError> {
    let config = layout::load_layout(&selector.layout)?;
    let mut fingerprints = calculate(&config)?;

    if let Some(name) = &selector.block {
        let available = fingerprints
            .iter()
            .map(|fingerprint| fingerprint.block.clone())
            .collect::<Vec<_>>();
        fingerprints.retain(|fingerprint| &fingerprint.block == name);
        if fingerprints.is_empty() {
            return Err(LayoutError::BlockNotFound(format!(
                "'{name}' in '{}'. Available blocks: {}",
                selector.layout.display(),
                available.join(", ")
            )));
        }
    }

    Ok(fingerprints)
}
