use crate::data_args::DataArgs;
use crate::layout_args::{LayoutArgs, parse_block_arg};
use crate::output_args::OutputArgs;

use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::{Args as ClapArgs, Parser, Subcommand, ValueEnum};
use mint_core::build::BlockSelector;
use mint_core::layout::abi::Abi;
use std::path::PathBuf;

pub const GUIDE_TEXT: &str = include_str!("../skill/mint/SKILL.md");

#[derive(Parser, Debug)]
#[command(
    name = "mint",
    bin_name = "mint",
    author,
    version,
    about = "Build flash blocks and generate C headers and ABI fingerprints from TOML layouts",
    after_help = "Run `mint <COMMAND> --help` for command options and `mint skill` for the guide and reference."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    #[command(about = "Build flash blocks from layout files and data sources")]
    Build(Args),
    #[command(about = "Generate a C header from layout blocks")]
    Header(HeaderArgs),
    #[command(about = "Print ABI fingerprints for layout blocks")]
    Fingerprint(FingerprintArgs),
    #[command(about = "List and inspect supported ABIs")]
    Abi(AbiArgs),
    #[command(about = "Print the agent guide, or one reference topic")]
    Skill(SkillArgs),
}

#[derive(ClapArgs, Debug)]
pub struct SkillArgs {
    #[arg(value_enum, help = "Reference topic to print instead of the guide")]
    pub topic: Option<SkillTopic>,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum SkillTopic {
    /// Layout syntax, ABI rules, validation, C headers and fingerprints
    Layout,
    /// Excel and JSON data sources
    Sources,
    /// Block selectors, outputs and build-system integration
    Cli,
}

impl SkillTopic {
    pub fn text(self) -> &'static str {
        match self {
            Self::Layout => include_str!("../doc/layout.md"),
            Self::Sources => include_str!("../doc/sources.md"),
            Self::Cli => include_str!("../doc/cli.md"),
        }
    }
}

#[derive(ClapArgs, Debug)]
pub struct AbiArgs {
    #[command(subcommand)]
    pub command: AbiCommand,
}

#[derive(Subcommand, Debug)]
pub enum AbiCommand {
    #[command(about = "List supported ABI names")]
    List,
    #[command(about = "Show layout properties for an ABI")]
    Show {
        #[arg(value_name = "ABI", value_parser = abi_value_parser())]
        abi: Abi,
    },
}

fn abi_value_parser() -> impl TypedValueParser<Value = Abi> {
    PossibleValuesParser::new(Abi::ALL.map(Abi::name)).try_map(|name| name.parse::<Abi>())
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
    #[arg(value_name = "FILE[#BLOCK] | FILE", required = true, num_args = 1.., value_parser = parse_block_arg, help = "Layout files (every block) or FILE#BLOCK selectors; typedefs follow argument order")]
    pub blocks: Vec<BlockSelector>,

    #[arg(short, long, value_name = "FILE", help = "Output C header path")]
    pub out: PathBuf,
}

#[derive(ClapArgs, Debug)]
pub struct FingerprintArgs {
    #[arg(value_name = "FILE[#BLOCK] | FILE", value_parser = parse_block_arg, help = "FILE#BLOCK prints that block's bare fingerprint; FILE prints a `<block> <fingerprint>` line per block")]
    pub block: BlockSelector,
}
