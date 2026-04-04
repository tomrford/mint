use serde::Deserialize;

/// Block header defining memory region.
#[derive(Debug, Deserialize)]
pub struct Header {
    pub start_address: u32,
    pub length: u32,
    #[serde(default = "default_padding")]
    pub padding: u8,
}

fn default_padding() -> u8 {
    0xFF
}
