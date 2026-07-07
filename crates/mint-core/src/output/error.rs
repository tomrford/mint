use thiserror::Error;

#[derive(Debug, Error)]
pub enum OutputError {
    #[error("file error: {0}")]
    FileError(String),

    #[error("hex output error: {0}")]
    HexOutputError(String),

    #[error("address range error: {0}")]
    AddressRangeError(String),

    #[error("block memory overlap detected: {0}")]
    BlockOverlapError(String),
}
