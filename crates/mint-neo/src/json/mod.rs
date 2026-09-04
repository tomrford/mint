use std::collections::HashSet;

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde_json::value::RawValue;

use crate::CompiledSchema;
use crate::abi::{Endianness, Scalar, ScalarValue, write_scalar_bytes};
use crate::diagnostic::Error;
use crate::layout::{ArrayLayout, ResolvedLayout};
use crate::source::{Source, Span};
use crate::types::{TypeId, TypeKind};

#[derive(Clone, Debug)]
enum Json {
    Null(Span),
    Bool(Span),
    Number(Span),
    String(Span),
    Array {
        items: Vec<Json>,
        span: Span,
    },
    Object {
        entries: Vec<ObjectEntry>,
        span: Span,
    },
}

#[derive(Clone, Debug)]
struct ObjectEntry {
    key: String,
    key_span: Span,
    value: Json,
}

impl Json {
    fn span(&self) -> Span {
        match *self {
            Self::Null(span) | Self::Bool(span) | Self::Number(span) | Self::String(span) => span,
            Self::Array { span, .. } | Self::Object { span, .. } => span,
        }
    }
}

pub fn encode(schema: &CompiledSchema, json: &Source) -> Result<Vec<u8>, Error> {
    let value = parse_json(json)?;
    let mut bytes = vec![schema.layout.padding; schema.layout.root_layout().size];
    bind(
        &schema.layout,
        schema.layout.root,
        0,
        &value,
        json,
        "",
        &mut bytes,
    )?;
    if let Some(field) = schema
        .layout
        .root_layout()
        .fields
        .iter()
        .find(|field| field.fingerprint)
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
    value: &Json,
    source: &Source,
    pointer: &str,
    bytes: &mut [u8],
) -> Result<(), Error> {
    match &layout.types[type_id.0] {
        TypeKind::Scalar { scalar } => {
            let Json::Number(span) = *value else {
                return Err(scalar_mismatch(value, source, pointer));
            };
            let encoded = convert_number(*scalar, source, span, pointer)?;
            write_at(bytes, offset, *scalar, layout.abi.endianness(), encoded);
            Ok(())
        }
        TypeKind::Record { .. } => {
            let Json::Object { entries, span } = value else {
                return Err(Error::data(
                    source,
                    value.span(),
                    pointer,
                    "expected a JSON object",
                ));
            };
            let fields = &layout.layouts[type_id.0].fields;
            let mut seen = HashSet::new();
            for entry in entries {
                let Some(field) = fields.iter().find(|field| field.name == entry.key) else {
                    return Err(Error::data(
                        source,
                        entry.key_span,
                        &join_pointer(pointer, &entry.key),
                        format!("unexpected property '{}'", entry.key),
                    ));
                };
                if field.fingerprint {
                    return Err(Error::data(
                        source,
                        entry.key_span,
                        &join_pointer(pointer, &entry.key),
                        "fingerprint fields must be absent from JSON",
                    ));
                }
                seen.insert(entry.key.clone());
                bind(
                    layout,
                    field.type_id,
                    offset + field.offset,
                    &entry.value,
                    source,
                    &join_pointer(pointer, &entry.key),
                    bytes,
                )?;
            }
            for field in fields {
                if field.fingerprint || seen.contains(&field.name) {
                    continue;
                }
                return Err(Error::data(
                    source,
                    *span,
                    &join_pointer(pointer, &field.name),
                    format!("missing required field '{}'", field.name),
                ));
            }
            Ok(())
        }
        TypeKind::Array { .. } => {
            bind_array(layout, type_id, offset, value, source, pointer, bytes, 0)
        }
    }
}

fn scalar_mismatch(value: &Json, source: &Source, pointer: &str) -> Error {
    let message = match value {
        Json::Null(_) => "null is invalid for every field",
        Json::Bool(_) => "JSON booleans are invalid for every field",
        Json::String(_) => "JSON strings are invalid for every field",
        _ => "expected a JSON number",
    };
    Error::data(source, value.span(), pointer, message)
}

#[allow(clippy::too_many_arguments)]
fn bind_array(
    layout: &ResolvedLayout,
    type_id: TypeId,
    offset: usize,
    value: &Json,
    source: &Source,
    pointer: &str,
    bytes: &mut [u8],
    dim: usize,
) -> Result<(), Error> {
    let array = layout.layouts[type_id.0].array.as_ref().ok_or_else(|| {
        Error::data(
            source,
            value.span(),
            pointer,
            "internal: missing array layout",
        )
    })?;
    let Json::Array { items, span } = value else {
        return Err(Error::data(
            source,
            value.span(),
            pointer,
            "expected a JSON array",
        ));
    };
    let expected = usize::try_from(array.dimensions[dim]).unwrap_or(0);
    if items.len() != expected {
        return Err(Error::data(
            source,
            *span,
            pointer,
            format!("expected array length {expected}, found {}", items.len()),
        ));
    }
    let stride = dim_stride(array, dim);
    let last = dim + 1 == array.dimensions.len();
    for (index, item) in items.iter().enumerate() {
        let child = format!("{pointer}/{index}");
        let at = offset + index * stride;
        if last {
            bind(layout, array.element, at, item, source, &child, bytes)?;
        } else {
            bind_array(layout, type_id, at, item, source, &child, bytes, dim + 1)?;
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

// RawValue keeps the original numeric token and borrows exact source spans.
// Serde owns JSON syntax, escapes and Unicode; this adapter only preserves
// duplicate keys and limits the tree used by structural binding.
const MAX_JSON_DEPTH: usize = 256;

fn parse_json(source: &Source) -> Result<Json, Error> {
    let raw: &RawValue = read_json(source, &source.text)?;
    parse_value(source, raw, "", 0)
}

fn parse_value(
    source: &Source,
    raw: &RawValue,
    pointer: &str,
    depth: usize,
) -> Result<Json, Error> {
    let span = borrowed_span(source, raw.get());
    if depth > MAX_JSON_DEPTH {
        return Err(Error::data(
            source,
            span,
            pointer,
            "JSON nesting exceeds 256 levels",
        ));
    }
    Ok(match raw.get().as_bytes()[0] {
        b'{' => {
            let RawObject(pairs) = read_json(source, raw.get())?;
            let mut keys = HashSet::new();
            let mut entries = Vec::with_capacity(pairs.len());
            for (key, value) in pairs {
                let key_span = borrowed_span(source, key.get());
                let key: String = read_json(source, key.get())?;
                let child = join_pointer(pointer, &key);
                if !keys.insert(key.clone()) {
                    return Err(Error::data(
                        source,
                        key_span,
                        &child,
                        format!("duplicate object property '{key}'"),
                    ));
                }
                entries.push(ObjectEntry {
                    key,
                    key_span,
                    value: parse_value(source, value, &child, depth + 1)?,
                });
            }
            Json::Object { entries, span }
        }
        b'[' => {
            let values: Vec<&RawValue> = read_json(source, raw.get())?;
            let items = values
                .into_iter()
                .enumerate()
                .map(|(index, value)| {
                    parse_value(
                        source,
                        value,
                        &join_pointer(pointer, &index.to_string()),
                        depth + 1,
                    )
                })
                .collect::<Result<_, _>>()?;
            Json::Array { items, span }
        }
        b'n' => Json::Null(span),
        b't' | b'f' => Json::Bool(span),
        b'"' => {
            let _: String = read_json(source, raw.get())?;
            Json::String(span)
        }
        _ => Json::Number(span),
    })
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
