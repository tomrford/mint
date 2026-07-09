use crate::data_args::DataArgs;
use mint_core::data::DataSource;
use mint_core::data::error::DataError;
use mint_core::data::{ExcelDataSource, ExcelDataSourceOptions, JsonDataSource};

pub fn create_data_source(args: &DataArgs) -> Result<Option<Box<dyn DataSource>>, DataError> {
    let variants = args.variants.clone();
    match (&args.xlsx, &args.json) {
        (Some(path), _) => {
            let mut options = ExcelDataSourceOptions::new(variants);
            if let Some(main_sheet) = &args.main_sheet {
                options.main_sheet.clone_from(main_sheet);
            }
            Ok(Some(Box::new(ExcelDataSource::from_path(path, options)?)))
        }
        (_, Some(input)) if !input.trim_start().starts_with('{') => {
            Ok(Some(Box::new(JsonDataSource::from_path(input, &variants)?)))
        }
        (_, Some(input)) => Ok(Some(Box::new(JsonDataSource::from_str(input, &variants)?))),
        _ => Ok(None),
    }
}
