use crate::data_args::DataArgs;
pub use mint_core::data::DataSource;
use mint_core::data::error::DataError;
use mint_core::data::{ExcelDataSource, ExcelDataSourceOptions, JsonDataSource};

pub mod args {
    pub use crate::data_args::*;
}

pub fn create_data_source(args: &DataArgs) -> Result<Option<Box<dyn DataSource>>, DataError> {
    let versions = args.get_version_list();
    match (&args.xlsx, &args.json) {
        (Some(path), _) => {
            let mut options = ExcelDataSourceOptions::new(versions);
            if let Some(main_sheet) = &args.main_sheet {
                options.main_sheet.clone_from(main_sheet);
            }
            Ok(Some(Box::new(ExcelDataSource::from_path(path, options)?)))
        }
        (_, Some(input)) if input.ends_with(".json") => {
            Ok(Some(Box::new(JsonDataSource::from_path(input, &versions)?)))
        }
        (_, Some(input)) => Ok(Some(Box::new(JsonDataSource::from_str(input, &versions)?))),
        _ => Ok(None),
    }
}
