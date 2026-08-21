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
    let result: Result<(ResolvedLayout, u64), Error> = (|| {
        let parsed = ParsedFile::parse(&source)?;
        let types = types::compile_types(&parsed)?;
        let layout = layout::resolve(types)?;
        let fingerprint = fingerprint::calculate(&layout);
        Ok((layout, fingerprint))
    })();
    match result {
        Ok((layout, fingerprint)) => Ok(CompiledSchema {
            source,
            layout,
            fingerprint,
        }),
        Err(error) => Err(error.with_source(source)),
    }
}

pub fn fingerprint_hex(schema: &CompiledSchema) -> String {
    fingerprint::hex(schema.fingerprint)
}
