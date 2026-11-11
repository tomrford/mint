use crate::build::BlockSelector;
use crate::layout;
use crate::layout::block::{Block, Config};
use crate::layout::error::LayoutError;
use crate::layout::plan::plan_block;

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

/// Calculate every block fingerprint in declaration order, fully validating
/// every block first.
pub fn calculate(config: &Config) -> Result<Vec<BlockFingerprint>, LayoutError> {
    config
        .blocks
        .iter()
        .map(|(name, block)| fingerprint_planned(config, name, block))
        .collect()
}

/// Calculate one block's fingerprint, fully validating that block and resolving
/// the ABIs of its fingerprint targets.
pub fn calculate_block(config: &Config, name: &str) -> Result<BlockFingerprint, LayoutError> {
    let (name, block) = config.blocks.get_key_value(name).ok_or_else(|| {
        LayoutError::BlockNotFound(format!(
            "'{name}'. Available blocks: {}",
            config.blocks.keys().cloned().collect::<Vec<_>>().join(", ")
        ))
    })?;
    fingerprint_planned(config, name, block)
}

fn fingerprint_planned(
    config: &Config,
    name: &str,
    block: &Block,
) -> Result<BlockFingerprint, LayoutError> {
    let planned = plan_block(config, name, block).map_err(|error| LayoutError::InBlock {
        block: name.to_owned(),
        error: Box::new(error),
    })?;
    Ok(BlockFingerprint {
        block: name.to_owned(),
        value: planned.fingerprint(),
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
