use thiserror::Error;

#[derive(Debug, Error)]
pub enum DataError {
    #[error("File error: {0}.")]
    FileError(String),

    #[error("Excel column not found: {0}.")]
    ColumnNotFound(String),

    #[error("Excel retrieval error: {0}.")]
    RetrievalError(String),

    #[error("Misc error: {0}.")]
    MiscError(String),

    #[error("While retrieving '{name}': {source}")]
    WhileRetrieving {
        name: String,
        #[source]
        source: Box<DataError>,
    },
}
