use super::MAX_RESOLVED_BLOCK_SIZE;
use super::abi::{Abi, ScalarAbi};
use super::block::{Block, Entry};
use super::entry::{EntrySource, LeafEntry, SizeSource};
use super::error::{LayoutError, in_field_path};
use super::scalar_type::ScalarType;
use super::settings::MintConfig;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedCoordinates {
    pub(crate) offset: usize,
    pub(crate) size: usize,
    pub(crate) alignment: usize,
}

pub(crate) struct ResolvedLayout<'a> {
    pub(crate) root: ResolvedNode<'a>,
    leaves: Vec<ResolvedLeafEntry<'a>>,
    nodes: HashMap<String, ResolvedTarget>,
    total_size: usize,
    abi: Abi,
}

pub(crate) fn validate_static<'a>(
    block: &'a Block,
    settings: &MintConfig,
) -> Result<ResolvedLayout<'a>, LayoutError> {
    let resolved = ResolvedLayout::new(&block.data, settings.abi)?;
    let total_size = resolved.total_size();
    if total_size > MAX_RESOLVED_BLOCK_SIZE {
        return Err(LayoutError::InvalidLayout(format!(
            "resolved layout size ({total_size} octets) exceeds Mint's materialized block limit ({MAX_RESOLVED_BLOCK_SIZE} octets)"
        )));
    }
    if total_size > block.header.length as usize {
        return Err(LayoutError::InvalidLayout(format!(
            "resolved layout size ({total_size} octets) exceeds configured block length ({} octets)",
            block.header.length
        )));
    }

    let unit_octets = settings.abi.address_unit_octets();
    if !(block.header.length as usize).is_multiple_of(unit_octets) {
        return Err(LayoutError::InvalidLayout(format!(
            "configured block length ({} octets) is not divisible by the {}-octet addressable unit of ABI '{}'",
            block.header.length,
            unit_octets,
            settings.abi.name()
        )));
    }
    if !total_size.is_multiple_of(unit_octets) {
        return Err(LayoutError::InvalidLayout(format!(
            "resolved layout size ({total_size} octets) is not divisible by the {}-octet addressable unit of ABI '{}'",
            unit_octets,
            settings.abi.name()
        )));
    }

    let output_start = u64::from(block.header.start_address)
        .checked_mul(unit_octets as u64)
        .ok_or_else(|| {
            LayoutError::InvalidLayout("block output start address overflow".to_owned())
        })?;
    let output_end = output_start + u64::from(block.header.length);
    if output_end > u64::from(u32::MAX) + 1 {
        return Err(LayoutError::InvalidLayout(format!(
            "block octet-addressed output range 0x{output_start:08X}-0x{:08X} exceeds the 32-bit address space",
            output_end.saturating_sub(1)
        )));
    }
    for (path, coordinates, _, leaf) in resolved.emission_leaves() {
        let size = leaf.size().map_err(|error| in_field_path(path, error))?;
        let result = match &leaf.source {
            EntrySource::Const(name) => leaf
                .validate_const(name, &settings.consts, size.as_ref())
                .map(|_| ()),
            EntrySource::Value(_) if matches!(size, Some(SizeSource::TwoD(_))) => {
                Err(LayoutError::InvalidLayout(
                    "2D arrays within the layout file are not supported.".to_owned(),
                ))
            }
            EntrySource::Checksum(_) if coordinates.offset == 0 => Err(LayoutError::InvalidLayout(
                "Checksum must follow at least one data byte.".to_owned(),
            )),
            EntrySource::Checksum(name) => settings.checksum_config(name).map(|_| ()),
            EntrySource::Ref(target) => {
                validate_ref_address(path, target, leaf, &resolved, block.header.start_address)
            }
            _ => Ok(()),
        };
        result.map_err(|error| in_field_path(path, error))?;
    }
    Ok(resolved)
}

impl<'a> ResolvedLayout<'a> {
    pub(crate) fn new(entry: &'a Entry, abi: Abi) -> Result<Self, LayoutError> {
        let mut root = collect_entry(entry, abi, &mut Vec::new())?;
        let mut cursor = 0usize;
        let mut leaves = Vec::new();
        let mut nodes = HashMap::new();
        layout_node(
            &mut root,
            &mut cursor,
            &mut Vec::new(),
            &mut leaves,
            &mut nodes,
        )?;

        for leaf in &leaves {
            if let EntrySource::Ref(path) = &leaf.leaf.source
                && !nodes.contains_key(path)
            {
                return Err(layout_size_error(format!(
                    "ref target '{path}' not found in block. Available fields: [{}]",
                    nodes.keys().cloned().collect::<Vec<_>>().join(", ")
                )));
            }
        }

        Ok(Self {
            root,
            leaves,
            nodes,
            total_size: cursor,
            abi,
        })
    }

    pub(crate) fn coordinates(&self, path: &str) -> Option<ResolvedCoordinates> {
        self.nodes.get(path).map(|target| target.coordinates)
    }

    pub(crate) fn total_size(&self) -> usize {
        self.total_size
    }

    pub(crate) fn emission_leaves(
        &self,
    ) -> impl ExactSizeIterator<Item = (&str, ResolvedCoordinates, ScalarAbi, &LeafEntry)> {
        self.leaves.iter().map(|leaf| {
            (
                leaf.path.as_str(),
                leaf.coordinates,
                leaf.scalar_abi,
                leaf.leaf,
            )
        })
    }

    pub(crate) fn target(&self, path: &str) -> Option<ResolvedTarget> {
        self.nodes.get(path).copied()
    }

    pub(crate) fn abi(&self) -> Abi {
        self.abi
    }
}

pub(crate) enum ResolvedNode<'a> {
    Branch {
        coordinates: ResolvedCoordinates,
        children: Vec<(String, ResolvedNode<'a>)>,
    },
    Leaf {
        coordinates: ResolvedCoordinates,
        scalar_abi: ScalarAbi,
        leaf: &'a LeafEntry,
        dimensions: Option<SizeSource>,
    },
}

impl ResolvedNode<'_> {
    fn coordinates(&self) -> ResolvedCoordinates {
        match self {
            Self::Branch { coordinates, .. } | Self::Leaf { coordinates, .. } => *coordinates,
        }
    }

    fn coordinates_mut(&mut self) -> &mut ResolvedCoordinates {
        match self {
            Self::Branch { coordinates, .. } | Self::Leaf { coordinates, .. } => coordinates,
        }
    }

    fn target(&self) -> ResolvedTarget {
        ResolvedTarget {
            coordinates: self.coordinates(),
            kind: match self {
                Self::Branch { .. } => TargetKind::Branch,
                Self::Leaf { .. } => TargetKind::Leaf,
            },
        }
    }
}

struct ResolvedLeafEntry<'a> {
    path: String,
    coordinates: ResolvedCoordinates,
    scalar_abi: ScalarAbi,
    leaf: &'a LeafEntry,
}

#[derive(Clone, Copy)]
pub(crate) struct ResolvedTarget {
    pub(crate) coordinates: ResolvedCoordinates,
    pub(crate) kind: TargetKind,
}

#[derive(Clone, Copy)]
pub(crate) enum TargetKind {
    Branch,
    Leaf,
}

fn collect_entry<'a>(
    entry: &'a Entry,
    abi: Abi,
    path: &mut Vec<String>,
) -> Result<ResolvedNode<'a>, LayoutError> {
    match entry {
        Entry::Leaf(leaf) => {
            let scalar_abi = abi.scalar(leaf.scalar_type)?;
            let dimensions = leaf.size()?;
            if let Some(dimensions) = &dimensions {
                let zero = match dimensions {
                    SizeSource::OneD(length) => *length == 0,
                    SizeSource::TwoD(extents) => extents.contains(&0),
                };
                if zero {
                    return Err(LayoutError::InvalidLayout(format!(
                        "array '{}' has a zero extent",
                        path.join(".")
                    )));
                }
            }
            let size = leaf_size(scalar_abi, dimensions.as_ref())?;
            match &leaf.source {
                EntrySource::Bitmap(fields) => {
                    leaf.validate_bitmap(fields, scalar_abi)?;
                }
                EntrySource::Ref(target) => {
                    leaf.validate_ref(target)?;
                }
                EntrySource::Checksum(_) => {
                    leaf.validate_checksum_storage()?;
                }
                EntrySource::Fingerprint(_) => {
                    leaf.validate_fingerprint()?;
                }
                _ => {}
            }
            Ok(ResolvedNode::Leaf {
                coordinates: ResolvedCoordinates {
                    offset: 0,
                    size,
                    alignment: leaf.get_alignment(scalar_abi),
                },
                scalar_abi,
                leaf,
                dimensions,
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
                let child = collect_entry(child, abi, path)?;
                path.pop();
                children.push((name.clone(), child));
            }
            let alignment = children
                .iter()
                .map(|(_, child)| child.coordinates().alignment)
                .max()
                .unwrap_or(1);
            Ok(ResolvedNode::Branch {
                coordinates: ResolvedCoordinates {
                    offset: 0,
                    size: 0,
                    alignment,
                },
                children,
            })
        }
    }
}

fn validate_ref_address(
    path: &str,
    target: &str,
    leaf: &LeafEntry,
    resolved: &ResolvedLayout<'_>,
    start_address: u32,
) -> Result<(), LayoutError> {
    let target_offset = resolved.coordinates(target).ok_or_else(|| {
        LayoutError::InvalidLayout(format!(
            "ref '{path}' target '{target}' disappeared after resolution"
        ))
    })?;
    let target_offset = resolved
        .abi()
        .offset_to_address_units(target_offset.offset)?;
    let address = u64::from(start_address)
        .checked_add(target_offset)
        .ok_or_else(|| {
            LayoutError::InvalidLayout(format!(
                "address overflow resolving ref '{path}' to target '{target}'"
            ))
        })?;
    let maximum = match leaf.scalar_type {
        ScalarType::U16 => u64::from(u16::MAX),
        ScalarType::U32 => u64::from(u32::MAX),
        ScalarType::U64 => u64::MAX,
        _ => unreachable!("ref storage was validated during resolution"),
    };
    if address > maximum {
        return Err(LayoutError::InvalidLayout(format!(
            "ref '{path}' target '{target}' resolves to address 0x{address:X}, which does not fit storage type {}",
            leaf.scalar_type
        )));
    }
    Ok(())
}

fn layout_node<'a>(
    node: &mut ResolvedNode<'a>,
    cursor: &mut usize,
    path: &mut Vec<String>,
    leaves: &mut Vec<ResolvedLeafEntry<'a>>,
    nodes: &mut HashMap<String, ResolvedTarget>,
) -> Result<(), LayoutError> {
    *cursor = aligned_offset(*cursor, node.coordinates().alignment)?;
    let offset = *cursor;
    node.coordinates_mut().offset = offset;

    match node {
        ResolvedNode::Leaf {
            coordinates,
            scalar_abi,
            leaf,
            ..
        } => {
            *cursor = cursor
                .checked_add(coordinates.size)
                .ok_or_else(|| layout_size_error("leaf byte count overflow"))?;
            leaves.push(ResolvedLeafEntry {
                path: path.join("."),
                coordinates: *coordinates,
                scalar_abi: *scalar_abi,
                leaf,
            });
        }
        ResolvedNode::Branch {
            coordinates,
            children,
        } => {
            for (name, child) in children {
                path.push(name.clone());
                layout_node(child, cursor, path, leaves, nodes)?;
                nodes.insert(path.join("."), child.target());
                path.pop();
            }
            *cursor = aligned_offset(*cursor, coordinates.alignment)?;
            coordinates.size = *cursor - offset;
        }
    }
    Ok(())
}

fn leaf_size(scalar_abi: ScalarAbi, dimensions: Option<&SizeSource>) -> Result<usize, LayoutError> {
    let elements = match dimensions {
        None => 1,
        Some(SizeSource::OneD(length)) => *length,
        Some(SizeSource::TwoD([rows, columns])) => rows
            .checked_mul(*columns)
            .ok_or_else(|| layout_size_error("array element count overflow"))?,
    };
    let element_size = if dimensions.is_some() {
        scalar_abi.array_stride
    } else {
        scalar_abi.storage_size
    };
    elements
        .checked_mul(element_size)
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

fn layout_size_error(message: impl Into<String>) -> LayoutError {
    LayoutError::InvalidLayout(message.into())
}
