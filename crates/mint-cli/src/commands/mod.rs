pub mod stats;
mod writer;

use crate::args::Args;
use mint_core::build::{self, BuildRequest, BuildStats};
use mint_core::data::DataSource;
use mint_core::error::MintError;
use mint_core::output::{self, OutputFile};
use writer::write_output;

pub fn build(args: &Args, data_source: Option<&dyn DataSource>) -> Result<BuildStats, MintError> {
    let artifact = build::build(BuildRequest {
        blocks: args.layout.blocks.clone(),
        data_source,
        strict: args.layout.strict,
        capture_values: args.output.export_json.is_some(),
    })?;

    if let (Some(path), Some(report)) = (&args.output.export_json, &artifact.used_values) {
        output::report::write_used_values_json(path, report)?;
    }

    let output_file = OutputFile {
        ranges: artifact.ranges,
        format: args.output.format,
        record_width: args.output.record_width as usize,
    };
    write_output(&output_file, &args.output)?;

    Ok(artifact.stats)
}
