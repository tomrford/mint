use std::collections::HashSet;

use crate::abi::{Endianness, Scalar, ScalarValue, write_scalar_bytes};
use crate::diagnostic::{Category, Diagnostic, Error};
use crate::layout::ResolvedLayout;
use crate::schema::CompiledSchema;
use crate::source::{Source, Span};
use crate::types::{TypeId, TypeKind};

#[derive(Clone, Debug)]
enum Json {
    Null {
        span: Span,
    },
    Bool {
        #[allow(dead_code)]
        value: bool,
        span: Span,
    },
    Number {
        raw: String,
        span: Span,
    },
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
        match self {
            Self::Null { span }
            | Self::Bool { span, .. }
            | Self::Number { span, .. }
            | Self::String { span, .. }
            | Self::Array { span, .. }
            | Self::Object { span, .. } => *span,
        }
    }
}

pub fn encode(schema: &CompiledSchema, json: &Source) -> Result<Vec<u8>, Error> {
    let value = match parse_json(json) {
        Ok(value) => value,
        Err(error) => return Err(error.with_source(json.clone())),
    };
    let mut bytes = vec![schema.layout.padding; schema.layout.root_layout().size];
    bind(
        &schema.layout,
        schema.layout.root,
        0,
        &value,
        json,
        "",
        &mut bytes,
    )
    .map_err(|error| error.with_source(json.clone()))?;
    if let Some(name) = &schema.layout.fingerprint_field {
        let field = schema
            .layout
            .root_layout()
            .fields
            .iter()
            .find(|field| field.name == *name)
            .ok_or_else(|| {
                Error::one(Diagnostic::new(
                    Category::Schema,
                    &schema.source.name,
                    "fingerprint field disappeared after resolution",
                ))
            })?;
        let value = ScalarValue::U(schema.fingerprint);
        write_at(
            &mut bytes,
            field.offset,
            Scalar::U64,
            schema.layout.abi.endianness(),
            value,
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
            let number = match value {
                Json::Number { raw, span } => (raw, *span),
                Json::Null { span } => {
                    return Err(data(
                        source,
                        *span,
                        pointer,
                        "null is invalid for every field",
                    ));
                }
                Json::Bool { span, .. } => {
                    return Err(data(
                        source,
                        *span,
                        pointer,
                        "JSON booleans are invalid for every field",
                    ));
                }
                Json::String { span, .. } => {
                    return Err(data(
                        source,
                        *span,
                        pointer,
                        "JSON strings are invalid for every field",
                    ));
                }
                Json::Array { span, .. } | Json::Object { span, .. } => {
                    return Err(data(source, *span, pointer, "expected a JSON number"));
                }
            };
            let encoded = convert_number(*scalar, number.0, source, number.1, pointer)?;
            write_at(bytes, offset, *scalar, layout.abi.endianness(), encoded);
            Ok(())
        }
        TypeKind::Record { .. } => {
            let Json::Object { entries, span } = value else {
                return Err(data(
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
                    return Err(data(
                        source,
                        entry.key_span,
                        pointer,
                        format!("unexpected property '{}'", entry.key),
                    ));
                };
                if field.fingerprint {
                    return Err(data(
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
                return Err(data(
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
        TypeKind::Enum => Err(data(
            source,
            value.span(),
            pointer,
            "enum-typed members are not supported",
        )),
    }
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
        data(
            source,
            value.span(),
            pointer,
            "internal: missing array layout",
        )
    })?;
    let Json::Array { items, span } = value else {
        return Err(data(source, value.span(), pointer, "expected a JSON array"));
    };
    let expected = usize::try_from(array.dimensions[dim]).unwrap_or(0);
    if items.len() != expected {
        return Err(data(
            source,
            *span,
            pointer,
            format!("expected array length {expected}, found {}", items.len()),
        ));
    }
    let next_stride = if dim + 1 == array.dimensions.len() {
        array.stride
    } else {
        let tail: u64 = array.dimensions[dim + 1..].iter().copied().product();
        array.stride * usize::try_from(tail).unwrap_or(0)
    };
    for (index, item) in items.iter().enumerate() {
        let child_pointer = format!("{pointer}/{index}");
        let child_offset = offset + index * next_stride;
        if dim + 1 == array.dimensions.len() {
            bind(
                layout,
                array.element,
                child_offset,
                item,
                source,
                &child_pointer,
                bytes,
            )?;
        } else {
            bind_array(
                layout,
                type_id,
                child_offset,
                item,
                source,
                &child_pointer,
                bytes,
                dim + 1,
            )?;
        }
    }
    Ok(())
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
    raw: &str,
    source: &Source,
    span: Span,
    pointer: &str,
) -> Result<ScalarValue, Error> {
    if scalar.is_float() {
        return convert_float(scalar, raw, source, span, pointer);
    }
    let integer =
        parse_exact_integer(raw).map_err(|message| data(source, span, pointer, message))?;
    match scalar {
        Scalar::U8 => in_range(integer, 0, i128::from(u8::MAX), raw, source, span, pointer)
            .map(|value| ScalarValue::U(value as u64)),
        Scalar::U16 => in_range(integer, 0, i128::from(u16::MAX), raw, source, span, pointer)
            .map(|value| ScalarValue::U(value as u64)),
        Scalar::U32 => in_range(integer, 0, i128::from(u32::MAX), raw, source, span, pointer)
            .map(|value| ScalarValue::U(value as u64)),
        Scalar::U64 => in_range(integer, 0, i128::from(u64::MAX), raw, source, span, pointer)
            .map(|value| ScalarValue::U(value as u64)),
        Scalar::I8 => in_range(
            integer,
            i128::from(i8::MIN),
            i128::from(i8::MAX),
            raw,
            source,
            span,
            pointer,
        )
        .map(|value| ScalarValue::I(value as i64)),
        Scalar::I16 => in_range(
            integer,
            i128::from(i16::MIN),
            i128::from(i16::MAX),
            raw,
            source,
            span,
            pointer,
        )
        .map(|value| ScalarValue::I(value as i64)),
        Scalar::I32 => in_range(
            integer,
            i128::from(i32::MIN),
            i128::from(i32::MAX),
            raw,
            source,
            span,
            pointer,
        )
        .map(|value| ScalarValue::I(value as i64)),
        Scalar::I64 => in_range(
            integer,
            i128::from(i64::MIN),
            i128::from(i64::MAX),
            raw,
            source,
            span,
            pointer,
        )
        .map(|value| ScalarValue::I(value as i64)),
        Scalar::F32 | Scalar::F64 => unreachable!(),
    }
}

fn convert_float(
    scalar: Scalar,
    raw: &str,
    source: &Source,
    span: Span,
    pointer: &str,
) -> Result<ScalarValue, Error> {
    match scalar {
        Scalar::F32 => {
            let value = raw.parse::<f32>().map_err(|_| {
                data(
                    source,
                    span,
                    pointer,
                    format!("invalid floating-point token '{raw}'"),
                )
            })?;
            if !value.is_finite() {
                return Err(data(
                    source,
                    span,
                    pointer,
                    format!("floating-point value '{raw}' overflows binary32"),
                ));
            }
            Ok(ScalarValue::F(f64::from(value)))
        }
        Scalar::F64 => {
            let value = raw.parse::<f64>().map_err(|_| {
                data(
                    source,
                    span,
                    pointer,
                    format!("invalid floating-point token '{raw}'"),
                )
            })?;
            if !value.is_finite() {
                return Err(data(
                    source,
                    span,
                    pointer,
                    format!("floating-point value '{raw}' overflows binary64"),
                ));
            }
            Ok(ScalarValue::F(value))
        }
        _ => unreachable!(),
    }
}

fn in_range(
    value: i128,
    min: i128,
    max: i128,
    raw: &str,
    source: &Source,
    span: Span,
    pointer: &str,
) -> Result<i128, Error> {
    if value < min || value > max {
        return Err(data(
            source,
            span,
            pointer,
            format!("integer '{raw}' is out of range"),
        ));
    }
    Ok(value)
}

fn parse_exact_integer(raw: &str) -> Result<i128, String> {
    let raw = raw.trim();
    let (negative, body) = if let Some(rest) = raw.strip_prefix('-') {
        (true, rest)
    } else {
        (false, raw)
    };
    if body.is_empty() {
        return Err(format!("invalid number '{raw}'"));
    }
    let (mantissa, exponent) = split_exponent(body)?;
    let (int, frac) = match mantissa.split_once('.') {
        Some((int, frac)) => (int, frac),
        None => (mantissa, ""),
    };
    if int.is_empty()
        || !int.chars().all(|character| character.is_ascii_digit())
        || !frac.chars().all(|character| character.is_ascii_digit())
    {
        return Err(format!("invalid number '{raw}'"));
    }
    let digits = format!("{int}{frac}");
    let mut exp = exponent - isize::try_from(frac.len()).unwrap_or(0);
    let mut digits = digits.trim_start_matches('0').to_owned();
    if digits.is_empty() {
        digits.push('0');
    }
    while exp > 0 {
        digits.push('0');
        exp -= 1;
    }
    if exp < 0 {
        let drop = usize::try_from(-exp).unwrap_or(0);
        if drop > digits.len() {
            return Err(format!("number '{raw}' is not an integer"));
        }
        let (keep, rest) = digits.split_at(digits.len() - drop);
        if rest.chars().any(|character| character != '0') {
            return Err(format!("number '{raw}' is not an integer"));
        }
        digits = keep.to_owned();
        if digits.is_empty() {
            digits.push('0');
        }
    }
    let value = digits
        .parse::<i128>()
        .map_err(|_| format!("integer '{raw}' is out of supported range"))?;
    if negative { Ok(-value) } else { Ok(value) }
}

fn split_exponent(body: &str) -> Result<(&str, isize), String> {
    if let Some(index) = body.find(['e', 'E']) {
        let (mantissa, exp) = body.split_at(index);
        let exp = &exp[1..];
        let exp = exp
            .parse::<isize>()
            .map_err(|_| format!("invalid exponent in '{body}'"))?;
        Ok((mantissa, exp))
    } else {
        Ok((body, 0))
    }
}

fn join_pointer(pointer: &str, key: &str) -> String {
    let escaped = key.replace('~', "~0").replace('/', "~1");
    format!("{pointer}/{escaped}")
}

fn data(source: &Source, span: Span, pointer: &str, message: impl Into<String>) -> Error {
    Error::one(
        Diagnostic::new(Category::Data, &source.name, message)
            .at(span)
            .pointer(pointer),
    )
}

fn parse_json(source: &Source) -> Result<Json, Error> {
    let mut parser = JsonParser { source, index: 0 };
    let value = parser.value()?;
    parser.skip_ws();
    if parser.index != source.len() {
        return Err(data(
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
            Some(b'n') => self.keyword(b"null", |span| Json::Null { span }),
            Some(b't') => self.keyword(b"true", |span| Json::Bool { value: true, span }),
            Some(b'f') => self.keyword(b"false", |span| Json::Bool { value: false, span }),
            Some(b'"') => self.string(),
            Some(b'[') => self.array(),
            Some(b'{') => self.object(),
            Some(b'-') | Some(b'0'..=b'9') => self.number(),
            _ => Err(data(
                self.source,
                Span::point(self.index),
                "",
                "expected a JSON value",
            )),
        }
    }

    fn object(&mut self) -> Result<Json, Error> {
        let start = self.index;
        self.bump();
        self.skip_ws();
        let mut entries = Vec::new();
        let mut keys = HashSet::new();
        if self.peek() == Some(b'}') {
            self.bump();
            return Ok(Json::Object {
                entries,
                span: Span::new(start, self.index),
            });
        }
        loop {
            self.skip_ws();
            let key = match self.string()? {
                Json::String { value, span } => (value, span),
                _ => unreachable!(),
            };
            if !keys.insert(key.0.clone()) {
                return Err(data(
                    self.source,
                    key.1,
                    "",
                    format!("duplicate object property '{}'", key.0),
                ));
            }
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(data(
                    self.source,
                    Span::point(self.index),
                    "",
                    "expected ':'",
                ));
            }
            self.bump();
            let value = self.value()?;
            entries.push(ObjectEntry {
                key: key.0,
                key_span: key.1,
                value,
            });
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.bump();
                    continue;
                }
                Some(b'}') => {
                    self.bump();
                    break;
                }
                _ => {
                    return Err(data(
                        self.source,
                        Span::point(self.index),
                        "",
                        "expected ',' or '}'",
                    ));
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
        if self.peek() == Some(b']') {
            self.bump();
            return Ok(Json::Array {
                items,
                span: Span::new(start, self.index),
            });
        }
        loop {
            items.push(self.value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.bump();
                    continue;
                }
                Some(b']') => {
                    self.bump();
                    break;
                }
                _ => {
                    return Err(data(
                        self.source,
                        Span::point(self.index),
                        "",
                        "expected ',' or ']'",
                    ));
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
        if self.peek() != Some(b'"') {
            return Err(data(
                self.source,
                Span::point(self.index),
                "",
                "expected a string",
            ));
        }
        self.bump();
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
                            let mut hex = String::new();
                            for _ in 0..4 {
                                let Some(digit) = self.peek() else {
                                    return Err(data(
                                        self.source,
                                        Span::point(self.index),
                                        "",
                                        "invalid unicode escape",
                                    ));
                                };
                                hex.push(digit as char);
                                self.bump();
                            }
                            let code = u32::from_str_radix(&hex, 16).map_err(|_| {
                                data(
                                    self.source,
                                    Span::point(self.index),
                                    "",
                                    "invalid unicode escape",
                                )
                            })?;
                            value.push(char::from_u32(code).ok_or_else(|| {
                                data(
                                    self.source,
                                    Span::point(self.index),
                                    "",
                                    "invalid unicode escape",
                                )
                            })?);
                            continue;
                        }
                        _ => {
                            return Err(data(
                                self.source,
                                Span::point(self.index),
                                "",
                                "invalid escape",
                            ));
                        }
                    }
                    self.bump();
                }
                b if b < 0x20 => {
                    return Err(data(
                        self.source,
                        Span::point(self.index),
                        "",
                        "unescaped control character",
                    ));
                }
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
        Err(data(
            self.source,
            Span::point(self.index),
            "",
            "unterminated string",
        ))
    }

    fn number(&mut self) -> Result<Json, Error> {
        let start = self.index;
        if self.peek() == Some(b'-') {
            self.bump();
        }
        match self.peek() {
            Some(b'0') => self.bump(),
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.bump();
                }
            }
            _ => {
                return Err(data(
                    self.source,
                    Span::point(self.index),
                    "",
                    "invalid number",
                ));
            }
        }
        if self.peek() == Some(b'.') {
            self.bump();
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(data(
                    self.source,
                    Span::point(self.index),
                    "",
                    "invalid number",
                ));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.bump();
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.bump();
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.bump();
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(data(
                    self.source,
                    Span::point(self.index),
                    "",
                    "invalid number",
                ));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.bump();
            }
        }
        Ok(Json::Number {
            raw: self.source.text[start..self.index].to_owned(),
            span: Span::new(start, self.index),
        })
    }

    fn keyword(&mut self, token: &[u8], build: impl FnOnce(Span) -> Json) -> Result<Json, Error> {
        let start = self.index;
        for expected in token {
            if self.peek() != Some(*expected) {
                return Err(data(
                    self.source,
                    Span::point(self.index),
                    "",
                    "invalid JSON keyword",
                ));
            }
            self.bump();
        }
        Ok(build(Span::new(start, self.index)))
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
