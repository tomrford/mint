use std::fmt;
use std::str::FromStr;

use serde::Deserialize;

use super::error::LayoutError;
use super::scalar_type::ScalarType;

/// Named ABI profile selected by a layout's `[mint].abi` setting.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Abi {
    GenericLe,
    GenericBe,
}

/// Shared rule set used by one or more named ABI profiles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbiFamily {
    GenericNatural,
}

/// Byte order used to encode multi-byte scalar values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Endianness {
    Little,
    Big,
}

pub(crate) trait EndianBytes {
    fn to_endian_bytes(self, endianness: Endianness) -> Vec<u8>;
}

macro_rules! impl_endian_bytes {
    ($($type:ty),* $(,)?) => {$(
        impl EndianBytes for $type {
            fn to_endian_bytes(self, endianness: Endianness) -> Vec<u8> {
                match endianness {
                    Endianness::Little => self.to_le_bytes().to_vec(),
                    Endianness::Big => self.to_be_bytes().to_vec(),
                }
            }
        }
    )*};
}
impl_endian_bytes!(u8, u16, u32, u64, i8, i16, i32, i64, f32, f64);

/// Effective layout properties for one scalar type under an ABI profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScalarAbi {
    /// Bytes occupied by one encoded scalar value.
    pub storage_size: usize,
    /// Byte alignment required for the scalar.
    pub alignment: usize,
    /// Byte distance between adjacent values in an array.
    pub array_stride: usize,
    /// C spelling used in generated headers.
    pub c_type: &'static str,
}

/// Effective rules provided by a named ABI profile.
pub trait AbiSpec: Copy {
    fn name(self) -> &'static str;
    fn family(self) -> AbiFamily;
    fn description(self) -> &'static str;
    fn endianness(self) -> Endianness;
    fn address_unit_bits(self) -> usize;
    fn scalar(self, scalar: ScalarType) -> Result<ScalarAbi, LayoutError>;

    fn offset_to_address_units(self, offset: usize) -> Result<u64, LayoutError> {
        let address_unit_bits = self.address_unit_bits();
        if address_unit_bits == 0 || !address_unit_bits.is_multiple_of(8) {
            return Err(LayoutError::InvalidLayout(format!(
                "ABI '{}' has an invalid {address_unit_bits}-bit addressable unit",
                self.name()
            )));
        }
        let unit_octets = address_unit_bits / 8;
        if !offset.is_multiple_of(unit_octets) {
            return Err(LayoutError::InvalidLayout(format!(
                "offset {offset} bytes cannot be represented in ABI '{}' with {}-bit addressable units",
                self.name(),
                address_unit_bits
            )));
        }
        u64::try_from(offset / unit_octets)
            .map_err(|_| LayoutError::InvalidLayout("address offset exceeds u64".to_owned()))
    }
}

impl Abi {
    /// Profiles accepted by layout parsing and the CLI.
    pub const ALL: [Self; 2] = [Self::GenericLe, Self::GenericBe];

    /// Human-readable scalar types accepted by this profile.
    pub fn supported_scalar_types(self) -> &'static str {
        match self.family() {
            AbiFamily::GenericNatural => {
                "u8, u16, u32, u64, i8, i16, i32, i64, f32, f64, fixed-point"
            }
        }
    }
}

impl AbiSpec for Abi {
    fn name(self) -> &'static str {
        match self {
            Self::GenericLe => "generic-le",
            Self::GenericBe => "generic-be",
        }
    }

    fn family(self) -> AbiFamily {
        AbiFamily::GenericNatural
    }

    fn description(self) -> &'static str {
        match self {
            Self::GenericLe => "Natural-width, byte-addressed C layout with little-endian values",
            Self::GenericBe => "Natural-width, byte-addressed C layout with big-endian values",
        }
    }

    fn endianness(self) -> Endianness {
        match self {
            Self::GenericLe => Endianness::Little,
            Self::GenericBe => Endianness::Big,
        }
    }

    fn address_unit_bits(self) -> usize {
        8
    }

    fn scalar(self, scalar: ScalarType) -> Result<ScalarAbi, LayoutError> {
        Ok(self.family().scalar(scalar))
    }
}

impl AbiFamily {
    pub const fn name(self) -> &'static str {
        match self {
            Self::GenericNatural => "generic-natural",
        }
    }

    fn scalar(self, scalar: ScalarType) -> ScalarAbi {
        match self {
            Self::GenericNatural => natural_scalar(scalar),
        }
    }
}

impl fmt::Display for Abi {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl fmt::Display for AbiFamily {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl fmt::Display for Endianness {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Little => "little",
            Self::Big => "big",
        })
    }
}

impl FromStr for Abi {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|abi| abi.name() == value)
            .ok_or_else(|| {
                format!(
                    "unknown ABI '{value}'; supported ABIs are {}",
                    Self::ALL
                        .iter()
                        .map(|abi| abi.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
    }
}

fn natural_scalar(scalar: ScalarType) -> ScalarAbi {
    let (storage_size, c_type) = match scalar {
        ScalarType::U8 => (1, "uint8_t"),
        ScalarType::U16 => (2, "uint16_t"),
        ScalarType::U32 => (4, "uint32_t"),
        ScalarType::U64 => (8, "uint64_t"),
        ScalarType::I8 => (1, "int8_t"),
        ScalarType::I16 => (2, "int16_t"),
        ScalarType::I32 => (4, "int32_t"),
        ScalarType::I64 => (8, "int64_t"),
        ScalarType::F32 => (4, "float"),
        ScalarType::F64 => (8, "double"),
        ScalarType::Fixed(fixed) if fixed.signed => match fixed.total_bits {
            8 => (1, "int8_t"),
            16 => (2, "int16_t"),
            32 => (4, "int32_t"),
            64 => (8, "int64_t"),
            _ => unreachable!(),
        },
        ScalarType::Fixed(fixed) => match fixed.total_bits {
            8 => (1, "uint8_t"),
            16 => (2, "uint16_t"),
            32 => (4, "uint32_t"),
            64 => (8, "uint64_t"),
            _ => unreachable!(),
        },
    };
    ScalarAbi {
        storage_size,
        alignment: storage_size,
        array_stride: storage_size,
        c_type,
    }
}

#[cfg(test)]
mod tests {
    use super::{Abi, AbiSpec, Endianness};
    use crate::layout::scalar_type::ScalarType;

    #[test]
    fn names_round_trip() {
        for abi in Abi::ALL {
            assert_eq!(abi.name().parse::<Abi>(), Ok(abi));
        }
    }

    #[test]
    fn generic_profiles_share_layout_but_not_byte_order() {
        let little = Abi::GenericLe.scalar(ScalarType::U32).unwrap();
        let big = Abi::GenericBe.scalar(ScalarType::U32).unwrap();
        assert_eq!(little, big);
        assert_eq!(Abi::GenericLe.endianness(), Endianness::Little);
        assert_eq!(Abi::GenericBe.endianness(), Endianness::Big);
    }

    #[test]
    fn byte_addressed_offsets_are_unchanged() {
        assert_eq!(Abi::GenericLe.offset_to_address_units(7).unwrap(), 7);
    }
}
