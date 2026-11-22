use super::MAX_RESOLVED_BLOCK_SIZE;
use super::abi::{Abi, ScalarAbi};
use super::block::{Block, Config, Entry};
use super::entry::{EntrySource, LeafEntry, RefSource, RefTarget, SizeSource};
use super::error::{LayoutError, in_field_path};
use super::scalar_type::ScalarType;
use super::settings::ChecksumConfig;
use super::value::ValueSource;
use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedCoordinates {
    pub(crate) offset: usize,
    pub(crate) size: usize,
    pub(crate) alignment: usize,
}

pub(crate) struct ResolvedLayout<'a> {
    pub(crate) root: ResolvedNode<'a>,
    nodes: HashMap<String, ResolvedTarget>,
    abi: Abi,
}

pub(crate) struct PlannedBlock<'a> {
    pub(crate) block: &'a Block,
    pub(crate) resolved: ResolvedLayout<'a>,
    pub(crate) allocation: Range<u64>,
    pub(crate) fields: Vec<PlannedField<'a>>,
}

pub(crate) struct PlannedField<'a> {
    pub(crate) leaf: ResolvedLeafEntry<'a>,
    pub(crate) binding: LeafBinding<'a>,
}

pub(crate) enum LeafBinding<'a> {
    Source,
    Literal(Cow<'a, ValueSource>),
    Ref(u64),
    RefList(Vec<u64>),
    Checksum(&'a ChecksumConfig),
    Fingerprint(&'a str),
}

impl<'a> ResolvedLayout<'a> {
    pub(crate) fn prepare(
        self,
        name: &'a str,
        block: &'a Block,
        config: &'a Config,
    ) -> Result<PlannedBlock<'a>, LayoutError> {
        let settings = &config.mint;
        let total_size = self.total_size();
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

        let mut fields = Vec::new();
        self.plan_node(
            &self.root,
            name,
            block.header.start_address,
            config,
            &mut Vec::new(),
            &mut fields,
        )?;
        Ok(PlannedBlock {
            block,
            resolved: self,
            allocation: output_start..output_end,
            fields,
        })
    }

    pub(crate) fn new(entry: &'a Entry, abi: Abi) -> Result<Self, LayoutError> {
        if matches!(entry, Entry::Leaf(_)) {
            return Err(LayoutError::InvalidLayout(
                "block data must be a table of fields; move the leaf under a field name, such as `value = { type = \"u8\", ... }`".to_owned(),
            ));
        }
        let mut root = collect_entry(entry, abi, &mut Vec::new())?;
        let mut cursor = 0usize;
        let mut nodes = HashMap::new();
        layout_node(&mut root, &mut cursor, &mut Vec::new(), &mut nodes)?;

        if cursor > MAX_RESOLVED_BLOCK_SIZE {
            return Err(LayoutError::InvalidLayout(format!(
                "resolved layout size ({cursor} octets) exceeds Mint's materialized block limit ({MAX_RESOLVED_BLOCK_SIZE} octets)"
            )));
        }

        Ok(Self { root, nodes, abi })
    }

    pub(crate) fn total_size(&self) -> usize {
        self.root.coordinates().size
    }

    pub(crate) fn ref_target(
        &self,
        path: &str,
        index: Option<usize>,
    ) -> Result<ResolvedTarget, LayoutError> {
        self.nodes.get(path).copied().ok_or_else(|| {
            let location = index
                .map(|index| format!(" at index {index}"))
                .unwrap_or_default();
            let mut available = self.nodes.keys().map(String::as_str).collect::<Vec<_>>();
            available.sort_unstable();
            layout_size_error(format!(
                "ref target{location} '{path}' not found in block. Available fields: [{}]",
                available.join(", ")
            ))
        })
    }

    pub(crate) fn abi(&self) -> Abi {
        self.abi
    }

    fn ref_address(
        &self,
        target: &RefTarget,
        start_address: u32,
        index: Option<usize>,
    ) -> Result<u64, LayoutError> {
        match target {
            RefTarget::Address(address) => Ok(*address),
            RefTarget::Path(path) => {
                let target = self.ref_target(path, index)?;
                let target_offset = self
                    .abi
                    .offset_to_address_units(target.coordinates.offset)?;
                u64::from(start_address)
                    .checked_add(target_offset)
                    .ok_or_else(|| {
                        LayoutError::InvalidLayout(format!(
                            "address overflow resolving ref target '{path}'"
                        ))
                    })
            }
        }
    }

    fn plan_node<'b>(
        &self,
        node: &'b ResolvedNode<'a>,
        block_name: &'a str,
        start_address: u32,
        config: &'a Config,
        path: &mut Vec<&'b str>,
        fields: &mut Vec<PlannedField<'a>>,
    ) -> Result<(), LayoutError> {
        match node {
            ResolvedNode::Leaf {
                coordinates,
                scalar_abi,
                leaf,
            } => {
                let field_path = path.join(".");
                let bind = || match &leaf.source {
                    EntrySource::Const(key) => {
                        let value = config.const_value(key)?;
                        leaf.validate_literal("const", &value)?;
                        Ok(LeafBinding::Literal(value))
                    }
                    EntrySource::Value(value) => {
                        leaf.validate_literal("value", value)?;
                        Ok(LeafBinding::Literal(Cow::Borrowed(value)))
                    }
                    EntrySource::Checksum(_) if coordinates.offset == 0 => {
                        Err(LayoutError::InvalidLayout(
                            "Checksum must follow at least one data byte.".to_owned(),
                        ))
                    }
                    EntrySource::Checksum(key) => {
                        config.mint.checksum_config(key).map(LeafBinding::Checksum)
                    }
                    EntrySource::Ref(source) => {
                        self.bind_ref(&field_path, source, leaf, start_address)
                    }
                    EntrySource::Fingerprint(target) => {
                        Ok(LeafBinding::Fingerprint(target.block_name(block_name)))
                    }
                    EntrySource::Name(_) | EntrySource::Bitmap(_) => Ok(LeafBinding::Source),
                };
                let binding = bind().map_err(|error| in_field_path(&field_path, error))?;
                fields.push(PlannedField {
                    leaf: ResolvedLeafEntry {
                        path: field_path,
                        coordinates: *coordinates,
                        scalar_abi: *scalar_abi,
                        leaf,
                    },
                    binding,
                });
            }
            ResolvedNode::Branch { children, .. } => {
                for (name, child) in children {
                    path.push(name.as_str());
                    self.plan_node(child, block_name, start_address, config, path, fields)?;
                    path.pop();
                }
            }
        }
        Ok(())
    }

    fn bind_ref(
        &self,
        path: &str,
        source: &RefSource,
        leaf: &LeafEntry,
        start_address: u32,
    ) -> Result<LeafBinding<'a>, LayoutError> {
        let maximum = match leaf.scalar_type {
            ScalarType::U16 => u64::from(u16::MAX),
            ScalarType::U32 => u64::from(u32::MAX),
            ScalarType::U64 => u64::MAX,
            _ => unreachable!("ref storage was validated during resolution"),
        };

        let resolve = |index, target: &RefTarget| {
            let index = source.is_list().then_some(index);
            let address = self.ref_address(target, start_address, index)?;
            if address > maximum {
                let target = match target {
                    RefTarget::Path(target) => format!("target '{target}'"),
                    RefTarget::Address(target) => format!("literal address 0x{target:X}"),
                };
                let location = index
                    .map(|index| format!(" at index {index}"))
                    .unwrap_or_default();
                return Err(LayoutError::InvalidLayout(format!(
                    "ref '{path}'{location} {target} resolves to address 0x{address:X}, which does not fit storage type {}",
                    leaf.scalar_type
                )));
            }
            Ok(address)
        };
        match source {
            RefSource::Scalar(target) => resolve(0, target).map(LeafBinding::Ref),
            RefSource::List(targets) => targets
                .iter()
                .enumerate()
                .map(|(index, target)| resolve(index, target))
                .collect::<Result<_, _>>()
                .map(LeafBinding::RefList),
        }
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
    },
}

impl ResolvedNode<'_> {
    pub(crate) fn coordinates(&self) -> ResolvedCoordinates {
        match self {
            Self::Branch { coordinates, .. } | Self::Leaf { coordinates, .. } => *coordinates,
        }
    }

    fn is_larger_than_one_octet(&self) -> bool {
        match self {
            Self::Branch { children, .. } => {
                children.len() > 1
                    || children
                        .iter()
                        .any(|(_, child)| child.is_larger_than_one_octet())
            }
            Self::Leaf { coordinates, .. } => coordinates.size > 1,
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

pub(crate) struct ResolvedLeafEntry<'a> {
    pub(crate) path: String,
    pub(crate) coordinates: ResolvedCoordinates,
    pub(crate) scalar_abi: ScalarAbi,
    pub(crate) leaf: &'a LeafEntry,
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
            if let Some(dimensions) = &leaf.size {
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
            let size = leaf_size(scalar_abi, leaf.size.as_ref())?;
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
                    alignment: scalar_abi.alignment,
                },
                scalar_abi,
                leaf,
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
            let mut alignment = children
                .iter()
                .map(|(_, child)| child.coordinates().alignment)
                .max()
                .unwrap_or(1);
            if children.len() > 1
                || children
                    .iter()
                    .any(|(_, child)| child.is_larger_than_one_octet())
            {
                alignment = alignment.max(abi.family().min_aggregate_alignment());
            }
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

fn layout_node(
    node: &mut ResolvedNode<'_>,
    cursor: &mut usize,
    path: &mut Vec<String>,
    nodes: &mut HashMap<String, ResolvedTarget>,
) -> Result<(), LayoutError> {
    *cursor = aligned_offset(*cursor, node.coordinates().alignment)?;
    let offset = *cursor;
    node.coordinates_mut().offset = offset;

    match node {
        ResolvedNode::Leaf { coordinates, .. } => {
            *cursor = cursor
                .checked_add(coordinates.size)
                .ok_or_else(|| layout_size_error("leaf byte count overflow"))?;
        }
        ResolvedNode::Branch {
            coordinates,
            children,
        } => {
            for (name, child) in children {
                path.push(name.clone());
                layout_node(child, cursor, path, nodes)?;
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
