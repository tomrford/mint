use crate::diagnostic::Error;
use crate::fingerprint;
use crate::layout::{self, ResolvedLayout};
use crate::source::Source;
use crate::syntax::ParsedFile;
use crate::types;

#[derive(Clone, Debug)]
pub struct CompiledSchema {
    pub source: Source,
    pub layout: ResolvedLayout,
    pub fingerprint: u64,
}

pub fn compile(source: Source) -> Result<CompiledSchema, Error> {
    let parsed = match ParsedFile::parse(&source) {
        Ok(parsed) => parsed,
        Err(error) => return Err(error.with_source(source)),
    };
    let types = match types::compile_types(&parsed) {
        Ok(types) => types,
        Err(error) => return Err(error.with_source(source)),
    };
    let layout = match layout::resolve(types) {
        Ok(layout) => layout,
        Err(error) => return Err(error.with_source(source)),
    };
    let fingerprint = fingerprint::calculate(&layout);
    Ok(CompiledSchema {
        source,
        layout,
        fingerprint,
    })
}

pub fn fingerprint_hex(schema: &CompiledSchema) -> String {
    fingerprint::hex(schema.fingerprint)
}
