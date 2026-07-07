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

use std::error::Error;
use std::process::ExitCode;

use clap::Parser;
use mint_cli::args::{Args, Cli, Command, SKILL_TEXT};
use mint_cli::{commands, data, visuals};
use mint_core::error::MintError;

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Build(args) => match run_build(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err}");
                let mut source = err.source();
                while let Some(cause) = source {
                    eprintln!("  caused by: {cause}");
                    source = cause.source();
                }
                ExitCode::FAILURE
            }
        },
        Command::Skill => {
            print!("{SKILL_TEXT}");
            ExitCode::SUCCESS
        }
    }
}

fn run_build(args: &Args) -> Result<(), MintError> {
    let data_source = data::create_data_source(&args.data)?;

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
