use crate::data_args::DataArgs;
use crate::layout_args::{LayoutArgs, parse_block_arg};
use crate::output_args::OutputArgs;

use clap::{Args as ClapArgs, Parser, Subcommand};
use mint_core::build::BlockSelector;
use std::path::PathBuf;

pub const SKILL_TEXT: &str = include_str!("../skill/mint/SKILL.md");

#[derive(Parser, Debug)]
#[command(
    name = "mint",
    // Pin the usage-line name so help output is identical on Windows,
    // where argv[0] is mint.exe.
    bin_name = "mint",
    author,
    version,
    about = "Build flash blocks and generate C headers from TOML layouts",
    after_help = "Run `mint <COMMAND> --help` for command options."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    #[command(
        about = "Build flash blocks from layout files and data sources",
        after_help = "For more information, visit https://crates.io/crates/mint-cli"
    )]
    Build(Args),
    #[command(about = "Generate a C header from layout blocks")]
    Header(HeaderArgs),
    #[command(about = "Print the bundled Mint skill text")]
    Skill,
}

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[command(flatten)]
    pub layout: LayoutArgs,

    #[command(flatten)]
    pub data: DataArgs,

    #[command(flatten)]
    pub output: OutputArgs,
}

#[derive(ClapArgs, Debug)]
pub struct HeaderArgs {
    #[arg(value_name = "FILE[#BLOCK] | FILE", required = true, num_args = 1.., value_parser = parse_block_arg, help = "One or more layout selectors as file[#block] or a layout file to generate all blocks")]
    pub blocks: Vec<BlockSelector>,

    #[arg(short, long, value_name = "FILE", help = "Output C header path")]
    pub out: PathBuf,
}
