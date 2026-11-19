use crate::layout;
use crate::layout::block::Config;
use crate::layout::error::LayoutError;
use crate::layout::selection::{BlockSelector, plan_layout, select_blocks};
use std::path::Path;

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
    calculate_selected(config, None, None)
}

/// Calculate one block's fingerprint, fully validating that block and resolving
/// the ABIs of its fingerprint targets.
pub fn calculate_block(config: &Config, name: &str) -> Result<BlockFingerprint, LayoutError> {
    let Some(fingerprint) = calculate_selected(config, Some(name), None)?.pop() else {
        unreachable!("named block selection always yields one block");
    };
    Ok(fingerprint)
}

/// Load a layout and calculate the selected block fingerprints.
pub fn load(selector: &BlockSelector) -> Result<Vec<BlockFingerprint>, LayoutError> {
    let config = layout::load_layout(&selector.layout)?;
    let fingerprints =
        calculate_selected(&config, selector.block.as_deref(), Some(&selector.layout))?;
    if fingerprints.is_empty() {
        return Err(LayoutError::NoBlocksProvided);
    }
    Ok(fingerprints)
}

fn calculate_selected(
    config: &Config,
    name: Option<&str>,
    path: Option<&Path>,
) -> Result<Vec<BlockFingerprint>, LayoutError> {
    let roots = select_blocks(config, name, path)?;
    let resolved = plan_layout(
        config,
        roots.iter().map(|(name, block)| (name.as_str(), block)),
        true,
    )?;
    Ok(roots
        .keys()
        .map(|name| BlockFingerprint {
            block: name.clone(),
            value: resolved.fingerprints[name.as_str()],
        })
        .collect())
}
