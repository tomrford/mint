use std::path::PathBuf;

use clap::Args;
pub use mint_core::output::OutputFormat;

pub fn parse_output_format(value: &str) -> Result<OutputFormat, String> {
    match value.to_ascii_lowercase().as_str() {
        "hex" => Ok(OutputFormat::Hex),
        "mot" => Ok(OutputFormat::Mot),
        _ => Err("unsupported output format; use hex or mot".to_owned()),
    }
}

/// Output configuration for the build command.
#[derive(Args, Debug, Clone)]
pub struct OutputArgs {
    /// Output file path (e.g., "out/firmware.hex").
    #[arg(
        short = 'o',
        long,
        value_name = "FILE",
        default_value = "out.hex",
        help = "Output file path"
    )]
    pub out: PathBuf,

    /// Number of bytes per HEX data record.
    #[arg(
        long,
        value_name = "N",
        default_value_t = 32u16,
        value_parser = clap::value_parser!(u16).range(1..=64),
        help = "Number of bytes per HEX data record (1..=64)",
    )]
    pub record_width: u16,

    /// Output format: hex or mot.
    #[arg(
        long,
        value_parser = parse_output_format,
        default_value = "hex",
        help = "Output format: hex or mot",
    )]
    pub format: OutputFormat,

    /// Export used values as a JSON report.
    #[arg(long, value_name = "FILE", help = "Export used values as JSON")]
    pub export_json: Option<PathBuf>,

    /// Show detailed build statistics.
    #[arg(long, help = "Show detailed build statistics")]
    pub stats: bool,

    /// Suppress all output except errors.
    #[arg(long, help = "Suppress all output except errors")]
    pub quiet: bool,
}
