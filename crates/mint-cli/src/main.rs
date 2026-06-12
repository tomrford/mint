#![cfg_attr(
    not(test),
    deny(
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::unwrap_used
    )
)]

use mint_cli::args::{Args, Cli, Command, SKILL_TEXT};
use mint_cli::{commands, data, visuals};
use mint_core::error::MintError;

fn main() -> Result<(), MintError> {
    match Cli::parse_normalized().command {
        Command::Build(args) => run_build(&args),
        Command::Skill => {
            print!("{SKILL_TEXT}");
            Ok(())
        }
    }
}

fn run_build(args: &Args) -> Result<(), MintError> {
    let data_source = data::create_data_source(&args.data)?;

    // Check if blocks are provided
    args.layout
        .blocks
        .first()
        .ok_or(mint_core::layout::error::LayoutError::NoBlocksProvided)?;

    let stats = commands::build(args, data_source.as_deref())?;

    if !args.output.quiet {
        if args.output.stats {
            visuals::print_detailed(&stats);
        } else {
            visuals::print_summary(&stats);
        }
    }

    Ok(())
}
