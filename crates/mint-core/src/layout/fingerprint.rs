use super::block::Config;
use super::entry::SizeSource;
use super::error::LayoutError;
use super::resolved::{ResolvedLayout, ResolvedLeafKind, ResolvedNode, TargetKind};
use super::scalar_type::ScalarType;
use super::settings::Endianness;
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

const HASH_CONTEXT: &str = "mint block ABI fingerprint v1";

pub(crate) fn calculate(config: &Config) -> Result<IndexMap<String, u64>, LayoutError> {
    calculate_scoped(config, config.blocks.keys().map(String::as_str), true)
}

pub(crate) fn calculate_scoped<'a>(
    config: &Config,
    roots: impl IntoIterator<Item = &'a str>,
    hash_roots: bool,
) -> Result<IndexMap<String, u64>, LayoutError> {
    let available = || config.blocks.keys().cloned().collect::<Vec<_>>().join(", ");
    let mut resolved_roots = HashMap::new();
    let mut root_names = HashSet::new();
    let mut hash_names = HashSet::new();

    for root_name in roots {
        if !root_names.insert(root_name.to_owned()) {
            continue;
        }
        let block = config.blocks.get(root_name).ok_or_else(|| {
            LayoutError::BlockNotFound(format!("'{root_name}'. Available blocks: {}", available()))
        })?;
        let resolved = ResolvedLayout::new(&block.data)?;
        for target in &resolved.fingerprint_targets {
            let target_name = target.block_name(root_name);
            if !config.blocks.contains_key(target_name) {
                return Err(LayoutError::BlockNotFound(format!(
                    "fingerprint target '{target_name}' from block '{root_name}'. Available blocks: {}",
                    available()
                )));
            }
            hash_names.insert(target_name.to_owned());
        }
        resolved_roots.insert(root_name.to_owned(), resolved);
    }

    if hash_roots {
        hash_names.extend(root_names);
    }

    let mut fingerprints = IndexMap::with_capacity(hash_names.len());
    for (name, block) in &config.blocks {
        if !hash_names.contains(name) {
            continue;
        }
        let resolved = match resolved_roots.remove(name) {
            Some(resolved) => resolved,
            None => ResolvedLayout::new(&block.data)?,
        };
        fingerprints.insert(
            name.clone(),
            fingerprint(&resolved, config.mint.endianness)?,
        );
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
    LayoutError::InvalidLayout(message.into())
}
