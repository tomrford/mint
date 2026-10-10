pub mod error;
mod excel;
mod json;

use crate::layout::value::{DataValue, ValueSource};
use error::DataError;
pub use excel::{ExcelDataSource, ExcelDataSourceOptions};
pub use json::JsonDataSource;

/// Values retrieved by layout field name.
pub trait DataSource: Sync {
    /// Retrieves a single numeric or boolean value.
    fn retrieve_single_value(&self, name: &str) -> Result<DataValue, DataError>;

    fn retrieve_1d_array_or_string(&self, name: &str) -> Result<ValueSource, DataError>;

    fn retrieve_2d_array(&self, name: &str) -> Result<Vec<Vec<DataValue>>, DataError>;
}
