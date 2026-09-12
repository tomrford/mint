mod writer;

use crate::args::{AbiArgs, AbiCommand, Args, FingerprintArgs, HeaderArgs};
use mint_core::build::{self, BuildRequest, BuildStats};
use mint_core::data::DataSource;
use mint_core::error::MintError;
use mint_core::layout::abi::Abi;
use mint_core::layout::scalar_type::ScalarType;
use mint_core::output;
use writer::write_files;

pub fn header(args: &HeaderArgs) -> Result<(), MintError> {
    let contents = mint_core::header::generate(&args.blocks)?;
    let inputs = args
        .blocks
        .iter()
        .map(|block| block.layout.as_path())
        .collect::<Vec<_>>();
    write_files(&inputs, &[(&args.out, &contents)])?;
    Ok(())
}

pub fn fingerprint(args: &FingerprintArgs) -> Result<(), MintError> {
    let fingerprints = mint_core::fingerprint::load(&args.block)?;
    if args.block.block.is_some() {
        for fingerprint in fingerprints {
            println!("{}", fingerprint.hex());
        }
    } else {
        for fingerprint in fingerprints {
            println!("{} {}", fingerprint.block, fingerprint.hex());
        }
    }
    Ok(())
}

pub fn abi(args: &AbiArgs) {
    match args.command {
        AbiCommand::List => {
            for abi in Abi::ALL {
                println!("{:<18} {}", abi.name(), abi.description());
            }
        }
        AbiCommand::Show { abi } => {
            println!("name: {}", abi.name());
            println!("family: {}", abi.family().name());
            println!("description: {}", abi.description());
            println!("byte order: {}", abi.endianness());
            println!("target addressable unit: {} bits", abi.address_unit_bits());
            println!("output addresses: {}", abi.output_addressing());
            println!("aggregate rules: {}", abi.family().aggregate_rules());
            println!();
            println!("type  storage  alignment  stride  C type");
            for scalar in [
                ScalarType::U8,
                ScalarType::I8,
                ScalarType::U16,
                ScalarType::I16,
                ScalarType::U32,
                ScalarType::I32,
                ScalarType::U64,
                ScalarType::I64,
                ScalarType::F32,
                ScalarType::F64,
            ] {
                match abi.scalar(scalar) {
                    Ok(layout) => println!(
                        "{:<4}  {:>7}  {:>9}  {:>6}  {}",
                        scalar,
                        layout.storage_size,
                        layout.alignment,
                        layout.array_stride,
                        layout.c_type
                    ),
                    Err(_) => println!("{scalar:<4}  unsupported"),
                }
            }
            println!("all sizes, alignments and strides are in octets");
            println!("fixed-point values use the matching-width signed or unsigned integer layout");
        }
    }
}

pub fn build(args: &Args, data_source: Option<&dyn DataSource>) -> Result<BuildStats, MintError> {
    let artifact = build::build(BuildRequest {
        blocks: args.layout.blocks.clone(),
        data_source,
        strict: args.layout.strict,
        capture_values: args.output.export_json.is_some(),
    })?;

    let contents = artifact.render(args.output.format, args.output.record_width as usize)?;

    let report = artifact
        .used_values
        .as_ref()
        .map(output::report::render_used_values_json)
        .transpose()?;
    let mut outputs = vec![(args.output.out.as_path(), contents.as_str())];
    if let (Some(path), Some(report)) = (&args.output.export_json, &report) {
        outputs.push((path.as_path(), report.as_str()));
    }
    let inputs = args
        .layout
        .blocks
        .iter()
        .map(|block| block.layout.as_path())
        .chain(args.data.path())
        .collect::<Vec<_>>();
    write_files(&inputs, &outputs)?;

    Ok(artifact.stats)
}
