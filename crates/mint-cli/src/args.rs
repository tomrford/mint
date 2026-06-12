use crate::data_args::DataArgs;
use crate::layout_args::LayoutArgs;
use crate::output_args::OutputArgs;
use std::ffi::OsString;

use clap::{CommandFactory, Parser, Subcommand};

pub const SKILL_TEXT: &str = include_str!("../skill/mint/SKILL.md");

const DEFAULT_COMMAND: &str = "build";
const PRIORITY_FLAGS: &[&str] = &["-h", "--help", "-V", "--version"];

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Build flash blocks from layout files and data sources (Excel or JSON)",
    after_help = "Run `mint build --help` for build options. Existing build invocations without the `build` command are also supported."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    pub fn parse_normalized() -> Self {
        Self::parse_from(normalize_args(std::env::args_os()))
    }

    pub fn try_parse_from_normalized<I, T>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
        Self::try_parse_from(normalize_args(args))
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    #[command(about = "Build flash blocks from layout files and data sources")]
    Build(Args),
    #[command(about = "Print the bundled Mint skill text")]
    Skill,
}

pub fn normalize_args<I, T>(args: I) -> Vec<OsString>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let mut args = args.into_iter().map(Into::into).collect::<Vec<_>>();

    if args.is_empty() {
        args.push(OsString::from("mint"));
    }

    if has_priority_flag(&args) || has_subcommand(&args) {
        return args;
    }

    args.insert(1, OsString::from(DEFAULT_COMMAND));
    args
}

fn has_priority_flag(args: &[OsString]) -> bool {
    args.iter()
        .skip(1)
        .filter_map(|arg| arg.to_str())
        .any(|arg| PRIORITY_FLAGS.contains(&arg))
}

fn has_subcommand(args: &[OsString]) -> bool {
    Cli::command()
        .ignore_errors(true)
        .try_get_matches_from(args)
        .ok()
        .and_then(|matches| matches.subcommand_name().map(str::to_owned))
        .is_some()
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
