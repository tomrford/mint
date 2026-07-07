use clap::Args;

#[derive(Args, Debug, Clone, Default)]
pub struct DataArgs {
    #[arg(
        short = 'x',
        long,
        value_name = "FILE",
        group = "datasource",
        requires = "variant_selectors",
        help = "Path to the Excel variants file"
    )]
    pub xlsx: Option<String>,

    #[arg(long, value_name = "NAME", help = "Main sheet name in Excel")]
    pub main_sheet: Option<String>,

    #[arg(
        short = 'j',
        long,
        value_name = "PATH or json string",
        group = "datasource",
        requires = "variant_selectors",
        help = "Path to JSON file or JSON string. Format: object with variant names as keys, each containing an object with name:value pairs (e.g., {\"VariantName\": {\"key1\": value1, \"key2\": value2}})"
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
        help = "Variant columns to use in priority order (separate with '/')"
    )]
    pub variants: Vec<String>,
}
