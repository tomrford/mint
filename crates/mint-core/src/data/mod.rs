pub mod error;
mod excel;
mod json;

use crate::layout::value::{DataValue, ValueSource};
use error::DataError;
pub use excel::{ExcelDataSource, ExcelDataSourceOptions};
pub use json::JsonDataSource;

/// Trait for data sources that provide values by name.
pub trait DataSource: Sync {
    /// Retrieves a single numeric or boolean value.
    fn retrieve_single_value(&self, name: &str) -> Result<DataValue, DataError>;

    /// Retrieves a 1D array (from sheet reference) or a literal string.
    fn retrieve_1d_array_or_string(&self, name: &str) -> Result<ValueSource, DataError>;

    /// Retrieves a 2D array from a sheet reference.
    fn retrieve_2d_array(&self, name: &str) -> Result<Vec<Vec<DataValue>>, DataError>;
}
