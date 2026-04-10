use super::error::LayoutError;
use clap::Args;

#[derive(Debug, Clone)]
pub struct BlockNames {
    pub name: String,
    pub file: String,
}

pub fn parse_block_arg(block: &str) -> Result<BlockNames, LayoutError> {
    if let Some((file, name)) = block.rsplit_once('#') {
        if name.is_empty() || file.is_empty() {
            return Err(LayoutError::InvalidBlockArgument(format!(
                "invalid selector '{block}'; use FILE or FILE#BLOCK"
            )));
        }

        return Ok(BlockNames {
            name: name.to_string(),
            file: file.to_string(),
        });
    }

    Ok(BlockNames {
        name: String::new(),
        file: block.to_string(),
    })
}

#[derive(Args, Debug)]
pub struct LayoutArgs {
    #[arg(value_name = "FILE[#BLOCK] | FILE", num_args = 1.., value_parser = parse_block_arg, help = "One or more layout selectors as file[#block] or a layout file (typically .toml) to build all blocks")]
    pub blocks: Vec<BlockNames>,

    #[arg(
        long,
        help = "Enable strict type conversions; disallow lossy casts during bytestream assembly",
        default_value_t = false
    )]
    pub strict: bool,
}
