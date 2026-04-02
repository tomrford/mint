use thiserror::Error;

#[derive(Debug, Error)]
pub enum LayoutError {
    #[error("File error: {0}.")]
    FileError(String),

    #[error("Block not found: {0}")]
    BlockNotFound(String),

    #[error("Data value export failed: {0}.")]
    DataValueExportFailed(String),

    #[error("Invalid block argument: {0}.")]
    InvalidBlockArgument(String),

    #[error("No blocks provided.")]
    NoBlocksProvided,

    #[error("Missing datasheet: {0}")]
    MissingDataSheet(String),

    #[error("In field '{field}': {source}")]
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
