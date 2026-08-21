use std::fmt;
use std::str::FromStr;

use crate::diagnostic::{Category, Error};
use crate::source::{Source, Span};

/// Named ABI profile selected by `@mint abi`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Abi {
    GenericLe,
    GenericBe,
    ArmAapcs32Le,
    TricoreEabiLe,
    RiscvIlp32Le,
    TiC28xEabi,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbiFamily {
    GenericNatural,
    NaturalAlign4,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Endianness {
    Little,
    Big,
}

/// Exact-width scalar representations accepted by Neo.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scalar {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScalarAbi {
    pub storage_size: usize,
    pub alignment: usize,
    pub array_stride: usize,
}

impl Scalar {
    pub const ALL: [Self; 10] = [
        Self::U8,
        Self::I8,
        Self::U16,
        Self::I16,
        Self::U32,
        Self::I32,
        Self::U64,
        Self::I64,
        Self::F32,
        Self::F64,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }

    pub fn c_name(self) -> &'static str {
        match self {
            Self::U8 => "uint8_t",
            Self::U16 => "uint16_t",
            Self::U32 => "uint32_t",
            Self::U64 => "uint64_t",
            Self::I8 => "int8_t",
            Self::I16 => "int16_t",
            Self::I32 => "int32_t",
            Self::I64 => "int64_t",
            Self::F32 => "float",
            Self::F64 => "double",
        }
    }

    pub fn size_bytes(self) -> usize {
        match self {
            Self::U8 | Self::I8 => 1,
            Self::U16 | Self::I16 => 2,
            Self::U32 | Self::I32 | Self::F32 => 4,
            Self::U64 | Self::I64 | Self::F64 => 8,
        }
    }

    pub fn is_signed(self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64)
    }

    /// Inclusive integer range, if this scalar is an integer type.
    pub fn integer_range(self) -> Option<(i128, i128)> {
        Some(match self {
            Self::U8 => (0, i128::from(u8::MAX)),
            Self::U16 => (0, i128::from(u16::MAX)),
            Self::U32 => (0, i128::from(u32::MAX)),
            Self::U64 => (0, i128::from(u64::MAX)),
            Self::I8 => (i128::from(i8::MIN), i128::from(i8::MAX)),
            Self::I16 => (i128::from(i16::MIN), i128::from(i16::MAX)),
            Self::I32 => (i128::from(i32::MIN), i128::from(i32::MAX)),
            Self::I64 => (i128::from(i64::MIN), i128::from(i64::MAX)),
            Self::F32 | Self::F64 => return None,
        })
    }

    pub fn hash_tag(self) -> u8 {
        match self {
            Self::U8 => 0,
            Self::U16 => 1,
            Self::U32 => 2,
            Self::U64 => 3,
            Self::I8 => 4,
            Self::I16 => 5,
            Self::I32 => 6,
            Self::I64 => 7,
            Self::F32 => 8,
            Self::F64 => 9,
        }
    }
}

impl fmt::Display for Scalar {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl Abi {
    pub const ALL: [Self; 6] = [
        Self::GenericLe,
        Self::GenericBe,
        Self::ArmAapcs32Le,
        Self::TricoreEabiLe,
        Self::RiscvIlp32Le,
        Self::TiC28xEabi,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::GenericLe => "generic-le",
            Self::GenericBe => "generic-be",
            Self::ArmAapcs32Le => "arm-aapcs32-le",
            Self::TricoreEabiLe => "tricore-eabi-le",
            Self::RiscvIlp32Le => "riscv-ilp32-le",
            Self::TiC28xEabi => "ti-c28x-eabi",
        }
    }

    pub fn family(self) -> AbiFamily {
        match self {
            Self::GenericLe | Self::GenericBe | Self::ArmAapcs32Le | Self::RiscvIlp32Le => {
                AbiFamily::GenericNatural
            }
            Self::TricoreEabiLe | Self::TiC28xEabi => AbiFamily::NaturalAlign4,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::GenericLe => "Natural-width C layout with little-endian values",
            Self::GenericBe => "Natural-width C layout with big-endian values",
            Self::ArmAapcs32Le => "ARM AAPCS32 layout with little-endian values",
            Self::TricoreEabiLe => {
                "Infineon TriCore EABI layout with little-endian values and 4-byte 64-bit alignment"
            }
            Self::RiscvIlp32Le => "RISC-V ILP32 layout with little-endian values",
            Self::TiC28xEabi => {
                "TI C28x EABI layout with 16-bit address units and no exact-width 8-bit types"
            }
        }
    }

    pub fn endianness(self) -> Endianness {
        match self {
            Self::GenericBe => Endianness::Big,
            Self::GenericLe
            | Self::ArmAapcs32Le
            | Self::TricoreEabiLe
            | Self::RiscvIlp32Le
            | Self::TiC28xEabi => Endianness::Little,
        }
    }

    pub fn address_unit_bits(self) -> usize {
        match self {
            Self::TiC28xEabi => 16,
            _ => 8,
        }
    }

    pub fn address_unit_octets(self) -> usize {
        self.address_unit_bits() / 8
    }

    pub fn output_addressing(self) -> &'static str {
        match self {
            Self::TiC28xEabi => "octet addresses (2 × target word address; standard Intel HEX)",
            _ => "octet addresses (standard Intel HEX)",
        }
    }

    pub fn guarantees_ieee(self) -> bool {
        !matches!(self, Self::TiC28xEabi)
    }

    pub fn scalar(self, scalar: Scalar) -> Result<ScalarAbi, String> {
        if self == Self::TiC28xEabi && scalar.size_bytes() == 1 {
            return Err(format!(
                "ABI '{}' does not support scalar type {scalar}; TI C28x EABI has 16-bit char and no exact-width 8-bit C type",
                self.name()
            ));
        }
        Ok(self.family().scalar(scalar))
    }

    pub fn offset_to_address_units(self, offset: usize) -> Result<u64, String> {
        let unit_octets = self.address_unit_octets();
        if !offset.is_multiple_of(unit_octets) {
            return Err(format!(
                "offset {offset} bytes cannot be represented in ABI '{}' with {}-bit addressable units",
                self.name(),
                self.address_unit_bits()
            ));
        }
        u64::try_from(offset / unit_octets).map_err(|_| "address offset exceeds u64".to_owned())
    }
}

impl AbiFamily {
    pub const fn name(self) -> &'static str {
        match self {
            Self::GenericNatural => "generic-natural",
            Self::NaturalAlign4 => "natural-align4",
        }
    }

    pub const fn aggregate_rules(self) -> &'static str {
        match self {
            Self::GenericNatural => {
                "aggregates align to their maximum member alignment and pad tails to that alignment"
            }
            Self::NaturalAlign4 => {
                "aggregates align to their maximum member alignment, raised to 2 octets when larger than one octet, and pad tails to that alignment"
            }
        }
    }

    pub const fn min_aggregate_alignment(self) -> usize {
        match self {
            Self::GenericNatural => 1,
            Self::NaturalAlign4 => 2,
        }
    }

    fn scalar(self, scalar: Scalar) -> ScalarAbi {
        let storage_size = scalar.size_bytes();
        let alignment = match self {
            Self::GenericNatural => storage_size,
            Self::NaturalAlign4 => storage_size.min(4),
        };
        ScalarAbi {
            storage_size,
            alignment,
            array_stride: storage_size,
        }
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

pub fn parse_abi(name: &str, source: &Source, span: Span) -> Result<Abi, Error> {
    name.parse::<Abi>()
        .map_err(|message| Error::schema(source, span, message))
}

pub fn list_text() -> String {
    let mut out = String::new();
    for abi in Abi::ALL {
        out.push_str(&format!("{:<18} {}\n", abi.name(), abi.description()));
    }
    out
}

pub fn show_text(name: &str) -> Result<String, Error> {
    let abi = name
        .parse::<Abi>()
        .map_err(|message| Error::named(Category::Usage, name, message))?;
    let mut out = String::new();
    out.push_str(&format!("name: {}\n", abi.name()));
    out.push_str(&format!("family: {}\n", abi.family().name()));
    out.push_str(&format!("description: {}\n", abi.description()));
    out.push_str(&format!("byte order: {}\n", abi.endianness()));
    out.push_str(&format!(
        "target addressable unit: {} bits\n",
        abi.address_unit_bits()
    ));
    out.push_str(&format!("output addresses: {}\n", abi.output_addressing()));
    out.push_str(&format!(
        "aggregate rules: {}\n",
        abi.family().aggregate_rules()
    ));
    out.push('\n');
    out.push_str("type  storage  alignment  stride  C type\n");
    for scalar in Scalar::ALL {
        match abi.scalar(scalar) {
            Ok(layout) => out.push_str(&format!(
                "{:<4}  {:>7}  {:>9}  {:>6}  {}\n",
                scalar,
                layout.storage_size,
                layout.alignment,
                layout.array_stride,
                scalar.c_name()
            )),
            Err(_) => out.push_str(&format!("{scalar:<4}  unsupported\n")),
        }
    }
    out.push_str("all sizes, alignments and strides are in octets\n");
    out.push_str("float32_t and float64_t select IEEE-754 binary32 and binary64 on every ABI\n");
    if !abi.guarantees_ieee() {
        out.push_str("C float and double are rejected on this ABI; use float32_t or float64_t\n");
    }
    Ok(out)
}

pub fn write_scalar_bytes(
    scalar: Scalar,
    endianness: Endianness,
    bytes: &mut [u8],
    value: ScalarValue,
) {
    match (scalar, value) {
        (Scalar::U8, ScalarValue::U(value)) => bytes[0] = value as u8,
        (Scalar::I8, ScalarValue::I(value)) => bytes[0] = value as u8,
        (Scalar::U16, ScalarValue::U(value)) => {
            put(
                bytes,
                endianness,
                (value as u16).to_le_bytes(),
                (value as u16).to_be_bytes(),
            );
        }
        (Scalar::U32, ScalarValue::U(value)) => {
            put(
                bytes,
                endianness,
                (value as u32).to_le_bytes(),
                (value as u32).to_be_bytes(),
            );
        }
        (Scalar::U64, ScalarValue::U(value)) => {
            put(bytes, endianness, value.to_le_bytes(), value.to_be_bytes());
        }
        (Scalar::I16, ScalarValue::I(value)) => {
            put(
                bytes,
                endianness,
                (value as i16).to_le_bytes(),
                (value as i16).to_be_bytes(),
            );
        }
        (Scalar::I32, ScalarValue::I(value)) => {
            put(
                bytes,
                endianness,
                (value as i32).to_le_bytes(),
                (value as i32).to_be_bytes(),
            );
        }
        (Scalar::I64, ScalarValue::I(value)) => {
            put(bytes, endianness, value.to_le_bytes(), value.to_be_bytes());
        }
        (Scalar::F32, ScalarValue::F(value)) => {
            put(
                bytes,
                endianness,
                (value as f32).to_le_bytes(),
                (value as f32).to_be_bytes(),
            );
        }
        (Scalar::F64, ScalarValue::F(value)) => {
            put(bytes, endianness, value.to_le_bytes(), value.to_be_bytes());
        }
        _ => {}
    }
}

fn put<const N: usize>(bytes: &mut [u8], endianness: Endianness, le: [u8; N], be: [u8; N]) {
    bytes.copy_from_slice(&match endianness {
        Endianness::Little => le,
        Endianness::Big => be,
    });
}

#[derive(Clone, Copy, Debug)]
pub enum ScalarValue {
    U(u64),
    I(i64),
    F(f64),
}

#[cfg(test)]
mod tests {
    use super::{Abi, Scalar};

    #[test]
    fn c28x_rejects_8_bit_and_aligns_u64_to_4() {
        assert!(Abi::TiC28xEabi.scalar(Scalar::U8).is_err());
        let scalar = Abi::TiC28xEabi.scalar(Scalar::U64).unwrap();
        assert_eq!(scalar.storage_size, 8);
        assert_eq!(scalar.alignment, 4);
        assert_eq!(scalar.array_stride, 8);
    }
}
