use super::error::LayoutError;
use clap::Args;

#[derive(Debug, Clone)]
pub struct BlockNames {
    pub name: String,
    pub file: String,
}

pub fn parse_block_arg(block: &str) -> Result<BlockNames, LayoutError> {
    let parts: Vec<&str> = block.split('@').collect();

    match parts.len() {
        2 => Ok(BlockNames {
            name: parts[0].to_string(),
            file: parts[1].to_string(),
        }),
        1 => Ok(BlockNames {
            name: String::new(),
            file: parts[0].to_string(),
        }),
        _ => Err(LayoutError::InvalidBlockArgument(format!(
            "Failed to unpack block {}",
            block
        ))),
    }
}

#[derive(Args, Debug)]
pub struct LayoutArgs {
    #[arg(value_name = "BLOCK@FILE | FILE", num_args = 1.., value_parser = parse_block_arg, help = "One or more blocks as name@layout_file or a layout file (typically .toml) to build all blocks")]
    pub blocks: Vec<BlockNames>,

    #[arg(
        long,
        help = "Enable strict type conversions; disallow lossy casts during bytestream assembly",
        default_value_t = false
    )]
    pub strict: bool,
}
