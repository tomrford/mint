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

use clap::{Parser, Subcommand};
use mint_neo::{
    Category, CompiledSchema, Error, InspectFormat, Source, abi_list, abi_show, compile_header,
    encode_json, inspect, render_hex, validate_abi,
};

#[derive(Parser, Debug)]
#[command(
    name = "mint-neo",
    bin_name = "mint-neo",
    version,
    propagate_version = true,
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
        #[arg(long, value_parser = parse_inspect_format, default_value = "text")]
        format: InspectFormat,
    },
    Abi {
        #[command(subcommand)]
        command: AbiCommand,
    },
}

#[derive(Subcommand, Debug)]
enum AbiCommand {
    List,
    Show {
        #[arg(value_parser = parse_abi_arg)]
        abi: String,
    },
}

fn parse_inspect_format(value: &str) -> Result<InspectFormat, String> {
    match value {
        "text" => Ok(InspectFormat::Text),
        "json" => Ok(InspectFormat::Json),
        other => Err(format!("invalid value '{other}' for '--format'")),
    }
}

fn parse_abi_arg(name: &str) -> Result<String, String> {
    validate_abi(name)?;
    Ok(name.to_owned())
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return clap_exit_code(&error);
        }
    };
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprint!("{error}");
            error.exit_code()
        }
    }
}

fn clap_exit_code(error: &clap::Error) -> ExitCode {
    match u8::try_from(error.exit_code()) {
        Ok(code) => ExitCode::from(code),
        Err(_) => ExitCode::from(2),
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
            print!("{}", inspect(&schema, format)?);
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
    reject_output_collision(header, json, out)?;
    let schema = load_header(header)?;
    let json_source = load_json(json)?;
    let bytes = encode_json(&schema, &json_source)?;
    let hex = render_hex(&schema, &bytes)?;
    std::fs::write(out, hex).map_err(|error| {
        Error::named(
            Category::Encoding,
            out.display().to_string(),
            format!("failed to write {}: {error}", out.display()),
        )
    })?;
    Ok(())
}

fn reject_output_collision(header: &Path, json: &Path, out: &Path) -> Result<(), Error> {
    let collision = if same_destination(header, out)? {
        Some("header")
    } else if json != Path::new("-") && same_destination(json, out)? {
        Some("JSON input")
    } else {
        None
    };
    let Some(input) = collision else {
        return Ok(());
    };
    Err(Error::named(
        Category::Encoding,
        out.display().to_string(),
        format!("--out resolves to the {input} path"),
    ))
}

fn same_destination(left: &Path, right: &Path) -> Result<bool, Error> {
    Ok(destination_identity(left)? == destination_identity(right)?)
}

fn destination_identity(path: &Path) -> Result<PathBuf, Error> {
    let absolute = std::path::absolute(path).map_err(|error| {
        Error::named(
            Category::Encoding,
            path.display().to_string(),
            format!("failed to resolve path {}: {error}", path.display()),
        )
    })?;
    if let Ok(canonical) = absolute.canonicalize() {
        return Ok(canonical);
    }
    let Some(parent) = absolute.parent() else {
        return Ok(absolute);
    };
    match (parent.canonicalize(), absolute.file_name()) {
        (Ok(parent), Some(name)) => Ok(parent.join(name)),
        _ => Ok(absolute),
    }
}

fn load_header(path: &Path) -> Result<CompiledSchema, Error> {
    compile_header(read_source(path, Category::Schema)?)
}

fn load_json(path: &Path) -> Result<Source, Error> {
    if path == Path::new("-") {
        let mut text = String::new();
        io::stdin().read_to_string(&mut text).map_err(|error| {
            Error::named(
                Category::Data,
                "<stdin>",
                format!("failed to read stdin: {error}"),
            )
        })?;
        return Ok(Source::new("<stdin>", text));
    }
    read_source(path, Category::Data)
}

fn read_source(path: &Path, category: Category) -> Result<Source, Error> {
    Source::from_path(path).map_err(|error| {
        Error::named(
            category,
            path.display().to_string(),
            format!("failed to read {}: {error}", path.display()),
        )
    })
}
