use thiserror::Error;

use crate::data::error::DataError;
use crate::layout::error::LayoutError;
use crate::output::error::OutputError;

#[derive(Debug, Error)]
pub enum MintError {
    #[error(transparent)]
    Layout(#[from] LayoutError),

    #[error(transparent)]
    Data(#[from] DataError),

    #[error(transparent)]
    Output(#[from] OutputError),

    #[error("while building block '{block_name}' from '{layout_file}'")]
    InBlock {
        block_name: String,
        layout_file: String,
        #[source]
        source: Box<MintError>,
    },
}
