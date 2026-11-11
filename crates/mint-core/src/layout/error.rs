use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LayoutError {
    #[error("file error: {0}")]
    FileError(String),

    #[error("block not found: {0}")]
    BlockNotFound(String),

    #[error("invalid layout: {0}")]
    InvalidLayout(String),

    #[error("data value export failed: {0}")]
    DataValueExportFailed(String),

    #[error("invalid block argument: {0}")]
    InvalidBlockArgument(String),

    #[error("C header generation failed: {0}")]
    HeaderGenerationFailed(String),

    #[error("no blocks provided")]
    NoBlocksProvided,

    #[error("missing datasheet: {0}")]
    MissingDataSheet(String),

    /// Names the block a static layout error is about; its message includes
    /// the full cause so callers without a chain printer still see it.
    #[error("in block '{block}': {}", full_message(error))]
    InBlock {
        block: String,
        error: Box<LayoutError>,
    },

    #[error("in field '{field}'")]
    InField {
        field: String,
        #[source]
        source: Box<LayoutError>,
    },

    #[error(
        "bitfield value {value} out of range for {bits}-bit {signedness} field ({min}..={max})"
    )]
    BitfieldOutOfRange {
        value: i128,
        bits: usize,
        signedness: &'static str,
        min: i128,
        max: i128,
    },

    #[error(transparent)]
    Data(#[from] crate::data::error::DataError),
}

fn full_message(error: &LayoutError) -> String {
    let mut message = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

pub(crate) fn in_field_path(path: &str, error: LayoutError) -> LayoutError {
    path.rsplit('.')
        .fold(error, |source, field| LayoutError::InField {
            field: field.to_owned(),
            source: Box::new(source),
        })
}
