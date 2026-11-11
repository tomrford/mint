use super::block::{Block, Config};
use super::entry::EntrySource;
use super::error::LayoutError;
use super::fingerprint::fingerprint;
use super::resolved::ResolvedLayout;
use std::collections::HashMap;

/// A selected block that passed every static check, with the fingerprints
/// it needs. Commands emit, render or print it without validating again.
pub(crate) struct PlannedBlock<'a> {
    pub(crate) name: &'a str,
    pub(crate) block: &'a Block,
    pub(crate) resolved: ResolvedLayout<'a>,
    /// This block's fingerprint and those of the blocks its fingerprint fields name.
    pub(crate) fingerprints: HashMap<&'a str, u64>,
}

impl PlannedBlock<'_> {
    pub(crate) fn fingerprint(&self) -> u64 {
        self.fingerprints[self.name]
    }
}

/// Resolves and statically validates one selected block. Its fingerprint
/// targets are only shape-resolved; unrelated blocks are not looked at.
pub(crate) fn plan_block<'a>(
    config: &'a Config,
    name: &'a str,
    block: &'a Block,
) -> Result<PlannedBlock<'a>, LayoutError> {
    let abi = config.mint.abi;
    let resolved = ResolvedLayout::new(&block.data, abi)?;
    resolved.validate(block, &config.mint)?;

    let mut fingerprints = HashMap::from([(name, fingerprint(&resolved)?)]);
    for (_, _, _, leaf) in resolved.emission_leaves() {
        let EntrySource::Fingerprint(target) = &leaf.source else {
            continue;
        };
        let target_name = target.block_name(name);
        if fingerprints.contains_key(target_name) {
            continue;
        }
        let (target_name, target) = config.blocks.get_key_value(target_name).ok_or_else(|| {
            LayoutError::BlockNotFound(format!(
                "fingerprint target '{target_name}' from block '{name}'. Available blocks: {}",
                config.blocks.keys().cloned().collect::<Vec<_>>().join(", ")
            ))
        })?;
        let value = ResolvedLayout::new(&target.data, abi)
            .and_then(|target| fingerprint(&target))
            .map_err(|error| LayoutError::InBlock {
                block: target_name.clone(),
                error: Box::new(error),
            })?;
        fingerprints.insert(target_name, value);
    }

    Ok(PlannedBlock {
        name,
        block,
        resolved,
        fingerprints,
    })
}
