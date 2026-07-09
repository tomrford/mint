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
        value_parser = clap::value_parser!(u16).range(1..=128),
        help = "Number of bytes per HEX data record (1..=128)",
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

impl OutputArgs {
    pub fn extension_warning(&self) -> Option<String> {
        let extension = self.out.extension()?.to_str()?.to_ascii_lowercase();
        let conflicts = match self.format {
            OutputFormat::Hex => {
                matches!(extension.as_str(), "mot" | "srec" | "s19" | "s28" | "s37")
            }
            OutputFormat::Mot => matches!(extension.as_str(), "hex" | "ihex" | "ihx"),
        };
        if !conflicts {
            return None;
        }

        let format_name = match self.format {
            OutputFormat::Hex => "Intel HEX",
            OutputFormat::Mot => "Motorola S-Record",
        };
        Some(format!(
            "output extension '.{extension}' does not match {format_name} format"
        ))
    }
}
