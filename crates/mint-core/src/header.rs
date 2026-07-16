use crate::build::{BlockSelector, resolve_blocks};
use crate::error::MintError;
use crate::layout::block::Entry;
use crate::layout::entry::{BitmapFieldSource, EntrySource, LeafEntry, SizeSource};
use crate::layout::error::LayoutError;
use crate::layout::fingerprint;
use crate::layout::resolved::ResolvedLayout;
use crate::layout::scalar_type::ScalarType;
use crate::layout::settings::MintConfig;
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

/// Generate a complete C11 header for the selected layout blocks.
pub fn generate(blocks: &[BlockSelector]) -> Result<String, MintError> {
    if blocks.is_empty() {
        return Err(LayoutError::NoBlocksProvided.into());
    }

    let (resolved, layouts) = resolve_blocks(blocks)?;
    let fingerprints = layouts
        .iter()
        .map(|(path, layout)| {
            if fingerprint::uses_fingerprints(layout) {
                fingerprint::calculate(layout).map(|values| (path.clone(), values))
            } else {
                Ok((path.clone(), IndexMap::new()))
            }
        })
        .collect::<Result<HashMap<_, _>, LayoutError>>()?;
    let mut rendered = Vec::with_capacity(resolved.len());
    let mut names = NameRegistry::default();
    let mut guard_parts = Vec::with_capacity(resolved.len());

    for selected in resolved {
        let layout = layouts.get(&selected.layout).ok_or_else(|| {
            LayoutError::FileError(format!(
                "resolved layout missing from header map: {}",
                selected.layout.display()
            ))
        })?;
        let block = layout.blocks.get(&selected.name).ok_or_else(|| {
            LayoutError::BlockNotFound(format!(
                "'{}' in '{}'",
                selected.name,
                selected.layout.display()
            ))
        })?;
        let block_fingerprints = fingerprints.get(&selected.layout).ok_or_else(|| {
            LayoutError::FileError(format!(
                "resolved layout missing from fingerprint map: {}",
                selected.layout.display()
            ))
        })?;

        let result = render_block(
            &selected.name,
            &block.data,
            &layout.mint,
            block_fingerprints,
            &mut names,
        );
        let block_output = result.map_err(|source| MintError::InHeaderBlock {
            block_name: selected.name.clone(),
            layout_file: selected.layout.display().to_string(),
            source: Box::new(source.into()),
        })?;
        guard_parts.push(block_output.macro_prefix.clone());
        rendered.push(block_output);
    }

    let guard = format!("MINT_{}_H", guard_parts.join("_"));
    let mut output = format!("#ifndef {guard}\n#define {guard}\n\n#include <stdint.h>\n");

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

    output.push_str(&format!("\n#endif /* {guard} */\n"));
    Ok(output)
}

struct RenderedBlock {
    macro_prefix: String,
    macros: Vec<MacroDefinition>,
    typedef: String,
}

struct MacroDefinition {
    text: String,
    group_start: bool,
}

#[derive(Default)]
struct NameRegistry {
    typedefs: HashSet<String>,
    block_prefixes: HashMap<String, String>,
    macros: HashMap<String, String>,
}

impl NameRegistry {
    fn add_typedef(&mut self, name: String) -> Result<(), LayoutError> {
        if !self.typedefs.insert(name.clone()) {
            return Err(header_error(format!(
                "generated typedef '{name}' is duplicated"
            )));
        }
        Ok(())
    }

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
    block_name: &str,
    data: &Entry,
    settings: &MintConfig,
    fingerprints: &IndexMap<String, u64>,
    names: &mut NameRegistry,
) -> Result<RenderedBlock, LayoutError> {
    let typedef_name = format!("{block_name}_t");
    names.add_typedef(typedef_name.clone())?;

    let macro_prefix = to_upper_snake(block_name, "block name")?;
    names.add_block_prefix(&macro_prefix, block_name)?;

    ResolvedLayout::new(data)?;

    let Entry::Branch(source) = data else {
        return Err(header_error("block data must be a table"));
    };

    let mut macros = Vec::new();
    let mut path = Vec::new();
    collect_macros(
        source,
        block_name,
        &macro_prefix,
        settings,
        fingerprints,
        names,
        &mut path,
        &mut macros,
    )?;

    let mut typedef = String::from("typedef struct {\n");
    render_fields(source, 1, &macro_prefix, &mut path, &mut typedef)?;
    typedef.push_str(&format!("}} {typedef_name};\n"));

    Ok(RenderedBlock {
        macro_prefix,
        macros,
        typedef,
    })
}

#[allow(clippy::too_many_arguments)]
fn collect_macros(
    fields: &IndexMap<String, Entry>,
    block_name: &str,
    block_prefix: &str,
    settings: &MintConfig,
    fingerprints: &IndexMap<String, u64>,
    names: &mut NameRegistry,
    path: &mut Vec<String>,
    output: &mut Vec<MacroDefinition>,
) -> Result<(), LayoutError> {
    for (name, node) in fields {
        path.push(name.clone());
        match node {
            Entry::Branch(children) => {
                collect_macros(
                    children,
                    block_name,
                    block_prefix,
                    settings,
                    fingerprints,
                    names,
                    path,
                    output,
                )?;
            }
            Entry::Leaf(leaf) => collect_leaf_macros(
                leaf,
                block_name,
                block_prefix,
                settings,
                fingerprints,
                names,
                path,
                output,
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
    settings: &MintConfig,
    fingerprints: &IndexMap<String, u64>,
    names: &mut NameRegistry,
    path: &[String],
    output: &mut Vec<MacroDefinition>,
) -> Result<(), LayoutError> {
    let size = leaf.size()?;
    if let EntrySource::Checksum(name) = &leaf.source {
        settings.checksum_config(name)?;
    }

    let path_prefix = macro_path(block_prefix, path)?;
    if let Some(size) = size {
        match size {
            SizeSource::OneD(length) => {
                validate_extent(length, path)?;
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
                validate_extent(rows, path)?;
                validate_extent(columns, path)?;
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
        let width = leaf.scalar_type.size_bytes() * 8;
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

fn validate_extent(extent: usize, path: &[String]) -> Result<(), LayoutError> {
    if extent == 0 {
        return Err(header_error(format!(
            "array '{}' has a zero extent, which is not valid C11",
            path.join(".")
        )));
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
) -> Result<(), LayoutError> {
    let indent = "  ".repeat(depth);
    for (name, node) in fields {
        path.push(name.clone());
        match node {
            Entry::Branch(children) => {
                output.push_str(&format!("{indent}struct {{\n"));
                render_fields(children, depth + 1, block_prefix, path, output)?;
                output.push_str(&format!("{indent}}} {name};\n"));
            }
            Entry::Leaf(leaf) => {
                let c_type = c_type(leaf.scalar_type);
                let dimensions = match leaf.size()? {
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

fn c_type(scalar: ScalarType) -> &'static str {
    match scalar {
        ScalarType::U8 => "uint8_t",
        ScalarType::U16 => "uint16_t",
        ScalarType::U32 => "uint32_t",
        ScalarType::U64 => "uint64_t",
        ScalarType::I8 => "int8_t",
        ScalarType::I16 => "int16_t",
        ScalarType::I32 => "int32_t",
        ScalarType::I64 => "int64_t",
        ScalarType::F32 => "float",
        ScalarType::F64 => "double",
        ScalarType::Fixed(fixed) if fixed.signed => match fixed.total_bits {
            8 => "int8_t",
            16 => "int16_t",
            32 => "int32_t",
            64 => "int64_t",
            _ => unreachable!("fixed-point widths are validated while parsing"),
        },
        ScalarType::Fixed(fixed) => match fixed.total_bits {
            8 => "uint8_t",
            16 => "uint16_t",
            32 => "uint32_t",
            64 => "uint64_t",
            _ => unreachable!("fixed-point widths are validated while parsing"),
        },
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
