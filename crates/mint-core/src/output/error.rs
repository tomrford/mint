use thiserror::Error;

#[derive(Debug, Error)]
pub enum OutputError {
    #[error("File error: {0}.")]
    FileError(String),

    #[error("Hex output error: {0}.")]
    HexOutputError(String),

    #[error("Block memory overlap detected: {0}")]
    BlockOverlapError(String),
}
