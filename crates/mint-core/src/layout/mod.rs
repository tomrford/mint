pub mod block;
mod conversions;
pub(crate) mod entry;
pub mod error;
pub(crate) mod fingerprint;
pub mod header;
pub mod scalar_type;
pub mod settings;
pub mod used_values;
pub mod value;

use block::Config;
use error::LayoutError;
use std::collections::hash_map::Entry;
use std::path::Path;

pub fn load_layout(filename: impl AsRef<Path>) -> Result<Config, LayoutError> {
    let filename = filename.as_ref();
    let text = std::fs::read_to_string(filename).map_err(|_| {
        LayoutError::FileError(format!("failed to open file: {}", filename.display()))
    })?;

    let ext = filename
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "toml" => parse_toml_layout_with_context(&text, &format!("file {}", filename.display())),
        _ => Err(LayoutError::FileError(
            "Unsupported layout file format; use .toml".to_owned(),
        )),
    }
}

pub fn parse_toml_layout(text: &str) -> Result<Config, LayoutError> {
    parse_toml_layout_with_context(text, "TOML layout")
}

fn parse_toml_layout_with_context(text: &str, context: &str) -> Result<Config, LayoutError> {
    let mut cfg: Config = toml::from_str(text)
        .map_err(|e| LayoutError::FileError(format!("failed to parse {}: {}", context, e)))?;
    promote_block_header_consts(&mut cfg)?;
    Ok(cfg)
}

pub(crate) fn validate_c_identifier(name: &str, kind: &str) -> Result<(), String> {
    if name.contains('.') {
        return Err(format!(
            "quoted dotted {kind} name '{name}' builds as a flat field with no C struct equivalent; use a nested table instead"
        ));
    }

    let mut chars = name.chars();
    let valid_start = chars
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic());
    if !valid_start || !chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
    {
        return Err(format!("{kind} name '{name}' is not a valid C identifier"));
    }
    if C_KEYWORDS.contains(&name) {
        return Err(format!("{kind} name '{name}' is a C keyword"));
    }
    Ok(())
}

const C_KEYWORDS: &[&str] = &[
    "_Alignas",
    "_Alignof",
    "_Atomic",
    "_Bool",
    "_Complex",
    "_Generic",
    "_Imaginary",
    "_Noreturn",
    "_Static_assert",
    "_Thread_local",
    "auto",
    "break",
    "case",
    "char",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extern",
    "float",
    "for",
    "goto",
    "if",
    "inline",
    "int",
    "long",
    "register",
    "restrict",
    "return",
    "short",
    "signed",
    "sizeof",
    "static",
    "struct",
    "switch",
    "typedef",
    "union",
    "unsigned",
    "void",
    "volatile",
    "while",
];

fn promote_block_header_consts(cfg: &mut Config) -> Result<(), LayoutError> {
    for (block_name, block) in &cfg.blocks {
        insert_promoted_const(
            &mut cfg.mint.consts,
            format!("{block_name}.start_address"),
            block.header.start_address,
        )?;
        insert_promoted_const(
            &mut cfg.mint.consts,
            format!("{block_name}.length"),
            block.header.length,
        )?;
    }
    Ok(())
}

fn insert_promoted_const(
    consts: &mut std::collections::HashMap<String, value::ValueSource>,
    name: String,
    value: u32,
) -> Result<(), LayoutError> {
    match consts.entry(name) {
        Entry::Occupied(entry) => Err(LayoutError::FileError(format!(
            "[mint.const] key '{}' collides with auto-promoted block header const",
            entry.key()
        ))),
        Entry::Vacant(entry) => {
            entry.insert(value::ValueSource::Single(value::DataValue::U64(
                u64::from(value),
            )));
            Ok(())
        }
    }
}
