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

use std::io::{self, Read, Write};
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
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprint!("{error}");
            error.exit_code()
        }
    }
}

fn run(cli: Cli) -> Result<(), Error> {
    let output = match cli.command {
        Command::Build { header, json, out } => return build(&header, &json, &out),
        Command::Fingerprint { header } => {
            let schema = load_header(&header)?;
            format!("{}\n", mint_neo::schema_fingerprint_hex(&schema))
        }
        Command::Inspect { header, format } => inspect(&load_header(&header)?, format)?,
        Command::Abi {
            command: AbiCommand::List,
        } => abi_list(),
        Command::Abi {
            command: AbiCommand::Show { abi },
        } => abi_show(&abi)?,
    };
    io::stdout()
        .lock()
        .write_all(output.as_bytes())
        .map_err(|error| {
            Error::named(
                Category::Encoding,
                "<stdout>",
                format!("failed to write stdout: {error}"),
            )
        })
}

fn build(header: &Path, json: &Path, out: &Path) -> Result<(), Error> {
    reject_output_collision(header, json, out)?;
    let schema = load_header(header)?;
    let json_source = load_json(json)?;
    let bytes = encode_json(&schema, &json_source)?;
    let hex = render_hex(&schema, &bytes)?;
    write_output(out, hex.as_bytes()).map_err(|error| {
        Error::named(
            Category::Encoding,
            out.display().to_string(),
            format!("failed to write {}: {error}", out.display()),
        )
    })?;
    Ok(())
}

fn write_output(out: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = out
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let builder = &mut tempfile::Builder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o666));
    }
    let mut file = builder.tempfile_in(parent)?;
    if let Ok(metadata) = out.symlink_metadata()
        && metadata.is_file()
    {
        file.as_file().set_permissions(metadata.permissions())?;
    }
    file.write_all(bytes)?;
    // Replace the directory entry only after the complete image is written.
    // In particular, never truncate an input reached through a hard link.
    file.persist(out).map_err(|error| error.error)?;
    Ok(())
}

fn reject_output_collision(header: &Path, json: &Path, out: &Path) -> Result<(), Error> {
    // A missing destination cannot alias an existing input. Hard links are
    // safe because write_output replaces the output entry instead of its data.
    let Ok(destination) = out.canonicalize() else {
        return Ok(());
    };
    for (input, name) in [(header, "header"), (json, "JSON input")] {
        if name == "JSON input" && input == Path::new("-") {
            continue;
        }
        if input.canonicalize().is_ok_and(|path| path == destination) {
            return Err(Error::named(
                Category::Encoding,
                out.display().to_string(),
                format!("--out resolves to the {name} path"),
            ));
        }
    }
    Ok(())
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
