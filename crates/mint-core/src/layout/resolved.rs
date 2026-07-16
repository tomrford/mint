use super::block::{Block, Entry};
use super::entry::{EntrySource, FingerprintTarget, LeafEntry, SizeSource};
use super::error::{LayoutError, in_field_path};
use super::settings::MintConfig;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedCoordinates {
    pub offset: usize,
    pub size: usize,
    pub alignment: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedLeaf<'a> {
    pub path: &'a str,
    pub coordinates: ResolvedCoordinates,
}

pub struct ResolvedLayout<'a> {
    pub(crate) root: ResolvedNode<'a>,
    leaves: Vec<ResolvedLeafEntry<'a>>,
    nodes: HashMap<String, ResolvedTarget>,
    total_size: usize,
    pub(crate) fingerprint_targets: Vec<&'a FingerprintTarget>,
}

pub(crate) fn validate_static<'a>(
    block: &'a Block,
    settings: &MintConfig,
) -> Result<ResolvedLayout<'a>, LayoutError> {
    let resolved = ResolvedLayout::new(&block.data)?;
    let total_size = resolved.total_size();
    if total_size > block.header.length as usize {
        return Err(LayoutError::InvalidLayout(format!(
            "resolved layout size ({total_size} bytes) exceeds configured block length ({} bytes)",
            block.header.length
        )));
    }
    for (path, _, leaf) in resolved.emission_leaves() {
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
            EntrySource::Checksum(name) => settings.checksum_config(name).map(|_| ()),
            _ => Ok(()),
        };
        result.map_err(|error| in_field_path(path, error))?;
    }
    Ok(resolved)
}

impl<'a> ResolvedLayout<'a> {
    pub fn new(entry: &'a Entry) -> Result<Self, LayoutError> {
        let mut fingerprint_targets = Vec::new();
        let mut root = collect_entry(entry, &mut Vec::new(), &mut fingerprint_targets)?;
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
            fingerprint_targets,
        })
    }

    pub fn coordinates(&self, path: &str) -> Option<ResolvedCoordinates> {
        self.nodes.get(path).map(|target| target.coordinates)
    }

    pub fn total_size(&self) -> usize {
        self.total_size
    }

    pub fn leaves(&self) -> impl ExactSizeIterator<Item = ResolvedLeaf<'_>> {
        self.leaves.iter().map(|leaf| ResolvedLeaf {
            path: &leaf.path,
            coordinates: leaf.coordinates,
        })
    }

    pub(crate) fn emission_leaves(
        &self,
    ) -> impl ExactSizeIterator<Item = (&str, ResolvedCoordinates, &LeafEntry)> {
        self.leaves
            .iter()
            .map(|leaf| (leaf.path.as_str(), leaf.coordinates, leaf.leaf))
    }

    pub(crate) fn target(&self, path: &str) -> Option<ResolvedTarget> {
        self.nodes.get(path).copied()
    }
}

pub(crate) enum ResolvedNode<'a> {
    Branch {
        coordinates: ResolvedCoordinates,
        children: Vec<(String, ResolvedNode<'a>)>,
    },
    Leaf {
        coordinates: ResolvedCoordinates,
        leaf: &'a LeafEntry,
        dimensions: Option<SizeSource>,
        kind: ResolvedLeafKind<'a>,
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

pub(crate) enum ResolvedLeafKind<'a> {
    Plain,
    Bitmap(Vec<usize>),
    Ref(&'a str),
}

struct ResolvedLeafEntry<'a> {
    path: String,
    coordinates: ResolvedCoordinates,
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
    path: &mut Vec<String>,
    fingerprint_targets: &mut Vec<&'a FingerprintTarget>,
) -> Result<ResolvedNode<'a>, LayoutError> {
    match entry {
        Entry::Leaf(leaf) => {
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
            let size = leaf_size(leaf, dimensions.as_ref())?;
            let kind = match &leaf.source {
                EntrySource::Bitmap(fields) => {
                    leaf.validate_bitmap(fields)?;
                    ResolvedLeafKind::Bitmap(fields.iter().map(|field| field.bits).collect())
                }
                EntrySource::Ref(target) => {
                    leaf.validate_ref(target)?;
                    ResolvedLeafKind::Ref(target)
                }
                EntrySource::Checksum(_) => {
                    leaf.validate_checksum_storage()?;
                    ResolvedLeafKind::Plain
                }
                EntrySource::Fingerprint(target) => {
                    leaf.validate_fingerprint()?;
                    fingerprint_targets.push(target);
                    ResolvedLeafKind::Plain
                }
                _ => ResolvedLeafKind::Plain,
            };
            Ok(ResolvedNode::Leaf {
                coordinates: ResolvedCoordinates {
                    offset: 0,
                    size,
                    alignment: leaf.get_alignment(),
                },
                leaf,
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
            coordinates, leaf, ..
        } => {
            *cursor = cursor
                .checked_add(coordinates.size)
                .ok_or_else(|| layout_size_error("leaf byte count overflow"))?;
            leaves.push(ResolvedLeafEntry {
                path: path.join("."),
                coordinates: *coordinates,
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

fn layout_size_error(message: impl Into<String>) -> LayoutError {
    LayoutError::InvalidLayout(message.into())
}
