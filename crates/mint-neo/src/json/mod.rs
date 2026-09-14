use std::collections::HashSet;

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde_json::value::RawValue;

use crate::CompiledSchema;
use crate::abi::{Endianness, Scalar, ScalarValue, write_scalar_bytes};
use crate::diagnostic::Error;
use crate::layout::{ArrayLayout, LayoutKind, ResolvedLayout};
use crate::source::{Source, Span};
use crate::types::TypeId;

pub fn encode(schema: &CompiledSchema, json: &Source) -> Result<Vec<u8>, Error> {
    // Serde validates the document and lends source slices directly to the
    // schema binder. No independent JSON tree or floating-point number model.
    let value: &RawValue = read_json(json, &json.text)?;
    let mut bytes = vec![schema.layout.padding; schema.layout.root_layout().size];
    bind(
        &schema.layout,
        schema.layout.root,
        0,
        value,
        json,
        "",
        &mut bytes,
    )?;
    for field in schema
        .layout
        .root_fields()
        .iter()
        .filter(|field| field.fingerprint)
    {
        write_at(
            &mut bytes,
            field.offset,
            Scalar::U64,
            schema.layout.abi.endianness(),
            ScalarValue::U(schema.fingerprint),
        );
    }
    Ok(bytes)
}

fn bind(
    layout: &ResolvedLayout,
    type_id: TypeId,
    offset: usize,
    value: &RawValue,
    source: &Source,
    pointer: &str,
    bytes: &mut [u8],
) -> Result<(), Error> {
    let span = borrowed_span(source, value.get());
    if pointer.bytes().filter(|&b| b == b'/').count() > 256 {
        return Err(Error::data(
            source,
            span,
            pointer,
            "JSON nesting exceeds 256 levels",
        ));
    }
    match &layout.layouts[type_id.0].kind {
        LayoutKind::Scalar(scalar) => {
            let message = match value.get().as_bytes()[0] {
                b'n' => Some("null is invalid for every field"),
                b't' | b'f' => Some("JSON booleans are invalid for every field"),
                b'"' => Some("JSON strings are invalid for every field"),
                b'{' | b'[' => Some("expected a JSON number"),
                _ => None,
            };
            if let Some(message) = message {
                return Err(Error::data(source, span, pointer, message));
            }
            let encoded = convert_number(*scalar, source, span, pointer)?;
            write_at(bytes, offset, *scalar, layout.abi.endianness(), encoded);
        }
        LayoutKind::Record(fields) => {
            if !value.get().starts_with('{') {
                return Err(Error::data(source, span, pointer, "expected a JSON object"));
            }
            let RawObject(entries) = read_json(source, value.get())?;
            let mut seen = HashSet::new();
            for (key, value) in entries {
                let key_span = borrowed_span(source, key.get());
                let key: String = read_json(source, key.get())?;
                let child = join_pointer(pointer, &key);
                if !seen.insert(key.clone()) {
                    return Err(Error::data(
                        source,
                        key_span,
                        &child,
                        format!("duplicate object property '{key}'"),
                    ));
                }
                let Some(field) = fields.iter().find(|field| field.name == key) else {
                    return Err(Error::data(
                        source,
                        key_span,
                        &child,
                        format!("unexpected property '{key}'"),
                    ));
                };
                if field.fingerprint {
                    return Err(Error::data(
                        source,
                        key_span,
                        &child,
                        "fingerprint fields must be absent from JSON",
                    ));
                }
                bind(
                    layout,
                    field.type_id,
                    offset + field.offset,
                    value,
                    source,
                    &child,
                    bytes,
                )?;
            }
            for field in fields {
                if !field.fingerprint && !seen.contains(&field.name) {
                    return Err(Error::data(
                        source,
                        span,
                        &join_pointer(pointer, &field.name),
                        format!("missing required field '{}'", field.name),
                    ));
                }
            }
        }
        LayoutKind::Array(array) => {
            bind_array(layout, array, offset, value, source, pointer, bytes, 0)?
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn bind_array(
    layout: &ResolvedLayout,
    array: &ArrayLayout,
    offset: usize,
    value: &RawValue,
    source: &Source,
    pointer: &str,
    bytes: &mut [u8],
    dim: usize,
) -> Result<(), Error> {
    let span = borrowed_span(source, value.get());
    if !value.get().starts_with('[') {
        return Err(Error::data(source, span, pointer, "expected a JSON array"));
    }
    let items: Vec<&RawValue> = read_json(source, value.get())?;
    let expected = array.dimensions[dim] as usize;
    if items.len() != expected {
        return Err(Error::data(
            source,
            span,
            pointer,
            format!("expected array length {expected}, found {}", items.len()),
        ));
    }
    let stride = dim_stride(array, dim);
    for (index, item) in items.into_iter().enumerate() {
        let child = format!("{pointer}/{index}");
        let at = offset + index * stride;
        if dim + 1 == array.dimensions.len() {
            bind(layout, array.element, at, item, source, &child, bytes)?;
        } else {
            bind_array(layout, array, at, item, source, &child, bytes, dim + 1)?;
        }
    }
    Ok(())
}

fn dim_stride(array: &ArrayLayout, dim: usize) -> usize {
    let tail: u64 = array.dimensions[dim + 1..].iter().copied().product();
    array.stride * usize::try_from(tail).unwrap_or(0)
}

fn write_at(
    bytes: &mut [u8],
    offset: usize,
    scalar: Scalar,
    endianness: Endianness,
    value: ScalarValue,
) {
    let size = scalar.size_bytes();
    write_scalar_bytes(scalar, endianness, &mut bytes[offset..offset + size], value);
}

fn convert_number(
    scalar: Scalar,
    source: &Source,
    span: Span,
    pointer: &str,
) -> Result<ScalarValue, Error> {
    let raw = source.slice(span);
    let Some((min, max)) = scalar.integer_range() else {
        return convert_float(scalar, raw, source, span, pointer);
    };
    let integer =
        parse_exact_integer(raw).map_err(|message| Error::data(source, span, pointer, message))?;
    if integer < min || integer > max {
        return Err(Error::data(
            source,
            span,
            pointer,
            format!("integer '{raw}' is out of range"),
        ));
    }
    Ok(if scalar.is_signed() {
        ScalarValue::I(integer as i64)
    } else {
        ScalarValue::U(integer as u64)
    })
}

fn convert_float(
    scalar: Scalar,
    raw: &str,
    source: &Source,
    span: Span,
    pointer: &str,
) -> Result<ScalarValue, Error> {
    let invalid = || {
        Error::data(
            source,
            span,
            pointer,
            format!("invalid floating-point token '{raw}'"),
        )
    };
    let (value, width) = match scalar {
        Scalar::F32 => (f64::from(raw.parse::<f32>().map_err(|_| invalid())?), "32"),
        Scalar::F64 => (raw.parse::<f64>().map_err(|_| invalid())?, "64"),
        _ => unreachable!(),
    };
    if value.is_finite() {
        Ok(ScalarValue::F(value))
    } else {
        Err(Error::data(
            source,
            span,
            pointer,
            format!("floating-point value '{raw}' overflows binary{width}"),
        ))
    }
}

/// i128 has at most 39 decimal digits. Any integer with more digits, or a
/// non-zero significand scaled by a larger non-negative power of ten, is
/// outside the supported range. This bound is applied *before* scaling so a
/// huge exponent never drives allocation or a long multiply loop.
const MAX_EXACT_INTEGER_DIGITS: usize = 39;

fn parse_exact_integer(raw: &str) -> Result<i128, String> {
    let raw = raw.trim();
    let (negative, body) = match raw.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, raw),
    };
    if body.is_empty() {
        return Err(format!("invalid number '{raw}'"));
    }
    let (mantissa, exponent) = split_exponent(body)?;
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if int.is_empty()
        || !int.bytes().all(|byte| byte.is_ascii_digit())
        || !frac.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(format!("invalid number '{raw}'"));
    }
    let combined;
    let digits = if frac.is_empty() {
        int
    } else {
        combined = format!("{int}{frac}");
        combined.as_str()
    };
    let significant = digits.trim_start_matches('0');
    if significant.is_empty() {
        return Ok(0);
    }
    let shift = i128::try_from(frac.len())
        .ok()
        .and_then(|frac_len| exponent.checked_sub(frac_len))
        .ok_or_else(|| format!("number '{raw}' is not an integer"))?;
    let value = if shift >= 0 {
        let Some(_) = usize::try_from(shift)
            .ok()
            .and_then(|zeros| significant.len().checked_add(zeros))
            .filter(|&digits| digits <= MAX_EXACT_INTEGER_DIGITS)
        else {
            return Err(format!("integer '{raw}' is out of supported range"));
        };
        let value = significant
            .parse::<i128>()
            .map_err(|_| format!("integer '{raw}' is out of supported range"))?;
        scale_pow10(value, shift, raw)?
    } else {
        let drop = shift
            .checked_neg()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| format!("number '{raw}' is not an integer"))?;
        if drop > significant.len()
            || significant.as_bytes()[significant.len() - drop..]
                .iter()
                .any(|&byte| byte != b'0')
        {
            return Err(format!("number '{raw}' is not an integer"));
        }
        let keep = &significant[..significant.len() - drop];
        if keep.is_empty() {
            return Ok(0);
        }
        if keep.len() > MAX_EXACT_INTEGER_DIGITS {
            return Err(format!("integer '{raw}' is out of supported range"));
        }
        keep.parse::<i128>()
            .map_err(|_| format!("integer '{raw}' is out of supported range"))?
    };
    if negative {
        value
            .checked_neg()
            .ok_or_else(|| format!("integer '{raw}' is out of supported range"))
    } else {
        Ok(value)
    }
}

fn scale_pow10(value: i128, shift: i128, raw: &str) -> Result<i128, String> {
    if shift == 0 {
        return Ok(value);
    }
    let exp = u32::try_from(shift).unwrap_or(u32::MAX);
    10i128
        .checked_pow(exp)
        .and_then(|scale| value.checked_mul(scale))
        .ok_or_else(|| format!("integer '{raw}' is out of supported range"))
}

fn split_exponent(body: &str) -> Result<(&str, i128), String> {
    match body.find(['e', 'E']) {
        Some(0) => Err(format!("invalid exponent in '{body}'")),
        Some(index) => Ok((&body[..index], parse_exponent(&body[index + 1..], body)?)),
        None => Ok((body, 0)),
    }
}

fn parse_exponent(text: &str, body: &str) -> Result<i128, String> {
    let invalid = || format!("invalid exponent in '{body}'");
    let text = text.strip_prefix('+').unwrap_or(text);
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid());
    }
    match digits.parse::<i128>() {
        Ok(value) if negative => Ok(value.checked_neg().unwrap_or(i128::MIN)),
        Ok(value) => Ok(value),
        Err(_) => Ok(if negative { i128::MIN } else { i128::MAX }),
    }
}

fn join_pointer(pointer: &str, key: &str) -> String {
    let escaped = key.replace('~', "~0").replace('/', "~1");
    format!("{pointer}/{escaped}")
}

fn borrowed_span(source: &Source, text: &str) -> Span {
    let start = text.as_ptr() as usize - source.text.as_ptr() as usize;
    Span::new(start, start + text.len())
}

fn read_json<'de, T: Deserialize<'de>>(source: &Source, text: &'de str) -> Result<T, Error> {
    serde_json::from_str(text).map_err(|error| {
        let line_start: usize = text
            .split_inclusive('\n')
            .take(error.line().saturating_sub(1))
            .map(str::len)
            .sum();
        let offset =
            borrowed_span(source, text).start + line_start + error.column().saturating_sub(1);
        Error::data(source, Span::point(offset), "", error.to_string())
    })
}

struct RawObject<'a>(Vec<(&'a RawValue, &'a RawValue)>);

impl<'de> Deserialize<'de> for RawObject<'de> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor;
        impl<'de> Visitor<'de> for ObjectVisitor {
            type Value = RawObject<'de>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(RawObject(entries))
            }
        }
        deserializer.deserialize_map(ObjectVisitor)
    }
}
