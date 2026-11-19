use crate::error::MintError;
use crate::layout::abi::{Abi, ScalarAbi};
use crate::layout::entry::{BitmapFieldSource, EntrySource, LeafEntry, SizeSource};
use crate::layout::error::LayoutError;
use crate::layout::resolved::{ResolvedLayout, ResolvedNode};
use crate::layout::selection::{BlockSelector, LayoutSelection, PreparedSelection, SelectedBlock};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

/// Generate a complete C11 header for the selected layout blocks.
pub fn generate(blocks: &[BlockSelector]) -> Result<String, MintError> {
    let selection = LayoutSelection::files(blocks)?;
    let prepared =
        selection.prepare(|block_name, layout_file, source| MintError::InHeaderBlock {
            block_name,
            layout_file,
            source,
        })?;
    validate_common_c_layout(&prepared)?;
    let blocks = prepared.blocks();
    let mut rendered = Vec::with_capacity(blocks.len());
    let mut names = NameRegistry::default();
    let mut guard = String::from("MINT");
    let mut macros = String::new();
    let mut includes: Option<(&Path, &[String])> = None;

    for selected in blocks {
        let configured_includes = &selected.layout.config.mint.header.includes;
        if let Some((first_path, first_includes)) = &includes {
            if *first_includes != configured_includes.as_slice() {
                return Err(header_error(format!(
                    "[mint.header].includes must match across selected layouts: '{}' and '{}' have different include lists",
                    first_path.display(),
                    selected.path.display()
                ))
                .into());
            }
        } else {
            for include in configured_includes {
                validate_include(include).map_err(|message| {
                    header_error(format!("{}: {message}", selected.path.display()))
                })?;
            }
            includes = Some((selected.path, configured_includes));
        }
        let result = render_block(&selected, &mut names, &mut macros);
        let block_output = result.map_err(|source| MintError::InHeaderBlock {
            block_name: selected.name.to_owned(),
            layout_file: selected.path.display().to_string(),
            source: Box::new(source.into()),
        })?;
        guard.push('_');
        guard.push_str(&block_output.macro_prefix);
        rendered.push(block_output);
    }

    guard.push_str("_H");
    if names.members.contains("CHAR_BIT") {
        return Err(
            header_error("field 'CHAR_BIT' collides with the required CHAR_BIT macro").into(),
        );
    }
    for member in &names.members {
        if member == &guard || names.macros.contains_key(member) {
            return Err(
                header_error(format!("field '{member}' collides with a generated macro")).into(),
            );
        }
    }
    let mut output = format!("#ifndef {guard}\n#define {guard}\n");
    if let Some((_, includes)) = includes
        && !includes.is_empty()
    {
        output.push('\n');
        for include in includes {
            output.push_str(&format!("#include {include}\n"));
        }
    }

    output.push_str(&macros);

    for block in &rendered {
        output.push('\n');
        output.push_str(&block.typedef);
    }

    for block in &rendered {
        output.push('\n');
        output.push_str(&block.assertions);
    }

    output.push_str(&format!("\n#endif /* {guard} */\n"));
    Ok(output)
}

fn validate_common_c_layout(prepared: &PreparedSelection<'_>) -> Result<(), LayoutError> {
    let mut blocks = prepared.blocks();
    let Some(first) = blocks.next() else {
        return Ok(());
    };
    let rules = |abi: Abi| (abi.family(), abi.address_unit_bits());
    let first_rules = rules(first.plan.resolved.abi());
    if blocks.all(|selected| rules(selected.plan.resolved.abi()) == first_rules) {
        return Ok(());
    }

    for (index, abi) in Abi::ALL.into_iter().enumerate() {
        if Abi::ALL[..index]
            .iter()
            .any(|&other| rules(abi) == rules(other))
        {
            continue;
        }
        if prepared.blocks().all(|selected| {
            let expected = &selected.plan.resolved;
            rules(expected.abi()) == rules(abi)
                || ResolvedLayout::new(&selected.plan.block.data, abi)
                    .is_ok_and(|candidate| expected.same_c_layout(&candidate))
        }) {
            return Ok(());
        }
    }

    Err(header_error(
        "selected blocks require incompatible C ABI layouts",
    ))
}

fn validate_include(include: &str) -> Result<(), String> {
    let name = include
        .strip_prefix('"')
        .and_then(|name| name.strip_suffix('"'))
        .or_else(|| {
            include
                .strip_prefix('<')
                .and_then(|name| name.strip_suffix('>'))
        });
    if name.is_some_and(|name| {
        !name.is_empty()
            && !name
                .chars()
                .any(|ch| ch.is_control() || matches!(ch, '<' | '>' | '"' | '\\'))
    }) {
        Ok(())
    } else {
        Err(format!(
            "invalid [mint.header].includes entry {include:?}: expected a non-empty header name enclosed in double quotes or angle brackets, without control characters, backslashes or nested delimiters"
        ))
    }
}

struct RenderedBlock {
    macro_prefix: String,
    typedef: String,
    assertions: String,
}

#[derive(Default)]
struct NameRegistry {
    block_prefixes: HashMap<String, String>,
    macros: HashMap<String, String>,
    members: HashSet<String>,
}

impl NameRegistry {
    fn add_block_prefix(&mut self, prefix: &str, block: &str) -> Result<(), LayoutError> {
        if let Some(existing) = self
            .block_prefixes
            .insert(prefix.to_owned(), block.to_owned())
        {
            return Err(header_error(format!(
                "block names '{existing}' and '{block}' both convert to macro prefix '{prefix}'"
            )));
        }
        Ok(())
    }

    fn add_macro(&mut self, name: &str, origin: String) -> Result<(), LayoutError> {
        if let Some(existing) = self.macros.insert(name.to_owned(), origin.clone()) {
            return Err(header_error(format!(
                "'{existing}' and '{origin}' both generate macro '{name}'"
            )));
        }
        Ok(())
    }
}

fn render_block(
    selected: &SelectedBlock<'_>,
    names: &mut NameRegistry,
    macros: &mut String,
) -> Result<RenderedBlock, LayoutError> {
    let block_name = selected.name;
    let block = selected.plan.block;
    let typedef_name = format!("{block_name}_t");
    let macro_prefix = to_upper_snake(block_name, "block name")?;
    names.add_block_prefix(&macro_prefix, block_name)?;

    let root = &selected.plan.resolved.root;

    let ResolvedNode::Branch {
        children: source, ..
    } = root
    else {
        unreachable!("resolution rejects block data that is not a table")
    };

    add_macro(
        names,
        macros,
        format!("{macro_prefix}_START_ADDRESS"),
        format!("0x{:X}u", block.header.start_address),
        format!("block header '{block_name}.start_address'"),
        true,
    )?;
    add_macro(
        names,
        macros,
        format!("{macro_prefix}_LENGTH"),
        format!("0x{:X}u", block.header.length),
        format!("block header '{block_name}.length'"),
        false,
    )?;

    let mut path = Vec::new();
    collect_macros(
        source,
        block_name,
        &macro_prefix,
        &selected.layout.fingerprints,
        names,
        &mut path,
        macros,
    )?;

    let mut typedef = String::from("typedef struct {\n");
    render_fields(source, 1, &macro_prefix, &mut path, &mut typedef)?;
    typedef.push_str(&format!("}} {typedef_name};\n"));

    let assertions = render_layout_assertions(block_name, &typedef_name, root);

    Ok(RenderedBlock {
        macro_prefix,
        typedef,
        assertions,
    })
}

#[allow(clippy::too_many_arguments)]
fn collect_macros(
    fields: &[(String, ResolvedNode<'_>)],
    block_name: &str,
    block_prefix: &str,
    fingerprints: &IndexMap<&str, u64>,
    names: &mut NameRegistry,
    path: &mut Vec<String>,
    output: &mut String,
) -> Result<(), LayoutError> {
    for (name, node) in fields {
        names.members.insert(name.clone());
        path.push(name.clone());
        match node {
            ResolvedNode::Branch { children, .. } => {
                collect_macros(
                    children,
                    block_name,
                    block_prefix,
                    fingerprints,
                    names,
                    path,
                    output,
                )?;
            }
            ResolvedNode::Leaf {
                leaf, scalar_abi, ..
            } => collect_leaf_macros(
                leaf,
                block_name,
                block_prefix,
                fingerprints,
                names,
                path,
                output,
                *scalar_abi,
            )?,
        }
        path.pop();
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn collect_leaf_macros(
    leaf: &LeafEntry,
    block_name: &str,
    block_prefix: &str,
    fingerprints: &IndexMap<&str, u64>,
    names: &mut NameRegistry,
    path: &[String],
    output: &mut String,
    scalar_abi: ScalarAbi,
) -> Result<(), LayoutError> {
    let size = leaf.size;

    let path_prefix = macro_path(block_prefix, path)?;
    if let Some(size) = size {
        match size {
            SizeSource::OneD(length) => {
                add_macro(
                    names,
                    output,
                    format!("{path_prefix}_LEN"),
                    format!("{length}u"),
                    format!("array '{}#{}'", block_name, path.join(".")),
                    true,
                )?;
            }
            SizeSource::TwoD([rows, columns]) => {
                let origin = format!("array '{}#{}'", block_name, path.join("."));
                add_macro(
                    names,
                    output,
                    format!("{path_prefix}_ROWS"),
                    format!("{rows}u"),
                    origin.clone(),
                    true,
                )?;
                add_macro(
                    names,
                    output,
                    format!("{path_prefix}_COLS"),
                    format!("{columns}u"),
                    origin,
                    false,
                )?;
            }
        }
    }

    if let EntrySource::Bitmap(fields) = &leaf.source {
        let width = scalar_abi.storage_size * 8;
        let mut shift = 0usize;
        for field in fields {
            if let BitmapFieldSource::Name(data_name) = &field.source {
                let region = to_upper_snake(data_name, "bitmap data-source name")?;
                let origin = format!(
                    "bitmap region '{}' in '{}#{}'",
                    data_name,
                    block_name,
                    path.join(".")
                );
                let prefix = format!("{path_prefix}_{region}");
                add_macro(
                    names,
                    output,
                    format!("{prefix}_SHIFT"),
                    format!("{shift}u"),
                    origin.clone(),
                    true,
                )?;
                add_macro(
                    names,
                    output,
                    format!("{prefix}_MASK"),
                    bitmap_mask(width, field.bits, shift),
                    origin,
                    false,
                )?;
            }
            shift += field.bits;
        }
    }

    if let EntrySource::Fingerprint(target) = &leaf.source {
        let target_name = target.block_name(block_name);
        let value = fingerprints.get(target_name).ok_or_else(|| {
            header_error(format!(
                "fingerprint target '{target_name}' from '{}' does not exist",
                path.join(".")
            ))
        })?;
        add_macro(
            names,
            output,
            format!("{path_prefix}_FINGERPRINT"),
            format!("UINT64_C(0x{value:016X})"),
            format!("fingerprint '{}#{}'", block_name, path.join(".")),
            true,
        )?;
    }

    Ok(())
}

fn add_macro(
    names: &mut NameRegistry,
    output: &mut String,
    name: String,
    value: String,
    origin: String,
    group_start: bool,
) -> Result<(), LayoutError> {
    names.add_macro(&name, origin)?;
    if group_start {
        output.push('\n');
    }
    let _ = writeln!(output, "#define {name} {value}");
    Ok(())
}

fn render_fields(
    fields: &[(String, ResolvedNode<'_>)],
    depth: usize,
    block_prefix: &str,
    path: &mut Vec<String>,
    output: &mut String,
) -> Result<(), LayoutError> {
    let indent = "  ".repeat(depth);
    for (name, node) in fields {
        path.push(name.clone());
        match node {
            ResolvedNode::Branch { children, .. } => {
                output.push_str(&format!("{indent}struct {{\n"));
                render_fields(children, depth + 1, block_prefix, path, output)?;
                output.push_str(&format!("{indent}}} {name};\n"));
            }
            ResolvedNode::Leaf {
                leaf, scalar_abi, ..
            } => {
                let c_type = scalar_abi.c_type;
                let dimensions = match leaf.size {
                    None => String::new(),
                    Some(SizeSource::OneD(_)) => {
                        format!("[{}_LEN]", macro_path(block_prefix, path)?)
                    }
                    Some(SizeSource::TwoD(_)) => {
                        let prefix = macro_path(block_prefix, path)?;
                        format!("[{prefix}_ROWS][{prefix}_COLS]")
                    }
                };
                let comment = match &leaf.source {
                    EntrySource::Bitmap(_) => " /* bitmap storage */".to_owned(),
                    EntrySource::Ref(source) if source.is_list() => {
                        " /* ref addresses */".to_owned()
                    }
                    EntrySource::Ref(_) => " /* ref address */".to_owned(),
                    EntrySource::Fingerprint(_) => " /* fingerprint */".to_owned(),
                    _ => leaf
                        .scalar_type
                        .fixed_point()
                        .map(|fixed| format!(" /* {fixed} */"))
                        .unwrap_or_default(),
                };
                output.push_str(&format!("{indent}{c_type} {name}{dimensions};{comment}\n"));
            }
        }
        path.pop();
    }
    Ok(())
}

fn render_layout_assertions(
    block_name: &str,
    typedef_name: &str,
    root: &ResolvedNode<'_>,
) -> String {
    let mut output = String::new();
    render_node_assertions(
        block_name,
        typedef_name,
        root,
        &mut String::new(),
        &mut output,
    );
    let root_size = root.coordinates().size;
    let _ = writeln!(
        output,
        "_Static_assert(sizeof({typedef_name}) * CHAR_BIT == {root_size}u * 8u, \"Mint ABI size mismatch for {typedef_name}\");"
    );
    output
}

fn render_node_assertions(
    block_name: &str,
    typedef_name: &str,
    node: &ResolvedNode<'_>,
    path: &mut String,
    output: &mut String,
) {
    let ResolvedNode::Branch { children, .. } = node else {
        return;
    };

    let parent_len = path.len();
    for (name, child) in children {
        if parent_len != 0 {
            path.push('.');
        }
        path.push_str(name);
        let offset = child.coordinates().offset;
        let _ = writeln!(
            output,
            "_Static_assert(offsetof({typedef_name}, {path}) * CHAR_BIT == {offset}u * 8u, \"Mint ABI offset mismatch for {block_name}.{path}\");"
        );
        render_node_assertions(block_name, typedef_name, child, path, output);
        path.truncate(parent_len);
    }
}

fn macro_path(block_prefix: &str, path: &[String]) -> Result<String, LayoutError> {
    let mut parts = Vec::with_capacity(path.len() + 1);
    parts.push(block_prefix.to_owned());
    for segment in path {
        parts.push(to_upper_snake(segment, "field name")?);
    }
    Ok(parts.join("_"))
}

fn bitmap_mask(storage_bits: usize, region_bits: usize, shift: usize) -> String {
    let base = if region_bits == 64 {
        u64::MAX
    } else {
        (1u64 << region_bits) - 1
    };
    let mask = ((base as u128) << shift) as u64;
    format!(
        "UINT{storage_bits}_C(0x{mask:0width$X})",
        width = storage_bits / 4
    )
}

fn to_upper_snake(value: &str, kind: &str) -> Result<String, LayoutError> {
    let chars = value.chars().collect::<Vec<_>>();
    let mut output = String::new();
    let mut previous_was_separator = true;

    for (index, character) in chars.iter().copied().enumerate() {
        if !character.is_ascii_alphanumeric() {
            if !output.is_empty() && !previous_was_separator {
                output.push('_');
            }
            previous_was_separator = true;
            continue;
        }

        let previous = index.checked_sub(1).and_then(|i| chars.get(i)).copied();
        let next = chars.get(index + 1).copied();
        let word_boundary = character.is_ascii_uppercase()
            && !previous_was_separator
            && (previous.is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                || (previous.is_some_and(|c| c.is_ascii_uppercase())
                    && next.is_some_and(|c| c.is_ascii_lowercase())));
        if word_boundary {
            output.push('_');
        }
        output.push(character.to_ascii_uppercase());
        previous_was_separator = false;
    }

    while output.ends_with('_') {
        output.pop();
    }
    if output.is_empty() {
        return Err(header_error(format!(
            "{kind} '{value}' does not contain an ASCII letter or digit"
        )));
    }
    if output.starts_with(|character: char| character.is_ascii_digit()) {
        output.insert(0, '_');
    }
    Ok(output)
}

fn header_error(message: impl Into<String>) -> LayoutError {
    LayoutError::HeaderGenerationFailed(message.into())
}
