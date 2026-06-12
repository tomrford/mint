use crate::data_args::DataArgs;
use crate::layout_args::LayoutArgs;
use crate::output_args::OutputArgs;

use clap::{Parser, Subcommand};

pub const SKILL_TEXT: &str = include_str!("../skill/mint/SKILL.md");

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Build flash blocks from layout files and data sources (Excel or JSON)",
    after_help = "Run `mint build --help` for build options."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    #[command(about = "Build flash blocks from layout files and data sources")]
    Build(Args),
    #[command(about = "Print the bundled Mint skill text")]
    Skill,
}

// Top-level CLI parser. Sub-sections are flattened from sub-Args structs.
#[derive(Parser, Debug)]
#[command(
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
