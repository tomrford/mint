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
    #[arg(value_name = "FILE[#BLOCK] | FILE", required = true, num_args = 1.., value_parser = parse_block_arg, help = "Layout files (every block) or FILE#BLOCK selectors; a block selected twice is built once")]
    pub blocks: Vec<BlockSelector>,

    #[arg(
        long,
        help = "Fail on lossy value conversions instead of truncating, saturating or clamping",
        default_value_t = false
    )]
    pub strict: bool,
}
