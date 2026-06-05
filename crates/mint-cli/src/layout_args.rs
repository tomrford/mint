use clap::Args;
use mint_core::build::BlockSelector;
use mint_core::layout::error::LayoutError;

pub fn parse_block_arg(block: &str) -> Result<BlockSelector, LayoutError> {
    if let Some((file, name)) = block.rsplit_once('#') {
        if name.is_empty() || file.is_empty() {
            return Err(LayoutError::InvalidBlockArgument(format!(
                "invalid selector '{block}'; use FILE or FILE#BLOCK"
            )));
        }

        return Ok(BlockSelector::named(file, name));
    }

    Ok(BlockSelector::all(block))
}

#[derive(Args, Debug)]
pub struct LayoutArgs {
    #[arg(value_name = "FILE[#BLOCK] | FILE", num_args = 1.., value_parser = parse_block_arg, help = "One or more layout selectors as file[#block] or a layout file (typically .toml) to build all blocks")]
    pub blocks: Vec<BlockSelector>,

    #[arg(
        long,
        help = "Enable strict type conversions; disallow lossy casts during bytestream assembly",
        default_value_t = false
    )]
    pub strict: bool,
}
