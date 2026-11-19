use crate::build::BlockSelector;
use crate::layout;
use crate::layout::block::{Block, Config};
use crate::layout::error::LayoutError;

/// A block's deterministic 64-bit ABI fingerprint.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BlockFingerprint {
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
    let blocks = config
        .blocks
        .iter()
        .map(|(name, block)| (name.as_str(), block));
    let resolved = layout::fingerprint::plan_layout(config, blocks, true)?;
    Ok(resolved
        .fingerprints
        .into_iter()
        .map(|(block, value)| BlockFingerprint { block, value })
        .collect())
}

/// Calculate one block's fingerprint, fully validating that block and resolving
/// the ABIs of its fingerprint targets.
pub fn calculate_block(config: &Config, name: &str) -> Result<BlockFingerprint, LayoutError> {
    let block = config.blocks.get(name).ok_or_else(|| {
        LayoutError::BlockNotFound(format!(
            "'{name}'. Available blocks: {}",
            config.blocks.keys().cloned().collect::<Vec<_>>().join(", ")
        ))
    })?;
    calculate_selected(config, name, block)
}

fn calculate_selected(
    config: &Config,
    name: &str,
    block: &Block,
) -> Result<BlockFingerprint, LayoutError> {
    let resolved = layout::fingerprint::plan_layout(config, [(name, block)], true)?;
    let value = resolved.fingerprints[name];
    Ok(BlockFingerprint {
        block: name.to_owned(),
        value,
    })
}

/// Load a layout and calculate the selected block fingerprints.
pub fn load(selector: &BlockSelector) -> Result<Vec<BlockFingerprint>, LayoutError> {
    let config = layout::load_layout(&selector.layout)?;
    if let Some(name) = &selector.block {
        let block = config.blocks.get(name).ok_or_else(|| {
            LayoutError::BlockNotFound(format!(
                "'{name}' in '{}'. Available blocks: {}",
                selector.layout.display(),
                config.blocks.keys().cloned().collect::<Vec<_>>().join(", ")
            ))
        })?;
        return Ok(vec![calculate_selected(&config, name, block)?]);
    }
    calculate(&config)
}
