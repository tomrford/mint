use clap::Args;

#[derive(Args, Debug, Clone, Default)]
pub struct DataArgs {
    #[arg(
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
        long,
        value_name = "PATH or json string",
        group = "datasource",
        requires = "version_selectors",
        help = "Path to the JSON file or a JSON string containing the postgres configuration (url, query_template, optional data_path for nested extraction)"
    )]
    pub postgres: Option<String>,

    #[arg(
        long,
        value_name = "PATH or json string",
        group = "datasource",
        requires = "version_selectors",
        help = "HTTP API configuration (url with $VERSION placeholder, optional method [GET/POST], optional body with $VERSION, optional headers, optional data_path)"
    )]
    pub http: Option<String>,

    #[arg(
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
        help = "Version columns to use in priority order (separate with '/')"
    )]
    pub versions: Option<String>,
}

impl DataArgs {
    /// Parses the version stack from the raw slash-separated string.
    pub fn get_version_list(&self) -> Vec<String> {
        let raw = self.versions.as_deref();
        raw.map(|r| {
            r.split('/')
                .map(|name| name.trim())
                .filter(|name| !name.is_empty())
                .map(|name| name.to_string())
                .collect()
        })
        .unwrap_or_default()
    }
}
