use std::collections::HashSet;

use crate::abi::{Endianness, Scalar, ScalarValue, write_scalar_bytes};
use crate::diagnostic::{Category, Error};
use crate::layout::{ArrayLayout, ResolvedLayout};
use crate::schema::CompiledSchema;
use crate::source::{Source, Span};
use crate::types::{TypeId, TypeKind};

#[derive(Clone, Debug)]
enum Json {
    Null(Span),
    Bool(Span),
    Number(Span),
    String {
        value: String,
        span: Span,
    },
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
            Self::Null(span) | Self::Bool(span) | Self::Number(span) => span,
            Self::String { span, .. } | Self::Array { span, .. } | Self::Object { span, .. } => {
                span
            }
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
    if let Some(name) = &schema.layout.fingerprint_field {
        let field = schema
            .layout
            .root_layout()
            .fields
            .iter()
            .find(|field| field.name == *name)
            .ok_or_else(|| {
                Error::named(
                    Category::Schema,
                    &schema.source.name,
                    "fingerprint field disappeared after resolution",
                )
                .with_source(schema.source.clone())
            })?;
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
        TypeKind::Scalar { scalar, .. } => {
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
        TypeKind::Enum => Err(Error::data(
            source,
            value.span(),
            pointer,
            "enum-typed members are not supported",
        )),
    }
}

fn scalar_mismatch(value: &Json, source: &Source, pointer: &str) -> Error {
    let message = match value {
        Json::Null(_) => "null is invalid for every field",
        Json::Bool(_) => "JSON booleans are invalid for every field",
        Json::String { .. } => "JSON strings are invalid for every field",
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

fn parse_json(source: &Source) -> Result<Json, Error> {
    let mut parser = JsonParser { source, index: 0 };
    let value = parser.value()?;
    parser.skip_ws();
    if parser.index != source.len() {
        return Err(Error::data(
            source,
            Span::point(parser.index),
            "",
            "unexpected trailing JSON text",
        ));
    }
    Ok(value)
}

struct JsonParser<'a> {
    source: &'a Source,
    index: usize,
}

impl JsonParser<'_> {
    fn value(&mut self) -> Result<Json, Error> {
        self.skip_ws();
        match self.peek() {
            Some(b'n') => self.keyword(b"null").map(Json::Null),
            Some(b't') => self.keyword(b"true").map(Json::Bool),
            Some(b'f') => self.keyword(b"false").map(Json::Bool),
            Some(b'"') => self.string(),
            Some(b'[') => self.array(),
            Some(b'{') => self.object(),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.error_at("expected a JSON value")),
        }
    }

    fn object(&mut self) -> Result<Json, Error> {
        let start = self.index;
        self.bump();
        self.skip_ws();
        let mut entries = Vec::new();
        let mut keys = HashSet::new();
        if !self.take(b'}') {
            loop {
                self.skip_ws();
                let Json::String {
                    value: key,
                    span: key_span,
                } = self.string()?
                else {
                    unreachable!()
                };
                if !keys.insert(key.clone()) {
                    return Err(Error::data(
                        self.source,
                        key_span,
                        "",
                        format!("duplicate object property '{key}'"),
                    ));
                }
                self.skip_ws();
                self.expect(b':', "expected ':'")?;
                let value = self.value()?;
                entries.push(ObjectEntry {
                    key,
                    key_span,
                    value,
                });
                if !self.comma_or_close(b'}', "expected ',' or '}'")? {
                    break;
                }
            }
        }
        Ok(Json::Object {
            entries,
            span: Span::new(start, self.index),
        })
    }

    fn array(&mut self) -> Result<Json, Error> {
        let start = self.index;
        self.bump();
        self.skip_ws();
        let mut items = Vec::new();
        if !self.take(b']') {
            loop {
                items.push(self.value()?);
                if !self.comma_or_close(b']', "expected ',' or ']'")? {
                    break;
                }
            }
        }
        Ok(Json::Array {
            items,
            span: Span::new(start, self.index),
        })
    }

    fn string(&mut self) -> Result<Json, Error> {
        let start = self.index;
        self.expect(b'"', "expected a string")?;
        let mut value = String::new();
        while let Some(byte) = self.peek() {
            match byte {
                b'"' => {
                    self.bump();
                    return Ok(Json::String {
                        value,
                        span: Span::new(start, self.index),
                    });
                }
                b'\\' => {
                    self.bump();
                    match self.peek() {
                        Some(b'"') => value.push('"'),
                        Some(b'\\') => value.push('\\'),
                        Some(b'/') => value.push('/'),
                        Some(b'b') => value.push('\u{0008}'),
                        Some(b'f') => value.push('\u{000c}'),
                        Some(b'n') => value.push('\n'),
                        Some(b'r') => value.push('\r'),
                        Some(b't') => value.push('\t'),
                        Some(b'u') => {
                            self.bump();
                            value.push(self.unicode_escape()?);
                            continue;
                        }
                        _ => return Err(self.error_at("invalid escape")),
                    }
                    self.bump();
                }
                b if b < 0x20 => return Err(self.error_at("unescaped control character")),
                _ => {
                    let ch = self.source.text[self.index..]
                        .chars()
                        .next()
                        .unwrap_or('\0');
                    value.push(ch);
                    self.index += ch.len_utf8();
                }
            }
        }
        Err(self.error_at("unterminated string"))
    }

    fn unicode_escape(&mut self) -> Result<char, Error> {
        let mut code = 0u32;
        let mut valid = true;
        for _ in 0..4 {
            let Some(digit) = self.peek() else {
                return Err(self.error_at("invalid unicode escape"));
            };
            self.bump();
            let nibble = match digit {
                b'0'..=b'9' => digit - b'0',
                b'a'..=b'f' => digit - b'a' + 10,
                b'A'..=b'F' => digit - b'A' + 10,
                _ => {
                    valid = false;
                    0
                }
            };
            code = (code << 4) | u32::from(nibble);
        }
        if !valid {
            return Err(self.error_at("invalid unicode escape"));
        }
        char::from_u32(code).ok_or_else(|| self.error_at("invalid unicode escape"))
    }

    fn number(&mut self) -> Result<Json, Error> {
        let start = self.index;
        let _ = self.take(b'-');
        match self.peek() {
            Some(b'0') => self.bump(),
            Some(b'1'..=b'9') => {
                self.consume_digits();
            }
            _ => return Err(self.error_at("invalid number")),
        }
        if self.take(b'.') && !self.consume_digits() {
            return Err(self.error_at("invalid number"));
        }
        if self.take(b'e') || self.take(b'E') {
            let _ = self.take(b'+') || self.take(b'-');
            if !self.consume_digits() {
                return Err(self.error_at("invalid number"));
            }
        }
        Ok(Json::Number(Span::new(start, self.index)))
    }

    fn keyword(&mut self, token: &[u8]) -> Result<Span, Error> {
        let start = self.index;
        for expected in token {
            if !self.take(*expected) {
                return Err(self.error_at("invalid JSON keyword"));
            }
        }
        Ok(Span::new(start, self.index))
    }

    fn comma_or_close(&mut self, close: u8, message: &'static str) -> Result<bool, Error> {
        self.skip_ws();
        if self.take(b',') {
            Ok(true)
        } else if self.take(close) {
            Ok(false)
        } else {
            Err(self.error_at(message))
        }
    }

    fn consume_digits(&mut self) -> bool {
        let start = self.index;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.bump();
        }
        self.index > start
    }

    fn expect(&mut self, byte: u8, message: &'static str) -> Result<(), Error> {
        if self.take(byte) {
            Ok(())
        } else {
            Err(self.error_at(message))
        }
    }

    fn take(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn error_at(&self, message: impl Into<String>) -> Error {
        Error::data(self.source, Span::point(self.index), "", message)
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.bump();
        }
    }

    fn peek(&self) -> Option<u8> {
        self.source.byte(self.index)
    }

    fn bump(&mut self) {
        self.index += 1;
    }
}
