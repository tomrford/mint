use crate::build::BlockSelector;
use crate::layout;
use crate::layout::block::Config;
use crate::layout::error::LayoutError;
use crate::layout::resolved::validate_static;

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
    for block in config.blocks.values() {
        validate_static(&block.data, &config.mint)?;
    }
    Ok(layout::fingerprint::calculate(config)?
        .into_iter()
        .map(|(block, value)| BlockFingerprint { block, value })
        .collect())
}

/// Calculate one block's fingerprint, validating only that block and its
/// fingerprint targets.
pub fn calculate_block(config: &Config, name: &str) -> Result<BlockFingerprint, LayoutError> {
    let block = config.blocks.get(name).ok_or_else(|| {
        LayoutError::BlockNotFound(format!(
            "'{name}'. Available blocks: {}",
            config.blocks.keys().cloned().collect::<Vec<_>>().join(", ")
        ))
    })?;
    validate_static(&block.data, &config.mint)?;
    let value = layout::fingerprint::calculate_scoped(config, [name], true)?
        .swap_remove(name)
        .ok_or_else(|| {
            LayoutError::BlockNotFound(format!("'{name}' missing from scoped calculation"))
        })?;
    Ok(BlockFingerprint {
        block: name.to_owned(),
        value,
    })
}

/// Load a layout and calculate the selected block fingerprints.
pub fn load(selector: &BlockSelector) -> Result<Vec<BlockFingerprint>, LayoutError> {
    let config = layout::load_layout(&selector.layout)?;
    if let Some(name) = &selector.block {
        if !config.blocks.contains_key(name) {
            return Err(LayoutError::BlockNotFound(format!(
                "'{name}' in '{}'. Available blocks: {}",
                selector.layout.display(),
                config.blocks.keys().cloned().collect::<Vec<_>>().join(", ")
            )));
        }
        return Ok(vec![calculate_block(&config, name)?]);
    }
    calculate(&config)
}
