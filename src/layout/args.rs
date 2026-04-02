use super::error::LayoutError;
use clap::Args;

#[derive(Debug, Clone)]
pub struct BlockNames {
    pub name: String,
    pub file: String,
    // TODO: remove this field and the `block@file` compatibility path after the
    // legacy selector syntax is fully deprecated.
    pub legacy_syntax: bool,
}

fn parse_selected_arg(
    name: &str,
    file: &str,
    original: &str,
    legacy_syntax: bool,
) -> Result<BlockNames, LayoutError> {
    if name.is_empty() || file.is_empty() {
        return Err(LayoutError::InvalidBlockArgument(format!(
            "invalid selector '{original}'; use FILE or FILE#BLOCK"
        )));
    }

    Ok(BlockNames {
        name: name.to_string(),
        file: file.to_string(),
        legacy_syntax,
    })
}

pub fn parse_block_arg(block: &str) -> Result<BlockNames, LayoutError> {
    if let Some((file, name)) = block.rsplit_once('#') {
        return parse_selected_arg(name, file, block, false);
    }

    if let Some((name, file)) = block.split_once('@') {
        return parse_selected_arg(name, file, block, true);
    }

    Ok(BlockNames {
        name: String::new(),
        file: block.to_string(),
        legacy_syntax: false,
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

impl LayoutArgs {
    pub fn uses_legacy_block_syntax(&self) -> bool {
        self.blocks.iter().any(|block| block.legacy_syntax)
    }
}
