#![cfg_attr(
    not(test),
    deny(
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::unwrap_used
    )
)]

mod abi;
mod annotation;
mod constants;
mod diagnostic;
mod fingerprint;
mod inspect;
mod integers;
mod json;
mod layout;
mod output;
mod source;
mod syntax;
mod types;

pub use diagnostic::{Category, Diagnostic, Error};
pub use inspect::InspectFormat;
pub use layout::{ArrayLayout, FieldLayout, LayoutKind, ResolvedLayout, TypeLayout};
pub use source::Source;

#[derive(Clone, Debug)]
pub struct CompiledSchema {
    source: Source,
    layout: ResolvedLayout,
    fingerprint: u64,
}

impl CompiledSchema {
    pub fn layout(&self) -> &ResolvedLayout {
        &self.layout
    }

    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }
}

pub fn compile_header(source: Source) -> Result<CompiledSchema, Error> {
    let parsed = syntax::ParsedFile::parse(&source)?;
    let types = types::compile_types(&parsed)?;
    let layout = layout::resolve(types, &source)?;
    let fingerprint = fingerprint::calculate(&layout);
    Ok(CompiledSchema {
        source,
        layout,
        fingerprint,
    })
}

pub fn schema_fingerprint_hex(schema: &CompiledSchema) -> String {
    format!("{:016x}", schema.fingerprint)
}

pub fn encode_json(schema: &CompiledSchema, json: &Source) -> Result<Vec<u8>, Error> {
    json::encode(schema, json)
}

pub fn render_hex(schema: &CompiledSchema, bytes: &[u8]) -> Result<String, Error> {
    output::render_i32hex(schema, bytes)
}

pub fn inspect(schema: &CompiledSchema, format: InspectFormat) -> Result<String, Error> {
    inspect::render(schema, format)
}

pub fn abi_list() -> String {
    abi::list_text()
}

pub fn abi_show(name: &str) -> Result<String, Error> {
    abi::show_text(name)
}

/// Validate an ABI profile name. The CLI uses this as a clap value parser.
pub fn validate_abi(name: &str) -> Result<(), String> {
    name.parse::<abi::Abi>().map(|_| ())
}
