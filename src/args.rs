use crate::data::args::DataArgs;
use crate::layout::args::LayoutArgs;
use crate::output::args::OutputArgs;
use clap::Parser;

// Top-level CLI parser. Sub-sections are flattened from sub-Args structs.
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Build flash blocks from layout files and data sources (Excel or JSON)",
    after_help = "For more information, visit https://crates.io/crates/mint-cli"
)]
pub struct Args {
    #[command(flatten)]
    pub layout: LayoutArgs,

    #[command(flatten)]
    pub data: DataArgs,

    #[command(flatten)]
    pub output: OutputArgs,
}
