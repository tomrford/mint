use super::block::{Config, Entry};
use super::entry::{EntrySource, SizeSource};
use super::error::LayoutError;
use super::resolved::{ResolvedLayout, ResolvedLeafKind, ResolvedNode, TargetKind};
use super::scalar_type::ScalarType;
use super::settings::Endianness;
use indexmap::IndexMap;

const HASH_CONTEXT: &str = "mint block ABI fingerprint v1";

/// Whether any block in the layout contains a fingerprint field.
pub(crate) fn uses_fingerprints(config: &Config) -> bool {
    fn entry_uses_fingerprints(entry: &Entry) -> bool {
        match entry {
            Entry::Leaf(leaf) => matches!(leaf.source, EntrySource::Fingerprint(_)),
            Entry::Branch(entries) => entries.values().any(entry_uses_fingerprints),
        }
    }

    config
        .blocks
        .values()
        .any(|block| entry_uses_fingerprints(&block.data))
}

pub(crate) fn calculate(config: &Config) -> Result<IndexMap<String, u64>, LayoutError> {
    let mut dependencies = IndexMap::with_capacity(config.blocks.len());
    let mut fingerprints = IndexMap::with_capacity(config.blocks.len());

    for (name, block) in &config.blocks {
        let resolved = ResolvedLayout::new(&block.data)?;
        let value = fingerprint(&resolved, config.mint.endianness)?;
        dependencies.insert(
            name.clone(),
            resolved
                .fingerprint_targets
                .iter()
                .map(|target| (*target).clone())
                .collect::<Vec<_>>(),
        );
        fingerprints.insert(name.clone(), value);
    }

    for (block_name, targets) in dependencies {
        for target in targets {
            let target_name = target.block_name(&block_name);
            if !fingerprints.contains_key(target_name) {
                return Err(LayoutError::BlockNotFound(format!(
                    "fingerprint target '{target_name}' from block '{block_name}'. Available blocks: {}",
                    fingerprints.keys().cloned().collect::<Vec<_>>().join(", ")
                )));
            }
        }
    }

    Ok(fingerprints)
}

fn fingerprint(resolved: &ResolvedLayout<'_>, endianness: Endianness) -> Result<u64, LayoutError> {
    let mut hasher = blake3::Hasher::new_derive_key(HASH_CONTEXT);
    hasher.update(&[match endianness {
        Endianness::Little => 0,
        Endianness::Big => 1,
    }]);
    hash_node(&resolved.root, resolved, &mut hasher)?;

    let digest = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest.as_bytes()[..8]);
    Ok(u64::from_be_bytes(bytes))
}

fn hash_node(
    node: &ResolvedNode<'_>,
    resolved: &ResolvedLayout<'_>,
    hasher: &mut blake3::Hasher,
) -> Result<(), LayoutError> {
    match node {
        ResolvedNode::Branch {
            coordinates,
            children,
        } => {
            hasher.update(&[0]);
            hash_usize(coordinates.offset, hasher)?;
            hash_usize(coordinates.size, hasher)?;
            hash_usize(coordinates.alignment, hasher)?;
            hash_usize(children.len(), hasher)?;
            for (_, child) in children {
                hash_node(child, resolved, hasher)?;
            }
        }
        ResolvedNode::Leaf {
            coordinates,
            leaf,
            dimensions,
            kind,
        } => {
            hasher.update(&[1]);
            hash_usize(coordinates.offset, hasher)?;
            hash_usize(coordinates.size, hasher)?;
            hash_usize(coordinates.alignment, hasher)?;
            hash_scalar(leaf.scalar_type, hasher);
            hash_dimensions(dimensions.as_ref(), hasher)?;
            match kind {
                ResolvedLeafKind::Plain => {
                    hasher.update(&[0]);
                }
                ResolvedLeafKind::Bitmap(widths) => {
                    hasher.update(&[1]);
                    hash_usize(widths.len(), hasher)?;
                    for width in widths {
                        hash_usize(*width, hasher)?;
                    }
                }
                ResolvedLeafKind::Ref(path) => {
                    hasher.update(&[2]);
                    let target = resolved.target(path).ok_or_else(|| {
                        layout_size_error(format!(
                            "ref target '{path}' disappeared after resolution"
                        ))
                    })?;
                    hasher.update(&[match target.kind {
                        TargetKind::Branch => 0,
                        TargetKind::Leaf => 1,
                    }]);
                    hash_usize(target.coordinates.offset, hasher)?;
                    hash_usize(target.coordinates.size, hasher)?;
                    hash_usize(target.coordinates.alignment, hasher)?;
                }
            }
        }
    }
    Ok(())
}

fn hash_scalar(scalar: ScalarType, hasher: &mut blake3::Hasher) {
    match scalar {
        ScalarType::U8 => {
            hasher.update(&[0]);
        }
        ScalarType::U16 => {
            hasher.update(&[1]);
        }
        ScalarType::U32 => {
            hasher.update(&[2]);
        }
        ScalarType::U64 => {
            hasher.update(&[3]);
        }
        ScalarType::I8 => {
            hasher.update(&[4]);
        }
        ScalarType::I16 => {
            hasher.update(&[5]);
        }
        ScalarType::I32 => {
            hasher.update(&[6]);
        }
        ScalarType::I64 => {
            hasher.update(&[7]);
        }
        ScalarType::F32 => {
            hasher.update(&[8]);
        }
        ScalarType::F64 => {
            hasher.update(&[9]);
        }
        ScalarType::Fixed(fixed) => {
            hasher.update(&[
                10,
                u8::from(fixed.signed),
                fixed.integer_bits,
                fixed.fractional_bits,
                fixed.total_bits,
            ]);
        }
    };
}

fn hash_dimensions(
    dimensions: Option<&SizeSource>,
    hasher: &mut blake3::Hasher,
) -> Result<(), LayoutError> {
    match dimensions {
        None => {
            hasher.update(&[0]);
        }
        Some(SizeSource::OneD(length)) => {
            hasher.update(&[1]);
            hash_usize(*length, hasher)?;
        }
        Some(SizeSource::TwoD([rows, columns])) => {
            hasher.update(&[2]);
            hash_usize(*rows, hasher)?;
            hash_usize(*columns, hasher)?;
        }
    }
    Ok(())
}

fn hash_usize(value: usize, hasher: &mut blake3::Hasher) -> Result<(), LayoutError> {
    let value = u64::try_from(value).map_err(|_| layout_size_error("layout value exceeds u64"))?;
    hasher.update(&value.to_le_bytes());
    Ok(())
}

fn layout_size_error(message: impl Into<String>) -> LayoutError {
    LayoutError::DataValueExportFailed(message.into())
}
