use super::block::{Config, Entry};
use super::entry::{EntrySource, FingerprintTarget, LeafEntry, SizeSource};
use super::error::LayoutError;
use super::scalar_type::ScalarType;
use super::settings::Endianness;
use indexmap::IndexMap;
use std::collections::HashMap;

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
        let plan = AbiPlan::new(&block.data)?;
        let value = plan.fingerprint(config.mint.endianness)?;
        dependencies.insert(name.clone(), plan.fingerprint_targets);
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

struct AbiPlan {
    root: AbiNode,
    targets: HashMap<String, AbiTarget>,
    fingerprint_targets: Vec<FingerprintTarget>,
}

impl AbiPlan {
    fn new(entry: &Entry) -> Result<Self, LayoutError> {
        let mut fingerprint_targets = Vec::new();
        let mut root = collect_entry(entry, &mut Vec::new(), &mut fingerprint_targets)?;
        let mut cursor = 0usize;
        let mut targets = HashMap::new();
        layout_node(&mut root, &mut cursor, &mut Vec::new(), &mut targets)?;
        Ok(Self {
            root,
            targets,
            fingerprint_targets,
        })
    }

    fn fingerprint(&self, endianness: Endianness) -> Result<u64, LayoutError> {
        let mut hasher = blake3::Hasher::new_derive_key(HASH_CONTEXT);
        hasher.update(&[match endianness {
            Endianness::Little => 0,
            Endianness::Big => 1,
        }]);
        hash_node(&self.root, &self.targets, &mut hasher)?;

        let digest = hasher.finalize();
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&digest.as_bytes()[..8]);
        Ok(u64::from_be_bytes(bytes))
    }
}

enum AbiNode {
    Branch {
        offset: usize,
        size: usize,
        alignment: usize,
        children: Vec<(String, AbiNode)>,
    },
    Leaf {
        offset: usize,
        size: usize,
        alignment: usize,
        scalar_type: ScalarType,
        dimensions: Option<SizeSource>,
        kind: LeafKind,
    },
}

impl AbiNode {
    fn alignment(&self) -> usize {
        match self {
            Self::Branch { alignment, .. } | Self::Leaf { alignment, .. } => *alignment,
        }
    }

    fn target(&self) -> AbiTarget {
        match self {
            Self::Branch {
                offset,
                size,
                alignment,
                ..
            } => AbiTarget {
                offset: *offset,
                size: *size,
                alignment: *alignment,
                kind: TargetKind::Branch,
            },
            Self::Leaf {
                offset,
                size,
                alignment,
                ..
            } => AbiTarget {
                offset: *offset,
                size: *size,
                alignment: *alignment,
                kind: TargetKind::Leaf,
            },
        }
    }
}

enum LeafKind {
    Plain,
    Bitmap(Vec<usize>),
    Ref(String),
}

#[derive(Clone, Copy)]
struct AbiTarget {
    offset: usize,
    size: usize,
    alignment: usize,
    kind: TargetKind,
}

#[derive(Clone, Copy)]
enum TargetKind {
    Branch,
    Leaf,
}

fn collect_entry(
    entry: &Entry,
    path: &mut Vec<String>,
    fingerprint_targets: &mut Vec<FingerprintTarget>,
) -> Result<AbiNode, LayoutError> {
    match entry {
        Entry::Leaf(leaf) => {
            let dimensions = leaf.size()?;
            let size = leaf_size(leaf, dimensions.as_ref())?;
            let kind = match &leaf.source {
                EntrySource::Bitmap(fields) => {
                    leaf.validate_bitmap(fields)?;
                    LeafKind::Bitmap(fields.iter().map(|field| field.bits).collect())
                }
                EntrySource::Ref(target) => {
                    leaf.validate_ref(target)?;
                    LeafKind::Ref(target.clone())
                }
                EntrySource::Fingerprint(target) => {
                    leaf.validate_fingerprint()?;
                    fingerprint_targets.push(target.clone());
                    LeafKind::Plain
                }
                _ => LeafKind::Plain,
            };
            Ok(AbiNode::Leaf {
                offset: 0,
                size,
                alignment: leaf.get_alignment(),
                scalar_type: leaf.scalar_type,
                dimensions,
                kind,
            })
        }
        Entry::Branch(entries) => {
            if entries.is_empty() {
                let name = if path.is_empty() {
                    "<root>".to_owned()
                } else {
                    path.join(".")
                };
                return Err(layout_size_error(format!(
                    "empty branch '{name}' is invalid"
                )));
            }

            let mut children = Vec::with_capacity(entries.len());
            for (name, child) in entries {
                path.push(name.clone());
                let child = collect_entry(child, path, fingerprint_targets)?;
                path.pop();
                children.push((name.clone(), child));
            }
            let alignment = children
                .iter()
                .map(|(_, child)| child.alignment())
                .max()
                .unwrap_or(1);
            Ok(AbiNode::Branch {
                offset: 0,
                size: 0,
                alignment,
                children,
            })
        }
    }
}

fn layout_node(
    node: &mut AbiNode,
    cursor: &mut usize,
    path: &mut Vec<String>,
    targets: &mut HashMap<String, AbiTarget>,
) -> Result<(), LayoutError> {
    *cursor = aligned_offset(*cursor, node.alignment())?;
    let offset = *cursor;

    match node {
        AbiNode::Leaf {
            offset: node_offset,
            size,
            ..
        } => {
            *node_offset = offset;
            *cursor = cursor
                .checked_add(*size)
                .ok_or_else(|| layout_size_error("leaf byte count overflow"))?;
        }
        AbiNode::Branch {
            offset: node_offset,
            size,
            alignment,
            children,
        } => {
            *node_offset = offset;
            for (name, child) in children {
                path.push(name.clone());
                layout_node(child, cursor, path, targets)?;
                targets.insert(path.join("."), child.target());
                path.pop();
            }
            *cursor = aligned_offset(*cursor, *alignment)?;
            *size = *cursor - offset;
        }
    }
    Ok(())
}

fn leaf_size(leaf: &LeafEntry, dimensions: Option<&SizeSource>) -> Result<usize, LayoutError> {
    let elements = match dimensions {
        None => 1,
        Some(SizeSource::OneD(length)) => *length,
        Some(SizeSource::TwoD([rows, columns])) => rows
            .checked_mul(*columns)
            .ok_or_else(|| layout_size_error("array element count overflow"))?,
    };
    elements
        .checked_mul(leaf.scalar_type.size_bytes())
        .ok_or_else(|| layout_size_error("array byte count overflow"))
}

fn aligned_offset(offset: usize, alignment: usize) -> Result<usize, LayoutError> {
    let remainder = offset % alignment;
    if remainder == 0 {
        return Ok(offset);
    }
    offset
        .checked_add(alignment - remainder)
        .ok_or_else(|| layout_size_error("alignment overflow"))
}

fn hash_node(
    node: &AbiNode,
    targets: &HashMap<String, AbiTarget>,
    hasher: &mut blake3::Hasher,
) -> Result<(), LayoutError> {
    match node {
        AbiNode::Branch {
            offset,
            size,
            alignment,
            children,
        } => {
            hasher.update(&[0]);
            hash_usize(*offset, hasher)?;
            hash_usize(*size, hasher)?;
            hash_usize(*alignment, hasher)?;
            hash_usize(children.len(), hasher)?;
            for (_, child) in children {
                hash_node(child, targets, hasher)?;
            }
        }
        AbiNode::Leaf {
            offset,
            size,
            alignment,
            scalar_type,
            dimensions,
            kind,
        } => {
            hasher.update(&[1]);
            hash_usize(*offset, hasher)?;
            hash_usize(*size, hasher)?;
            hash_usize(*alignment, hasher)?;
            hash_scalar(*scalar_type, hasher);
            hash_dimensions(dimensions.as_ref(), hasher)?;
            match kind {
                LeafKind::Plain => {
                    hasher.update(&[0]);
                }
                LeafKind::Bitmap(widths) => {
                    hasher.update(&[1]);
                    hash_usize(widths.len(), hasher)?;
                    for width in widths {
                        hash_usize(*width, hasher)?;
                    }
                }
                LeafKind::Ref(path) => {
                    hasher.update(&[2]);
                    let target = targets.get(path).ok_or_else(|| {
                        layout_size_error(format!(
                            "ref target '{path}' not found in block. Available fields: [{}]",
                            targets.keys().cloned().collect::<Vec<_>>().join(", ")
                        ))
                    })?;
                    hasher.update(&[match target.kind {
                        TargetKind::Branch => 0,
                        TargetKind::Leaf => 1,
                    }]);
                    hash_usize(target.offset, hasher)?;
                    hash_usize(target.size, hasher)?;
                    hash_usize(target.alignment, hasher)?;
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
