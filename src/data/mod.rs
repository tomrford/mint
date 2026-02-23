pub mod args;
pub mod error;
mod excel;
mod helpers;
mod json;

use crate::layout::value::{DataValue, ValueSource};
use error::DataError;
use excel::ExcelDataSource;
use json::JsonDataSource;

/// Trait for data sources that provide values by name.
pub trait DataSource: Sync {
    /// Retrieves a single numeric or boolean value.
    fn retrieve_single_value(&self, name: &str) -> Result<DataValue, DataError>;

    /// Retrieves a 1D array (from sheet reference) or a literal string.
    fn retrieve_1d_array_or_string(&self, name: &str) -> Result<ValueSource, DataError>;

    /// Retrieves a 2D array from a sheet reference.
    fn retrieve_2d_array(&self, name: &str) -> Result<Vec<Vec<DataValue>>, DataError>;
}

/// Creates a data source from CLI arguments.
///
/// Returns `None` if no data source is configured (e.g., no `--xlsx` provided).
pub fn create_data_source(args: &args::DataArgs) -> Result<Option<Box<dyn DataSource>>, DataError> {
    match (&args.xlsx, &args.postgres, &args.http, &args.json) {
        (Some(_), _, _, _) => Ok(Some(Box::new(ExcelDataSource::new(args)?))),
        (_, Some(_), _, _) => Ok(Some(Box::new(JsonDataSource::from_postgres(args)?))),
        (_, _, Some(_), _) => Ok(Some(Box::new(JsonDataSource::from_http(args)?))),
        (_, _, _, Some(_)) => Ok(Some(Box::new(JsonDataSource::from_json(args)?))),
        _ => Ok(None),
    }
}
