use crate::build::{BlockSelector, Selection};
use crate::error::MintError;
use crate::layout::abi::Abi;
use crate::layout::block::Entry;
use crate::layout::entry::{BitmapFieldSource, EntrySource, LeafEntry, SizeSource};
use crate::layout::error::LayoutError;
use crate::layout::plan::PlannedBlock;
use crate::layout::resolved::ResolvedNode;
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Generate a complete C11 header for the selected layout blocks.
pub fn generate(blocks: &[BlockSelector]) -> Result<String, MintError> {
    let selection = Selection::load(blocks)?;
    let plan = selection.plan(|layout, block, error| MintError::InHeaderBlock {
        block_name: block.to_owned(),
        layout_file: layout.display().to_string(),
        source: Box::new(error.into()),
    })?;
    let mut rendered = Vec::with_capacity(plan.blocks.len());
    let mut names = NameRegistry::default();
    let mut guard_parts = Vec::with_capacity(plan.blocks.len());
    let mut includes: Option<(&Path, &[String])> = None;
    let mut abi: Option<(&Path, Abi)> = None;

    for selected in &plan.blocks {
        let layout_abi = selected.settings.abi;
        if let Some((first_path, first_abi)) = &abi {
            if !same_layout_rules(*first_abi, layout_abi) {
                return Err(header_error(format!(
                    "[mint].abi layout rules must match across selected layouts: '{}' uses '{}' and '{}' uses '{}', which differ in layout family, byte order or addressable-unit width; generate a separate header per ABI",
                    first_path.display(),
                    first_abi.name(),
                    selected.layout.display(),
                    layout_abi.name()
                ))
                .into());
            }
        } else {
            abi = Some((selected.layout, layout_abi));
        }
        let configured_includes = &selected.settings.header.includes;
        if let Some((first_path, first_includes)) = &includes {
            if *first_includes != configured_includes.as_slice() {
                return Err(header_error(format!(
                    "[mint.header].includes must match across selected layouts: '{}' and '{}' have different include lists",
                    first_path.display(),
                    selected.layout.display()
                ))
                .into());
            }
        } else {
            for include in configured_includes {
                validate_include(include).map_err(|message| {
                    header_error(format!("{}: {message}", selected.layout.display()))
                })?;
            }
            includes = Some((selected.layout, configured_includes));
        }

        let result = render_block(&selected.block, selected.settings.abi, &mut names);
        let block_output = result.map_err(|source| MintError::InHeaderBlock {
            block_name: selected.block.name.to_owned(),
            layout_file: selected.layout.display().to_string(),
            source: Box::new(source.into()),
        })?;
        guard_parts.push(block_output.macro_prefix.clone());
        rendered.push(block_output);
    }

    let guard = format!("MINT_{}_H", guard_parts.join("_"));
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

    let macros = rendered
        .iter()
        .flat_map(|block| block.macros.iter())
        .collect::<Vec<_>>();
    if !macros.is_empty() {
        output.push('\n');
        for (index, definition) in macros.iter().enumerate() {
            if index > 0 && definition.group_start {
                output.push('\n');
            }
            output.push_str(&definition.text);
            output.push('\n');
        }
    }

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

/// Profile names are ignored, as they are by fingerprints.
fn same_layout_rules(first: Abi, second: Abi) -> bool {
    first.family() == second.family()
        && first.endianness() == second.endianness()
        && first.address_unit_bits() == second.address_unit_bits()
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
    macros: Vec<MacroDefinition>,
    typedef: String,
    assertions: String,
}

struct MacroDefinition {
    text: String,
    group_start: bool,
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
    planned: &PlannedBlock<'_>,
    abi: Abi,
    names: &mut NameRegistry,
) -> Result<RenderedBlock, LayoutError> {
    let (block_name, block) = (planned.name, planned.block);
    let typedef_name = format!("{block_name}_t");
    let macro_prefix = to_upper_snake(block_name, "block name")?;
    names.add_block_prefix(&macro_prefix, block_name)?;

    let Entry::Branch(source) = &block.data else {
        unreachable!("resolution rejects block data that is not a table")
    };

    let mut macros = Vec::new();
    add_macro(
        names,
        &mut macros,
        format!("{macro_prefix}_START_ADDRESS"),
        format!("0x{:X}u", block.header.start_address),
        format!("block header '{block_name}.start_address'"),
        true,
    )?;
    add_macro(
        names,
        &mut macros,
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
        &planned.fingerprints,
        names,
        &mut path,
        &mut macros,
        abi,
    )?;

    let mut typedef = String::from("typedef struct {\n");
    render_fields(source, 1, &macro_prefix, &mut path, &mut typedef, abi)?;
    typedef.push_str(&format!("}} {typedef_name};\n"));

    let assertions = render_layout_assertions(block_name, &typedef_name, &planned.resolved.root);

    Ok(RenderedBlock {
        macro_prefix,
        macros,
        typedef,
        assertions,
    })
}

#[allow(clippy::too_many_arguments)]
fn collect_macros(
    fields: &IndexMap<String, Entry>,
    block_name: &str,
    block_prefix: &str,
    fingerprints: &HashMap<&str, u64>,
    names: &mut NameRegistry,
    path: &mut Vec<String>,
    output: &mut Vec<MacroDefinition>,
    abi: Abi,
) -> Result<(), LayoutError> {
    for (name, node) in fields {
        names.members.insert(name.clone());
        path.push(name.clone());
        match node {
            Entry::Branch(children) => {
                collect_macros(
                    children,
                    block_name,
                    block_prefix,
                    fingerprints,
                    names,
                    path,
                    output,
                    abi,
                )?;
            }
            Entry::Leaf(leaf) => collect_leaf_macros(
                leaf,
                block_name,
                block_prefix,
                fingerprints,
                names,
                path,
                output,
                abi,
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
    fingerprints: &HashMap<&str, u64>,
    names: &mut NameRegistry,
    path: &[String],
    output: &mut Vec<MacroDefinition>,
    abi: Abi,
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
        let width = abi.scalar(leaf.scalar_type)?.storage_size * 8;
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
        let value = fingerprints[target.block_name(block_name)];
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
    output: &mut Vec<MacroDefinition>,
    name: String,
    value: String,
    origin: String,
    group_start: bool,
) -> Result<(), LayoutError> {
    names.add_macro(&name, origin)?;
    output.push(MacroDefinition {
        text: format!("#define {name} {value}"),
        group_start,
    });
    Ok(())
}

fn render_fields(
    fields: &IndexMap<String, Entry>,
    depth: usize,
    block_prefix: &str,
    path: &mut Vec<String>,
    output: &mut String,
    abi: Abi,
) -> Result<(), LayoutError> {
    let indent = "  ".repeat(depth);
    for (name, node) in fields {
        path.push(name.clone());
        match node {
            Entry::Branch(children) => {
                output.push_str(&format!("{indent}struct {{\n"));
                render_fields(children, depth + 1, block_prefix, path, output, abi)?;
                output.push_str(&format!("{indent}}} {name};\n"));
            }
            Entry::Leaf(leaf) => {
                let c_type = abi.scalar(leaf.scalar_type)?.c_type;
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
    let mut path = Vec::new();
    render_node_assertions(block_name, typedef_name, root, &mut path, &mut output);

    let root_size = root.coordinates().size;
    output.push_str(&format!(
        "_Static_assert(sizeof({typedef_name}) * CHAR_BIT == {root_size}u * 8u, \"Mint ABI size mismatch for {typedef_name}\");\n"
    ));
    output
}

fn render_node_assertions(
    block_name: &str,
    typedef_name: &str,
    node: &ResolvedNode<'_>,
    path: &mut Vec<String>,
    output: &mut String,
) {
    let ResolvedNode::Branch { children, .. } = node else {
        return;
    };

    for (name, child) in children {
        path.push(name.clone());
        let offset = child.coordinates().offset;
        let member = path.join(".");
        output.push_str(&format!(
            "_Static_assert(offsetof({typedef_name}, {member}) * CHAR_BIT == {offset}u * 8u, \"Mint ABI offset mismatch for {block_name}.{member}\");\n"
        ));
        render_node_assertions(block_name, typedef_name, child, path, output);
        path.pop();
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
