use clap::Args;

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

impl DataArgs {
    pub fn path(&self) -> Option<&std::path::Path> {
        self.xlsx
            .as_deref()
            .or_else(|| {
                self.json
                    .as_deref()
                    .filter(|input| !input.trim_start().starts_with('{'))
            })
            .map(std::path::Path::new)
    }
}
