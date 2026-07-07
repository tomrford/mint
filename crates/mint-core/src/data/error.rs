use thiserror::Error;

#[derive(Debug, Error)]
pub enum DataError {
    #[error("file error: {0}")]
    FileError(String),

    #[error("Excel column not found: {0}")]
    ColumnNotFound(String),

    #[error("retrieval error: {0}")]
    RetrievalError(String),

    #[error("data source error: {0}")]
    MiscError(String),

    #[error("while retrieving '{name}'")]
    WhileRetrieving {
        name: String,
        #[source]
        source: Box<DataError>,
    },
}
