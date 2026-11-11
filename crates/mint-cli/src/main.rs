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

mod args;
mod commands;
mod data;
mod data_args;
mod layout_args;
mod output_args;
mod visuals;

use std::error::Error;
use std::process::ExitCode;

use args::{Args, Cli, Command, GUIDE_TEXT};
use clap::Parser;
use mint_core::build::Selection;
use mint_core::error::MintError;

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Build(args) => run_command(|| run_build(&args)),
        Command::Header(args) => run_command(|| commands::header(&args)),
        Command::Fingerprint(args) => run_command(|| commands::fingerprint(&args)),
        Command::Abi(args) => {
            commands::abi(&args);
            ExitCode::SUCCESS
        }
        Command::Skill(args) => {
            print!("{}", args.topic.map_or(GUIDE_TEXT, |topic| topic.text()));
            ExitCode::SUCCESS
        }
    }
}

fn run_command(command: impl FnOnce() -> Result<(), MintError>) -> ExitCode {
    match command() {
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
    }
}

fn run_build(args: &Args) -> Result<(), MintError> {
    let selection = Selection::load(&args.layout.blocks)?;
    let plan = selection.preflight()?;

    if !args.output.quiet
        && let Some(warning) = args.output.extension_warning()
    {
        eprintln!("warning: {warning}");
    }

    let data_source = data::create_data_source(&args.data)?;

    let stats = commands::build(args, &plan, data_source.as_deref())?;

    if !args.output.quiet {
        if args.output.stats {
            visuals::print_detailed(&stats);
        } else {
            visuals::print_summary(&stats);
        }
    }

    Ok(())
}
