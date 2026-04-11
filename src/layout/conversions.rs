use super::error::LayoutError;
use super::scalar_type::{FixedPointType, ScalarType};
use super::settings::{EndianBytes, Endianness};
use super::value::DataValue;
use std::fmt;

macro_rules! impl_try_from_data_value {
    ($($t:ty),* $(,)?) => {$(
        impl TryFrom<&DataValue> for $t {
            type Error = LayoutError;
            fn try_from(value: &DataValue) -> Result<Self, LayoutError> {
                match value {
                    DataValue::Bool(val) => {
                        let n: u8 = if *val { 1 } else { 0 };
                        Ok(n as $t)
                    }
                    DataValue::U64(val) => Ok(*val as $t),
                    DataValue::I64(val) => Ok(*val as $t),
                    DataValue::F64(val) => Ok(*val as $t),
                    DataValue::Str(_) => {
                        return Err(LayoutError::DataValueExportFailed(
                            "Cannot convert string to scalar type.".to_string(),
                        ));
                    }
                }
            }
        }
    )* }; }

impl_try_from_data_value!(u8, u16, u32, u64, i8, i16, i32, i64, i128, f32, f64);

pub trait TryFromStrict<T>: Sized {
    fn try_from_strict(value: T) -> Result<Self, LayoutError>;
}

macro_rules! err {
    ($msg:expr) => {
        LayoutError::DataValueExportFailed($msg.to_string())
    };
}

macro_rules! impl_try_from_strict_unsigned {
    ($($t:ty),* $(,)?) => {$(
        impl TryFromStrict<&DataValue> for $t {
            fn try_from_strict(value: &DataValue) -> Result<Self, LayoutError> {
                match value {
                    DataValue::U64(v) => <Self as TryFrom<u64>>::try_from(*v)
                        .map_err(|_| err!(format!("u64 value {} out of range for {}", v, stringify!($t)))),
                    DataValue::I64(v) => {
                        if *v < 0 { return Err(err!("negative integer cannot convert to unsigned in strict mode")); }
                        <Self as TryFrom<u64>>::try_from(*v as u64)
                            .map_err(|_| err!(format!("i64 value {} out of range for {}", v, stringify!($t))))
                    }
                    DataValue::F64(v) => {
                        if !v.is_finite() { return Err(err!("non-finite float cannot convert to integer in strict mode")); }
                        if v.fract() != 0.0 { return Err(err!("float to integer conversion not allowed unless value is an exact integer")); }
                        if *v < 0.0 || *v > (<$t>::MAX as f64) { return Err(err!(format!("float value {} out of range for {}", v, stringify!($t)))); }
                        Ok(*v as $t)
                    }
                    DataValue::Bool(b) => {
                        let n: u8 = if *b { 1 } else { 0 };
                        Ok(n as $t)
                    }
                    DataValue::Str(_) => Err(err!("Cannot convert string to scalar type.")),
                }
            }
        }
    )*};
}

macro_rules! impl_try_from_strict_signed {
    ($($t:ty),* $(,)?) => {$(
        impl TryFromStrict<&DataValue> for $t {
            fn try_from_strict(value: &DataValue) -> Result<Self, LayoutError> {
                match value {
                    DataValue::U64(v) => {
                        <Self as TryFrom<i128>>::try_from(*v as i128)
                            .map_err(|_| err!(format!("u64 value {} out of range for {}", v, stringify!($t))))
                    }
                    DataValue::I64(v) => <Self as TryFrom<i64>>::try_from(*v)
                        .map_err(|_| err!(format!("i64 value {} out of range for {}", v, stringify!($t)))),
                    DataValue::F64(v) => {
                        if !v.is_finite() { return Err(err!("non-finite float cannot convert to integer in strict mode")); }
                        if v.fract() != 0.0 { return Err(err!("float to integer conversion not allowed unless value is an exact integer")); }
                        if *v < (<$t>::MIN as f64) || *v > (<$t>::MAX as f64) { return Err(err!(format!("float value {} out of range for {}", v, stringify!($t)))); }
                        Ok(*v as $t)
                    }
                    DataValue::Bool(b) => {
                        let n: u8 = if *b { 1 } else { 0 };
                        Ok(n as $t)
                    }
                    DataValue::Str(_) => Err(err!("Cannot convert string to scalar type.")),
                }
            }
        }
    )*};
}

macro_rules! impl_try_from_strict_float_targets {
    ($t:ty) => {
        impl TryFromStrict<&DataValue> for $t {
            fn try_from_strict(value: &DataValue) -> Result<Self, LayoutError> {
                match value {
                    DataValue::F64(v) => {
                        if !v.is_finite() {
                            return Err(err!("non-finite float not allowed in strict mode"));
                        }
                        let out = *v as $t;
                        if out.is_finite() {
                            Ok(out)
                        } else {
                            Err(err!(format!(
                                "float value {} out of range for {}",
                                v,
                                stringify!($t)
                            )))
                        }
                    }
                    DataValue::U64(v) => {
                        let out = (*v as $t);
                        if !out.is_finite() {
                            return Err(err!("integer to float produced non-finite value"));
                        }
                        // exactness check via round-trip
                        if (out as u64) == *v {
                            Ok(out)
                        } else {
                            Err(err!(
                                "lossy integer to float conversion not allowed in strict mode"
                            ))
                        }
                    }
                    DataValue::I64(v) => {
                        let out = (*v as $t);
                        if !out.is_finite() {
                            return Err(err!("integer to float produced non-finite value"));
                        }
                        if (out as i64) == *v {
                            Ok(out)
                        } else {
                            Err(err!(
                                "lossy integer to float conversion not allowed in strict mode"
                            ))
                        }
                    }
                    DataValue::Bool(b) => {
                        let out: $t = if *b { 1.0 } else { 0.0 };
                        Ok(out)
                    }
                    DataValue::Str(_) => Err(err!("Cannot convert string to scalar type.")),
                }
            }
        }
    };
}

impl_try_from_strict_unsigned!(u8, u16, u32, u64);
impl_try_from_strict_signed!(i8, i16, i32, i64, i128);
impl_try_from_strict_float_targets!(f32);
impl TryFromStrict<&DataValue> for f64 {
    fn try_from_strict(value: &DataValue) -> Result<Self, LayoutError> {
        match value {
            DataValue::F64(v) => Ok(*v),
            DataValue::U64(v) => {
                let out = *v as f64;
                if (out as u64) == *v {
                    Ok(out)
                } else {
                    Err(err!(
                        "lossy integer to float conversion not allowed in strict mode"
                    ))
                }
            }
            DataValue::I64(v) => {
                let out = *v as f64;
                if (out as i64) == *v {
                    Ok(out)
                } else {
                    Err(err!(
                        "lossy integer to float conversion not allowed in strict mode"
                    ))
                }
            }
            DataValue::Bool(b) => {
                let out = if *b { 1.0 } else { 0.0 };
                Ok(out)
            }
            DataValue::Str(_) => Err(err!("Cannot convert string to scalar type.")),
        }
    }
}

/// Converts a DataValue to an i128 for bitfield packing, with range clamping/checking.
///
/// - `bits`: field width in bits (must be > 0)
/// - `signed`: whether to interpret as two's complement signed field
/// - `strict`: if true, out-of-range or non-integer floats produce errors; otherwise saturate
pub fn clamp_bitfield_value(
    value: &DataValue,
    bits: usize,
    signed: bool,
    strict: bool,
) -> Result<i128, LayoutError> {
    let raw: i128 = if strict {
        i128::try_from_strict(value)?
    } else {
        i128::try_from(value)?
    };

    let (min, max) = if signed {
        let half = 1i128 << (bits - 1);
        (-half, half - 1)
    } else {
        (0, (1i128 << bits) - 1)
    };

    if strict && (raw < min || raw > max) {
        return Err(LayoutError::BitfieldOutOfRange {
            value: raw,
            bits,
            signedness: if signed { "signed" } else { "unsigned" },
            min,
            max,
        });
    }
    Ok(raw.clamp(min, max))
}

fn data_value_display(value: &DataValue) -> String {
    match value {
        DataValue::Bool(value) => value.to_string(),
        DataValue::U64(value) => value.to_string(),
        DataValue::I64(value) => value.to_string(),
        DataValue::F64(value) => value.to_string(),
        DataValue::Str(value) => format!("{value:?}"),
    }
}

fn fixed_point_overflow_error(
    fixed: FixedPointType,
    value: &DataValue,
    scaled: impl fmt::Display,
) -> LayoutError {
    LayoutError::DataValueExportFailed(format!(
        "fixed-point type '{}' overflows {} for value {} (scaled to {})",
        fixed,
        fixed.storage_label(),
        data_value_display(value),
        scaled
    ))
}

fn fixed_point_non_finite_error(fixed: FixedPointType, value: &DataValue) -> LayoutError {
    LayoutError::DataValueExportFailed(format!(
        "fixed-point type '{}' cannot encode non-finite value {}",
        fixed,
        data_value_display(value)
    ))
}

fn encode_fixed_point_bytes(
    value: &DataValue,
    fixed: FixedPointType,
    endianness: &Endianness,
    strict: bool,
) -> Result<Vec<u8>, LayoutError> {
    let encoded = encode_fixed_point_value(value, fixed, strict)?;
    encode_integer_bytes(encoded, fixed, endianness)
}

fn encode_fixed_point_value(
    value: &DataValue,
    fixed: FixedPointType,
    strict: bool,
) -> Result<i128, LayoutError> {
    let (min, max) = fixed.encoded_bounds();
    let scale = 1i128 << fixed.fractional_bits;

    let encoded = match value {
        DataValue::Bool(raw) => clamp_fixed_point_integer(
            if *raw { 1 } else { 0 },
            scale,
            min,
            max,
            fixed,
            value,
            strict,
        )?,
        DataValue::U64(raw) => {
            clamp_fixed_point_integer(i128::from(*raw), scale, min, max, fixed, value, strict)?
        }
        DataValue::I64(raw) => {
            clamp_fixed_point_integer(i128::from(*raw), scale, min, max, fixed, value, strict)?
        }
        DataValue::F64(raw) => clamp_fixed_point_float(*raw, min, max, fixed, strict, value)?,
        DataValue::Str(_) => {
            return Err(LayoutError::DataValueExportFailed(
                "Cannot convert string to scalar type.".to_string(),
            ));
        }
    };

    Ok(encoded)
}

fn clamp_fixed_point_integer(
    raw: i128,
    scale: i128,
    min: i128,
    max: i128,
    fixed: FixedPointType,
    original: &DataValue,
    strict: bool,
) -> Result<i128, LayoutError> {
    let Some(scaled) = raw.checked_mul(scale) else {
        if strict {
            return Err(fixed_point_overflow_error(
                fixed,
                original,
                "integer scaling overflow",
            ));
        }
        return Ok(if raw.is_negative() { min } else { max });
    };

    if strict && (scaled < min || scaled > max) {
        return Err(fixed_point_overflow_error(fixed, original, scaled));
    }

    Ok(scaled.clamp(min, max))
}

fn clamp_fixed_point_float(
    raw: f64,
    min: i128,
    max: i128,
    fixed: FixedPointType,
    strict: bool,
    original: &DataValue,
) -> Result<i128, LayoutError> {
    if !raw.is_finite() {
        return Err(fixed_point_non_finite_error(fixed, original));
    }

    let scaled = raw * (2f64).powi(i32::from(fixed.fractional_bits));
    if !scaled.is_finite() {
        if strict {
            return Err(fixed_point_overflow_error(fixed, original, scaled));
        }
        return Ok(if scaled.is_sign_negative() { min } else { max });
    }

    let rounded = scaled.round_ties_even();
    let rounded_int = rounded as i128;
    if rounded_int < min {
        if strict {
            return Err(fixed_point_overflow_error(fixed, original, rounded));
        }
        return Ok(min);
    }
    if rounded_int > max {
        if strict {
            return Err(fixed_point_overflow_error(fixed, original, rounded));
        }
        return Ok(max);
    }

    Ok(rounded_int)
}

fn encode_integer_bytes(
    encoded: i128,
    fixed: FixedPointType,
    endianness: &Endianness,
) -> Result<Vec<u8>, LayoutError> {
    match (fixed.signed, fixed.total_bits) {
        (false, 8) => Ok((encoded as u8).to_endian_bytes(endianness)),
        (false, 16) => Ok((encoded as u16).to_endian_bytes(endianness)),
        (false, 32) => Ok((encoded as u32).to_endian_bytes(endianness)),
        (false, 64) => Ok(u64::try_from(encoded)
            .map_err(|_| {
                LayoutError::DataValueExportFailed(format!(
                    "fixed-point type '{}' encoded value {} could not be written as u64",
                    fixed, encoded
                ))
            })?
            .to_endian_bytes(endianness)),
        (true, 8) => Ok((encoded as i8).to_endian_bytes(endianness)),
        (true, 16) => Ok((encoded as i16).to_endian_bytes(endianness)),
        (true, 32) => Ok((encoded as i32).to_endian_bytes(endianness)),
        (true, 64) => Ok(i64::try_from(encoded)
            .map_err(|_| {
                LayoutError::DataValueExportFailed(format!(
                    "fixed-point type '{}' encoded value {} could not be written as i64",
                    fixed, encoded
                ))
            })?
            .to_endian_bytes(endianness)),
        _ => Err(LayoutError::DataValueExportFailed(format!(
            "unsupported fixed-point width '{}'",
            fixed
        ))),
    }
}

pub fn convert_value_to_bytes(
    value: &DataValue,
    scalar_type: ScalarType,
    endianness: &Endianness,
    strict: bool,
) -> Result<Vec<u8>, LayoutError> {
    macro_rules! to_bytes {
        ($t:ty) => {{
            let val: $t = if strict {
                <$t as TryFromStrict<&DataValue>>::try_from_strict(value)?
            } else {
                <$t as TryFrom<&DataValue>>::try_from(value)?
            };
            Ok(val.to_endian_bytes(endianness))
        }};
    }

    match scalar_type {
        ScalarType::U8 => to_bytes!(u8),
        ScalarType::I8 => to_bytes!(i8),
        ScalarType::U16 => to_bytes!(u16),
        ScalarType::I16 => to_bytes!(i16),
        ScalarType::U32 => to_bytes!(u32),
        ScalarType::I32 => to_bytes!(i32),
        ScalarType::U64 => to_bytes!(u64),
        ScalarType::I64 => to_bytes!(i64),
        ScalarType::F32 => to_bytes!(f32),
        ScalarType::F64 => to_bytes!(f64),
        ScalarType::Fixed(fixed) => encode_fixed_point_bytes(value, fixed, endianness, strict),
    }
}
