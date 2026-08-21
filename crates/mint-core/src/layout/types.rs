use super::block::{Config, Entry};
use super::entry::{BitmapFieldSource, EntrySource, SizeSource};
use super::error::LayoutError;
use super::resolved::{ResolvedLayout, ResolvedNode};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub(crate) struct TypeMember {
    pub(crate) block: String,
    pub(crate) path: String,
}

impl TypeMember {
    pub(crate) fn display(&self) -> String {
        format!("{}.{}", self.block, self.path)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct NamedType {
    pub(crate) name: String,
    pub(crate) members: Vec<TypeMember>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct NamedTypes {
    types: IndexMap<String, NamedType>,
    by_path: HashMap<(String, String), String>,
}

impl NamedTypes {
    pub(crate) fn get(&self, block: &str, path: &str) -> Option<&str> {
        self.by_path
            .get(&(block.to_owned(), path.to_owned()))
            .map(String::as_str)
    }

    pub(crate) fn get_type(&self, name: &str) -> Option<&NamedType> {
        self.types.get(name)
    }

    pub(crate) fn used_by<'a>(
        &'a self,
        selected: &HashSet<&str>,
    ) -> Result<Vec<&'a NamedType>, LayoutError> {
        let mut used = IndexMap::new();
        for named in self.types.values() {
            if named
                .members
                .iter()
                .any(|member| selected.contains(member.block.as_str()))
            {
                mark_used(self, named, &mut used)?;
            }
        }
        topo_sort(self, used)
    }
}

pub(crate) fn validate(config: &Config) -> Result<NamedTypes, LayoutError> {
    if config.mint.types.is_empty() {
        return Ok(NamedTypes::default());
    }

    let mut resolved = HashMap::new();
    for (name, block) in &config.blocks {
        resolved.insert(
            name.as_str(),
            ResolvedLayout::new(&block.data, config.mint.abi)?,
        );
    }

    let mut named = NamedTypes::default();
    for (type_name, paths) in &config.mint.types {
        super::validate_c_identifier(type_name, "type").map_err(LayoutError::InvalidLayout)?;
        if let Some(block) = type_name.strip_suffix("_t")
            && config.blocks.contains_key(block)
        {
            return Err(LayoutError::InvalidLayout(format!(
                "named type '{type_name}' collides with generated typedef '{type_name}' for block '{block}'"
            )));
        }

        if paths.is_empty() {
            return Err(LayoutError::InvalidLayout(format!(
                "named type '{type_name}' must list at least one aggregate path"
            )));
        }

        let mut members = Vec::with_capacity(paths.len());
        let mut seen_paths = HashSet::new();
        for raw in paths {
            let member = parse_type_path(raw)?;
            if !seen_paths.insert(member.display()) {
                return Err(LayoutError::InvalidLayout(format!(
                    "named type '{type_name}' lists path '{}' more than once",
                    member.display()
                )));
            }
            let key = (member.block.clone(), member.path.clone());
            if let Some(existing) = named.by_path.get(&key) {
                return Err(LayoutError::InvalidLayout(format!(
                    "path '{}' is assigned to both '{existing}' and '{type_name}'",
                    member.display()
                )));
            }
            let layout = resolved.get(member.block.as_str()).ok_or_else(|| {
                LayoutError::InvalidLayout(format!(
                    "named type '{type_name}' path '{}' names unknown block '{}'. Available blocks: [{}]",
                    member.display(),
                    member.block,
                    config.blocks.keys().cloned().collect::<Vec<_>>().join(", ")
                ))
            })?;
            match layout.node(&member.path) {
                None => {
                    return Err(LayoutError::InvalidLayout(format!(
                        "named type '{type_name}' path '{}' not found. Available aggregates: [{}]",
                        member.display(),
                        available_aggregates(config).join(", ")
                    )));
                }
                Some(ResolvedNode::Leaf { .. }) => {
                    return Err(LayoutError::InvalidLayout(format!(
                        "named type '{type_name}' path '{}' is a leaf; named types can only label nested aggregates",
                        member.display()
                    )));
                }
                Some(ResolvedNode::Branch { .. }) => {}
            }
            named.by_path.insert(key, type_name.clone());
            members.push(member);
        }

        let first = layout_node(&resolved, &members[0])?;
        for other in members.iter().skip(1) {
            let node = layout_node(&resolved, other)?;
            if let Some(difference) = shape_mismatch(first, node, "") {
                return Err(LayoutError::InvalidLayout(format!(
                    "named type '{type_name}': '{}' and '{}' have different shapes{difference}",
                    members[0].display(),
                    other.display()
                )));
            }
        }

        named.types.insert(
            type_name.clone(),
            NamedType {
                name: type_name.clone(),
                members,
            },
        );
    }

    Ok(named)
}

pub(crate) fn collect(config: &Config) -> Result<NamedTypes, LayoutError> {
    validate(config)
}

pub(crate) fn entry_at<'a>(root: &'a Entry, path: &str) -> Option<&'a Entry> {
    if path.is_empty() {
        return Some(root);
    }
    let mut entry = root;
    for segment in path.split('.') {
        let Entry::Branch(children) = entry else {
            return None;
        };
        entry = children.get(segment)?;
    }
    Some(entry)
}

fn parse_type_path(raw: &str) -> Result<TypeMember, LayoutError> {
    let Some((block, path)) = raw.split_once('.') else {
        return Err(LayoutError::InvalidLayout(format!(
            "named type path '{raw}' must be 'block.aggregate'; named types can only label nested aggregates"
        )));
    };
    if block.is_empty() || path.is_empty() {
        return Err(LayoutError::InvalidLayout(format!(
            "named type path '{raw}' must be 'block.aggregate'; named types can only label nested aggregates"
        )));
    }
    Ok(TypeMember {
        block: block.to_owned(),
        path: path.to_owned(),
    })
}

fn layout_node<'a>(
    resolved: &'a HashMap<&str, ResolvedLayout<'a>>,
    member: &TypeMember,
) -> Result<&'a ResolvedNode<'a>, LayoutError> {
    resolved
        .get(member.block.as_str())
        .and_then(|layout| layout.node(&member.path))
        .ok_or_else(|| {
            LayoutError::InvalidLayout(format!(
                "named type path '{}' disappeared after resolution",
                member.display()
            ))
        })
}

fn available_aggregates(config: &Config) -> Vec<String> {
    let mut out = Vec::new();
    for (block_name, block) in &config.blocks {
        collect_aggregates(block_name, &block.data, &mut Vec::new(), &mut out);
    }
    out
}

fn collect_aggregates(block: &str, entry: &Entry, path: &mut Vec<String>, out: &mut Vec<String>) {
    let Entry::Branch(children) = entry else {
        return;
    };
    if !path.is_empty() {
        out.push(format!("{block}.{}", path.join(".")));
    }
    for (name, child) in children {
        path.push(name.clone());
        collect_aggregates(block, child, path, out);
        path.pop();
    }
}

fn mark_used<'a>(
    named: &'a NamedTypes,
    current: &'a NamedType,
    used: &mut IndexMap<&'a str, &'a NamedType>,
) -> Result<(), LayoutError> {
    if used.contains_key(current.name.as_str()) {
        return Ok(());
    }
    used.insert(current.name.as_str(), current);
    for nested in nested_types(named, current)? {
        mark_used(named, nested, used)?;
    }
    Ok(())
}

fn nested_types<'a>(
    named: &'a NamedTypes,
    current: &'a NamedType,
) -> Result<Vec<&'a NamedType>, LayoutError> {
    let first = current.members.first().ok_or_else(|| {
        LayoutError::InvalidLayout(format!(
            "named type '{}' has no members after validation",
            current.name
        ))
    })?;
    let prefix = format!("{}.", first.path);
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for ((block, path), type_name) in &named.by_path {
        if block != &first.block || path == &first.path || !path.starts_with(&prefix) {
            continue;
        }
        if seen.insert(type_name.clone())
            && let Some(nested) = named.get_type(type_name)
        {
            found.push(nested);
        }
    }
    Ok(found)
}

fn topo_sort<'a>(
    named: &'a NamedTypes,
    used: IndexMap<&'a str, &'a NamedType>,
) -> Result<Vec<&'a NamedType>, LayoutError> {
    let mut remaining: IndexMap<&str, HashSet<&str>> = IndexMap::new();
    for (name, typed) in &used {
        let deps = nested_types(named, typed)?
            .into_iter()
            .map(|dep| dep.name.as_str())
            .filter(|dep| used.contains_key(*dep) && *dep != *name)
            .collect();
        remaining.insert(*name, deps);
    }

    let mut ordered = Vec::with_capacity(used.len());
    while !remaining.is_empty() {
        let ready = remaining
            .iter()
            .find(|(_, deps)| deps.is_empty())
            .map(|(name, _)| *name);
        let Some(name) = ready else {
            return Err(LayoutError::InvalidLayout(
                "named types form a dependency cycle".to_owned(),
            ));
        };
        remaining.shift_remove(name);
        for deps in remaining.values_mut() {
            deps.remove(name);
        }
        ordered.push(used[name]);
    }
    Ok(ordered)
}

fn shape_mismatch(left: &ResolvedNode<'_>, right: &ResolvedNode<'_>, rel: &str) -> Option<String> {
    let at = |member: &str| {
        if rel.is_empty() {
            if member.is_empty() {
                String::new()
            } else {
                format!(" at '{member}'")
            }
        } else if member.is_empty() {
            format!(" at '{rel}'")
        } else {
            format!(" at '{rel}.{member}'")
        }
    };

    match (left, right) {
        (
            ResolvedNode::Branch {
                coordinates: left_coords,
                children: left_children,
            },
            ResolvedNode::Branch {
                coordinates: right_coords,
                children: right_children,
            },
        ) => {
            if left_children.len() != right_children.len()
                || left_children
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .ne(right_children.iter().map(|(name, _)| name.as_str()))
            {
                return Some(format!(
                    "{}: member names or order [{}] vs [{}]",
                    at(""),
                    child_names(left_children),
                    child_names(right_children)
                ));
            }
            for ((name, left_child), (_, right_child)) in left_children.iter().zip(right_children) {
                let left_rel = left_child.coordinates().offset - left_coords.offset;
                let right_rel = right_child.coordinates().offset - right_coords.offset;
                if left_rel != right_rel {
                    return Some(format!(
                        "{}: relative offset {left_rel} vs {right_rel}",
                        at(name)
                    ));
                }
                let child_rel = if rel.is_empty() {
                    name.clone()
                } else {
                    format!("{rel}.{name}")
                };
                if let Some(difference) = shape_mismatch(left_child, right_child, &child_rel) {
                    return Some(difference);
                }
            }
            if left_coords.size != right_coords.size
                || left_coords.alignment != right_coords.alignment
            {
                return Some(format!(
                    "{}: size/alignment {}/{} vs {}/{}",
                    at(""),
                    left_coords.size,
                    left_coords.alignment,
                    right_coords.size,
                    right_coords.alignment
                ));
            }
            None
        }
        (
            ResolvedNode::Leaf {
                coordinates: left_coords,
                leaf: left_leaf,
                dimensions: left_dims,
                ..
            },
            ResolvedNode::Leaf {
                coordinates: right_coords,
                leaf: right_leaf,
                dimensions: right_dims,
                ..
            },
        ) => {
            if left_leaf.scalar_type != right_leaf.scalar_type {
                return Some(format!(
                    "{}: type {} vs {}",
                    at(""),
                    left_leaf.scalar_type,
                    right_leaf.scalar_type
                ));
            }
            if !dimensions_equal(left_dims.as_ref(), right_dims.as_ref()) {
                return Some(format!(
                    "{}: array dimensions {} vs {}",
                    at(""),
                    format_dimensions(left_dims.as_ref()),
                    format_dimensions(right_dims.as_ref())
                ));
            }
            if left_coords.size != right_coords.size
                || left_coords.alignment != right_coords.alignment
            {
                return Some(format!(
                    "{}: size/alignment {}/{} vs {}/{}",
                    at(""),
                    left_coords.size,
                    left_coords.alignment,
                    right_coords.size,
                    right_coords.alignment
                ));
            }
            match (&left_leaf.source, &right_leaf.source) {
                (EntrySource::Bitmap(left_fields), EntrySource::Bitmap(right_fields)) => {
                    let left_shape = bitmap_shape(left_fields);
                    let right_shape = bitmap_shape(right_fields);
                    if left_shape != right_shape {
                        return Some(format!("{}: bitmap regions differ", at("")));
                    }
                    None
                }
                (EntrySource::Bitmap(_), _) | (_, EntrySource::Bitmap(_)) => {
                    Some(format!("{}: bitmap vs non-bitmap storage", at("")))
                }
                _ => None,
            }
        }
        (ResolvedNode::Branch { .. }, ResolvedNode::Leaf { .. })
        | (ResolvedNode::Leaf { .. }, ResolvedNode::Branch { .. }) => {
            Some(format!("{}: nested aggregate vs leaf", at("")))
        }
    }
}

fn child_names(children: &[(String, ResolvedNode<'_>)]) -> String {
    children
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn dimensions_equal(left: Option<&SizeSource>, right: Option<&SizeSource>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(SizeSource::OneD(a)), Some(SizeSource::OneD(b))) => a == b,
        (Some(SizeSource::TwoD(a)), Some(SizeSource::TwoD(b))) => a == b,
        _ => false,
    }
}

fn format_dimensions(dimensions: Option<&SizeSource>) -> String {
    match dimensions {
        None => "scalar".to_owned(),
        Some(SizeSource::OneD(length)) => format!("[{length}]"),
        Some(SizeSource::TwoD([rows, columns])) => format!("[{rows}][{columns}]"),
    }
}

fn bitmap_shape(fields: &[super::entry::BitmapField]) -> Vec<(usize, Option<&str>)> {
    fields
        .iter()
        .map(|field| {
            let name = match &field.source {
                BitmapFieldSource::Name(name) => Some(name.as_str()),
                BitmapFieldSource::Value(_) => None,
            };
            (field.bits, name)
        })
        .collect()
}
