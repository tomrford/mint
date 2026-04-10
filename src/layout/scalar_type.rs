use std::fmt;
use std::str::FromStr;

use serde::Deserialize;
use serde::Deserializer;

use super::error::LayoutError;

/// Scalar type enum derived from 'type' string in leaf entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarType {
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
    Fixed(FixedPointType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedPointType {
    pub signed: bool,
    pub integer_bits: u8,
    pub fractional_bits: u8,
    pub total_bits: u8,
}

impl FixedPointType {
    pub fn size_bytes(&self) -> usize {
        usize::from(self.total_bits / 8)
    }

    pub fn storage_label(&self) -> String {
        format!(
            "{} {}-bit storage",
            if self.signed { "signed" } else { "unsigned" },
            self.total_bits
        )
    }

    pub fn encoded_bounds(&self) -> (i128, i128) {
        if self.signed {
            let half = 1i128 << (self.total_bits - 1);
            (-half, half - 1)
        } else {
            (0, (1i128 << self.total_bits) - 1)
        }
    }
}

impl ScalarType {
    /// Returns the size of the scalar type in bytes.
    pub fn size_bytes(&self) -> usize {
        match self {
            ScalarType::U8 | ScalarType::I8 => 1,
            ScalarType::U16 | ScalarType::I16 => 2,
            ScalarType::U32 | ScalarType::I32 | ScalarType::F32 => 4,
            ScalarType::U64 | ScalarType::I64 | ScalarType::F64 => 8,
            ScalarType::Fixed(fixed) => fixed.size_bytes(),
        }
    }

    /// Returns true if this is an integer storage type supported for bitmaps.
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            ScalarType::U8
                | ScalarType::U16
                | ScalarType::U32
                | ScalarType::U64
                | ScalarType::I8
                | ScalarType::I16
                | ScalarType::I32
                | ScalarType::I64
        )
    }

    /// Returns true if this is an unsigned integer type.
    pub fn is_unsigned_integer(&self) -> bool {
        matches!(
            self,
            ScalarType::U8 | ScalarType::U16 | ScalarType::U32 | ScalarType::U64
        )
    }

    /// Returns true if this is a signed type.
    pub fn is_signed(&self) -> bool {
        matches!(
            self,
            ScalarType::I8 | ScalarType::I16 | ScalarType::I32 | ScalarType::I64
        )
    }

    pub fn fixed_point(&self) -> Option<FixedPointType> {
        match self {
            ScalarType::Fixed(fixed) => Some(*fixed),
            _ => None,
        }
    }

    /// Returns the type name as a string.
    pub fn name(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for ScalarType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScalarType::U8 => write!(f, "u8"),
            ScalarType::U16 => write!(f, "u16"),
            ScalarType::U32 => write!(f, "u32"),
            ScalarType::U64 => write!(f, "u64"),
            ScalarType::I8 => write!(f, "i8"),
            ScalarType::I16 => write!(f, "i16"),
            ScalarType::I32 => write!(f, "i32"),
            ScalarType::I64 => write!(f, "i64"),
            ScalarType::F32 => write!(f, "f32"),
            ScalarType::F64 => write!(f, "f64"),
            ScalarType::Fixed(fixed) => write!(f, "{fixed}"),
        }
    }
}

impl fmt::Display for FixedPointType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.signed {
            write!(f, "q{}.{}", self.integer_bits, self.fractional_bits)
        } else {
            write!(f, "uq{}.{}", self.integer_bits, self.fractional_bits)
        }
    }
}

impl FromStr for ScalarType {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "u8" => return Ok(ScalarType::U8),
            "u16" => return Ok(ScalarType::U16),
            "u32" => return Ok(ScalarType::U32),
            "u64" => return Ok(ScalarType::U64),
            "i8" => return Ok(ScalarType::I8),
            "i16" => return Ok(ScalarType::I16),
            "i32" => return Ok(ScalarType::I32),
            "i64" => return Ok(ScalarType::I64),
            "f32" => return Ok(ScalarType::F32),
            "f64" => return Ok(ScalarType::F64),
            _ => {}
        }

        parse_fixed_point_type(value).map(ScalarType::Fixed)
    }
}

impl<'de> Deserialize<'de> for ScalarType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        ScalarType::from_str(&raw).map_err(serde::de::Error::custom)
    }
}

fn parse_fixed_point_type(value: &str) -> Result<FixedPointType, String> {
    let (signed, body) = if let Some(rest) = value.strip_prefix("uq") {
        (false, rest)
    } else if let Some(rest) = value.strip_prefix('q') {
        (true, rest)
    } else {
        return Err(format!("unknown scalar type '{value}'"));
    };

    let mut parts = body.split('.');
    let integer_bits = parts.next().unwrap_or_default();
    let fractional_bits = parts.next().unwrap_or_default();
    if integer_bits.is_empty()
        || fractional_bits.is_empty()
        || parts.next().is_some()
        || !integer_bits.chars().all(|c| c.is_ascii_digit())
        || !fractional_bits.chars().all(|c| c.is_ascii_digit())
    {
        return Err(format!(
            "invalid fixed-point type '{value}'; expected qI.F or uqI.F with non-negative integer bit counts"
        ));
    }

    let integer_bits = integer_bits
        .parse::<u8>()
        .map_err(|_| format!("invalid fixed-point type '{value}'; integer bits must fit in u8"))?;
    let fractional_bits = fractional_bits.parse::<u8>().map_err(|_| {
        format!("invalid fixed-point type '{value}'; fractional bits must fit in u8")
    })?;

    let total_bits = if signed {
        1u8.checked_add(integer_bits)
            .and_then(|bits| bits.checked_add(fractional_bits))
    } else {
        integer_bits.checked_add(fractional_bits)
    }
    .ok_or_else(|| format!("invalid fixed-point type '{value}'; total width overflowed"))?;

    if !matches!(total_bits, 8 | 16 | 32 | 64) {
        return Err(format!(
            "unsupported fixed-point width in type '{value}'; total width must be 8, 16, 32, or 64 bits"
        ));
    }

    Ok(FixedPointType {
        signed,
        integer_bits,
        fractional_bits,
        total_bits,
    })
}

pub fn fixed_point_unsupported_error(kind: &str, scalar_type: ScalarType) -> LayoutError {
    LayoutError::DataValueExportFailed(format!(
        "{kind} does not support fixed-point storage type '{}'.",
        scalar_type
    ))
}

#[cfg(test)]
mod tests {
    use super::{FixedPointType, ScalarType};

    #[test]
    fn parses_builtin_scalar_types() {
        assert_eq!("u16".parse::<ScalarType>().unwrap(), ScalarType::U16);
        assert_eq!("f64".parse::<ScalarType>().unwrap(), ScalarType::F64);
    }

    #[test]
    fn parses_fixed_point_types_with_matching_widths() {
        assert_eq!(
            "uq0.16".parse::<ScalarType>().unwrap(),
            ScalarType::Fixed(FixedPointType {
                signed: false,
                integer_bits: 0,
                fractional_bits: 16,
                total_bits: 16,
            })
        );
        assert_eq!(
            "q15.16".parse::<ScalarType>().unwrap(),
            ScalarType::Fixed(FixedPointType {
                signed: true,
                integer_bits: 15,
                fractional_bits: 16,
                total_bits: 32,
            })
        );
    }

    #[test]
    fn rejects_malformed_fixed_point_types() {
        for value in ["q8", "q8.8.8", "q16.-1", "uq", "uq8."] {
            let err = value.parse::<ScalarType>().expect_err("type should fail");
            assert!(
                err.contains("invalid fixed-point type"),
                "expected targeted parse error for {value}, got: {err}"
            );
        }
    }

    #[test]
    fn rejects_unsupported_fixed_point_widths() {
        let err = "q3.10".parse::<ScalarType>().expect_err("type should fail");
        assert!(
            err.contains("unsupported fixed-point width"),
            "expected width error, got: {err}"
        );
    }
}
