use clap::Args;

#[derive(Args, Debug, Clone, Default)]
pub struct DataArgs {
    #[arg(
        short = 'x',
        long,
        value_name = "FILE",
        group = "datasource",
        requires = "version_selectors",
        help = "Path to the Excel versions file"
    )]
    pub xlsx: Option<String>,

    #[arg(long, value_name = "NAME", help = "Main sheet name in Excel")]
    pub main_sheet: Option<String>,

    #[arg(
        short = 'j',
        long,
        value_name = "PATH or json string",
        group = "datasource",
        requires = "version_selectors",
        help = "Path to JSON file or JSON string. Format: object with version names as keys, each containing an object with name:value pairs (e.g., {\"VersionName\": {\"key1\": value1, \"key2\": value2}})"
    )]
    pub json: Option<String>,

    #[arg(
        short = 'v',
        long = "versions",
        value_name = "NAME[/NAME...]",
        requires = "datasource",
        group = "version_selectors",
        value_delimiter = '/',
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        help = "Version columns to use in priority order (separate with '/')"
    )]
    pub versions: Vec<String>,
}
