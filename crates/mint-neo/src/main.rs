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

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use mint_neo::{
    Category, CompiledSchema, Diagnostic, Error, InspectFormat, Source, abi_list, abi_show,
    compile_header, encode_json, inspect, render_hex,
};

#[derive(Parser, Debug)]
#[command(
    name = "mint-neo",
    bin_name = "mint-neo",
    about = "Encode one C header and one resolved JSON object into one Intel HEX range"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Build {
        header: PathBuf,
        #[arg(long)]
        json: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Fingerprint {
        header: PathBuf,
    },
    Inspect {
        header: PathBuf,
        #[arg(long, value_enum, default_value = "text")]
        format: CliInspectFormat,
    },
    Abi {
        #[command(subcommand)]
        command: AbiCommand,
    },
}

#[derive(Subcommand, Debug)]
enum AbiCommand {
    List,
    Show { abi: String },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliInspectFormat {
    Text,
    Json,
}

impl From<CliInspectFormat> for InspectFormat {
    fn from(value: CliInspectFormat) -> Self {
        match value {
            CliInspectFormat::Text => Self::Text,
            CliInspectFormat::Json => Self::Json,
        }
    }
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return ExitCode::from(2);
        }
    };
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprint!("{}", error.render(&[]));
            error.exit_code()
        }
    }
}

fn run(cli: Cli) -> Result<(), Error> {
    match cli.command {
        Command::Build { header, json, out } => build(&header, &json, &out),
        Command::Fingerprint { header } => {
            let schema = load_header(&header)?;
            println!("{}", mint_neo::schema_fingerprint_hex(&schema));
            Ok(())
        }
        Command::Inspect { header, format } => {
            let schema = load_header(&header)?;
            print!("{}", inspect(&schema, format.into())?);
            Ok(())
        }
        Command::Abi {
            command: AbiCommand::List,
        } => {
            print!("{}", abi_list());
            Ok(())
        }
        Command::Abi {
            command: AbiCommand::Show { abi },
        } => {
            print!("{}", abi_show(&abi)?);
            Ok(())
        }
    }
}

fn build(header: &Path, json: &Path, out: &Path) -> Result<(), Error> {
    let schema = load_header(header)?;
    let json_source = load_json(json)?;
    let bytes = encode_json(&schema, &json_source)?;
    let hex = render_hex(&schema, &bytes)?;
    std::fs::write(out, hex).map_err(|error| {
        Error::one(Diagnostic::new(
            Category::Encoding,
            out.display().to_string(),
            format!("failed to write output: {error}"),
        ))
    })?;
    Ok(())
}

fn load_header(path: &Path) -> Result<CompiledSchema, Error> {
    let source = Source::from_path(path).map_err(|message| {
        Error::one(Diagnostic::new(
            Category::Schema,
            path.display().to_string(),
            message,
        ))
    })?;
    compile_header(source)
}

fn load_json(path: &Path) -> Result<Source, Error> {
    if path == Path::new("-") {
        let mut text = String::new();
        io::stdin().read_to_string(&mut text).map_err(|error| {
            Error::one(Diagnostic::new(
                Category::Data,
                "<stdin>",
                format!("failed to read stdin: {error}"),
            ))
        })?;
        return Ok(Source::new("<stdin>", text));
    }
    Source::from_path(path).map_err(|message| {
        Error::one(Diagnostic::new(
            Category::Data,
            path.display().to_string(),
            message,
        ))
    })
}
