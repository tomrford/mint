/// Parse a C integer token with optional unsigned or long suffixes.
/// Accepted bases are decimal, hexadecimal and octal. The represented value
/// is unchanged by suffixes.
pub fn parse_c_unsigned(text: &str) -> Result<u128, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("empty integer token".to_owned());
    }
    let (body, _suffix) = split_integer_suffix(text)?;
    if body.is_empty() {
        return Err(format!("invalid integer '{text}'"));
    }
    if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        if hex.is_empty() || !hex.chars().all(|character| character.is_ascii_hexdigit()) {
            return Err(format!("invalid hexadecimal integer '{text}'"));
        }
        return u128::from_str_radix(hex, 16)
            .map_err(|_| format!("hexadecimal integer '{text}' is out of range"));
    }
    if body != "0"
        && body.starts_with('0')
        && body.chars().all(|character| character.is_ascii_digit())
    {
        if !body
            .chars()
            .all(|character| ('0'..='7').contains(&character))
        {
            return Err(format!("invalid octal integer '{text}'"));
        }
        return u128::from_str_radix(body, 8)
            .map_err(|_| format!("octal integer '{text}' is out of range"));
    }
    if !body.chars().all(|character| character.is_ascii_digit()) {
        return Err(format!("invalid integer '{text}'"));
    }
    body.parse::<u128>()
        .map_err(|_| format!("integer '{text}' is out of range"))
}

pub(crate) fn split_integer_suffix(text: &str) -> Result<(&str, &str), String> {
    let bytes = text.as_bytes();
    let mut index = bytes.len();
    while index > 0 {
        match bytes[index - 1] {
            b'u' | b'U' | b'l' | b'L' => index -= 1,
            _ => break,
        }
    }
    let suffix = &text[index..];
    if !valid_integer_suffix(suffix) {
        return Err(format!("unsupported integer suffix in '{text}'"));
    }
    Ok((&text[..index], suffix))
}

fn valid_integer_suffix(suffix: &str) -> bool {
    if suffix.contains("lL") || suffix.contains("Ll") {
        return false;
    }
    let lower = suffix.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "" | "u" | "l" | "ll" | "ul" | "lu" | "ull" | "llu"
    )
}

#[cfg(test)]
mod tests {
    use super::parse_c_unsigned;

    #[test]
    fn parses_decimal_hex_and_octal() {
        assert_eq!(parse_c_unsigned("10").unwrap(), 10);
        assert_eq!(parse_c_unsigned("0xFF").unwrap(), 255);
        assert_eq!(parse_c_unsigned("0x8000u").unwrap(), 0x8000);
        assert_eq!(parse_c_unsigned("010").unwrap(), 8);
        assert_eq!(parse_c_unsigned("0").unwrap(), 0);
        assert_eq!(parse_c_unsigned("4ull").unwrap(), 4);
    }

    #[test]
    fn rejects_invalid_tokens() {
        assert!(parse_c_unsigned("08").is_err());
        assert!(parse_c_unsigned("0b10").is_err());
        assert!(parse_c_unsigned("4.0").is_err());
        assert!(parse_c_unsigned("4f").is_err());
    }
}
