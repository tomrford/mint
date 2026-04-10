use serde::Deserialize;
use std::collections::HashMap;

/// Top-level `[mint]` configuration section.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MintConfig {
    pub endianness: Endianness,
    #[serde(default = "default_offset")]
    pub virtual_offset: u32,
    #[serde(default)]
    pub checksum: HashMap<String, ChecksumConfig>,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Endianness {
    Little,
    Big,
}

/// Named checksum algorithm configuration, referenced by leaf entries via `checksum = "name"`.
/// All fields are required — no inheritance or merging.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ChecksumConfig {
    pub polynomial: u32,
    pub start: u32,
    pub xor_out: u32,
    pub ref_in: bool,
    pub ref_out: bool,
}

fn default_offset() -> u32 {
    0
}

pub trait EndianBytes {
    fn to_endian_bytes(self, endianness: &Endianness) -> Vec<u8>;
}

macro_rules! impl_endian_bytes {
    ($($t:ty),* $(,)?) => {$(
        impl EndianBytes for $t {
            fn to_endian_bytes(self, e: &Endianness) -> Vec<u8> {
                match e {
                    Endianness::Little => self.to_le_bytes().to_vec(),
                    Endianness::Big => self.to_be_bytes().to_vec(),
                }
            }
        }
    )*};
}
impl_endian_bytes!(u8, u16, u32, u64, i8, i16, i32, i64, f32, f64);
