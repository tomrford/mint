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
mod schema;
mod source;
mod syntax;
mod types;

pub use diagnostic::{Category, Diagnostic, Error};
pub use inspect::InspectFormat;
pub use schema::CompiledSchema;
pub use source::Source;

pub fn compile_header(source: Source) -> Result<CompiledSchema, Error> {
    schema::compile(source)
}

pub fn schema_fingerprint_hex(schema: &CompiledSchema) -> String {
    fingerprint::hex(schema.fingerprint)
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
