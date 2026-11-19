use clap::Args;
use mint_core::data::{
    DataSource, ExcelDataSource, ExcelDataSourceOptions, JsonDataSource, error::DataError,
};
use std::path::Path;

#[derive(Args, Debug, Clone, Default)]
pub struct DataArgs {
    #[arg(
        short = 'x',
        long,
        id = "xlsx",
        value_name = "FILE",
        group = "datasource",
        requires = "variant_selectors",
        help = "Excel workbook data source"
    )]
    pub xlsx: Option<String>,

    #[arg(
        long,
        value_name = "NAME",
        requires = "xlsx",
        conflicts_with = "json",
        help = "Excel sheet holding the Name and variant columns [default: Main]"
    )]
    pub main_sheet: Option<String>,

    #[arg(
        short = 'j',
        long,
        value_name = "PATH or json string",
        group = "datasource",
        requires = "variant_selectors",
        help = "Path to JSON file, or inline JSON when the trimmed value starts with '{'. Format: object with variant names as keys, each containing an object with name:value pairs (e.g., {\"VariantName\": {\"key1\": value1, \"key2\": value2}})"
    )]
    pub json: Option<String>,

    #[arg(
        short = 'v',
        long = "variants",
        value_name = "NAME[/NAME...]",
        requires = "datasource",
        group = "variant_selectors",
        value_delimiter = '/',
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        help = "Variants to read in priority order, separated by '/'; a value missing from one variant falls through to the next"
    )]
    pub variants: Vec<String>,
}

type LoadedSource = Result<Option<Box<dyn DataSource>>, DataError>;

enum DataInput<'a> {
    None,
    Excel(&'a Path),
    JsonFile(&'a Path),
    JsonInline(&'a str),
}

impl DataArgs {
    pub fn input(&self) -> (Option<&Path>, impl FnOnce() -> LoadedSource) {
        let input = match (self.xlsx.as_deref(), self.json.as_deref()) {
            (Some(path), _) => DataInput::Excel(Path::new(path)),
            (_, Some(json)) if json.trim_start().starts_with('{') => DataInput::JsonInline(json),
            (_, Some(path)) => DataInput::JsonFile(Path::new(path)),
            (None, None) => DataInput::None,
        };
        let file = match input {
            DataInput::Excel(path) | DataInput::JsonFile(path) => Some(path),
            DataInput::JsonInline(_) | DataInput::None => None,
        };
        (file, move || -> LoadedSource {
            match input {
                DataInput::Excel(path) => {
                    let mut options = ExcelDataSourceOptions::new(self.variants.clone());
                    options.main_sheet = self.main_sheet.clone().unwrap_or(options.main_sheet);
                    Ok(Some(Box::new(ExcelDataSource::from_path(path, options)?)))
                }
                DataInput::JsonFile(path) => Ok(Some(Box::new(JsonDataSource::from_path(
                    path,
                    &self.variants,
                )?))),
                DataInput::JsonInline(json) => Ok(Some(Box::new(JsonDataSource::from_str(
                    json,
                    &self.variants,
                )?))),
                DataInput::None => Ok(None),
            }
        })
    }
}
