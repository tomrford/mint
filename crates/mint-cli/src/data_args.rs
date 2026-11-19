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

impl DataArgs {
    pub fn input(&self) -> (Option<&Path>, impl FnOnce() -> LoadedSource) {
        enum Source<'a> {
            None,
            Excel(&'a Path),
            JsonFile(&'a Path),
            JsonInline(&'a str),
        }
        let source = match (self.xlsx.as_deref(), self.json.as_deref()) {
            (Some(path), _) => Source::Excel(Path::new(path)),
            (_, Some(json)) if json.trim_start().starts_with('{') => Source::JsonInline(json),
            (_, Some(path)) => Source::JsonFile(Path::new(path)),
            (None, None) => Source::None,
        };
        let file = match source {
            Source::Excel(path) | Source::JsonFile(path) => Some(path),
            _ => None,
        };
        (file, move || match source {
            Source::Excel(path) => {
                let mut options = ExcelDataSourceOptions::new(self.variants.clone());
                options.main_sheet = self.main_sheet.clone().unwrap_or(options.main_sheet);
                Ok(Some(Box::new(ExcelDataSource::from_path(path, options)?)))
            }
            Source::JsonFile(path) => Ok(Some(Box::new(JsonDataSource::from_path(
                path,
                &self.variants,
            )?))),
            Source::JsonInline(json) => Ok(Some(Box::new(JsonDataSource::from_str(
                json,
                &self.variants,
            )?))),
            Source::None => Ok(None),
        })
    }
}
