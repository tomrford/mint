use mint_neo::{Source, compile_header, encode_json};

fn header(text: &str) -> Source {
    Source::new("config.h", text)
}

fn json(text: &str) -> Source {
    Source::new("config.json", text)
}

fn schema() -> mint_neo::CompiledSchema {
    compile_header(header(
        r#"
#include <stdint.h>

typedef struct {
    uint16_t x;
    uint16_t y;
} point_t;

/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct {
    uint64_t wide;
    int64_t delta;
    point_t origin;
    uint8_t items[2];
} config_t;
"#,
    ))
    .expect("header")
}

fn encode_wide(value: &str) -> Result<Vec<u8>, mint_neo::Error> {
    encode_json(
        &schema(),
        &json(&format!(
            r#"{{"wide": {value}, "delta": 0, "origin": {{"x": 0, "y": 0}}, "items": [0, 0]}}"#
        )),
    )
}

fn first_line(error: &mint_neo::Error) -> String {
    error.render(&[]).lines().next().unwrap_or("").to_owned()
}

fn pointer_of(error: &mint_neo::Error) -> Option<&str> {
    error.diagnostic.json_pointer.as_deref()
}

#[test]
fn exact_u64_above_f64_mantissa_is_preserved() {
    let bytes = encode_wide("9007199254740993").expect("2^53+1");
    assert_eq!(
        &bytes[..8],
        &[0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00]
    );
}

#[test]
fn ordinary_exact_integers_including_u64_boundaries() {
    let cases: &[(&str, [u8; 8])] = &[
        ("1.0", [1, 0, 0, 0, 0, 0, 0, 0]),
        ("2e0", [2, 0, 0, 0, 0, 0, 0, 0]),
        ("10e-1", [1, 0, 0, 0, 0, 0, 0, 0]),
        ("1.230e2", [123, 0, 0, 0, 0, 0, 0, 0]),
        ("0e1000000", [0; 8]),
        ("18446744073709551615", [0xff; 8]),
        (
            "10000000000000000000",
            10_000_000_000_000_000_000u64.to_le_bytes(),
        ),
    ];
    for (token, expected) in cases {
        let bytes = encode_wide(token).expect(token);
        assert_eq!(&bytes[..8], expected, "{token}");
    }

    let min = encode_json(
        &schema(),
        &json(
            r#"{"wide": 0, "delta": -9223372036854775808, "origin": {"x": 0, "y": 0}, "items": [0, 0]}"#,
        ),
    )
    .expect("i64::MIN");
    assert_eq!(
        &min[8..16],
        &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80]
    );
}

#[test]
fn extreme_exponents_reject_without_accepting_non_integers() {
    for token in [
        "1e1000000",
        "1e9223372036854775807",
        "1e+99999999999999999999",
        "1e-1000000",
        "1e-9223372036854775808",
        "1.5",
        "1e-1",
        "1.23e1",
        "18446744073709551616",
    ] {
        let error = encode_wide(token).expect_err(token);
        let message = error.to_string();
        assert!(
            message.contains("not an integer") || message.contains("out of"),
            "{token}: {message}"
        );
    }
}

#[test]
fn rejects_bool_string_and_wrong_array_length() {
    let schema = schema();
    let boolean = encode_json(
        &schema,
        &json(r#"{"wide": true, "delta": 0, "origin": {"x": 0, "y": 0}, "items": [0, 0]}"#),
    )
    .expect_err("bool");
    assert!(boolean.to_string().contains("boolean"), "{boolean}");
    assert_eq!(pointer_of(&boolean), Some("/wide"));

    let string = encode_json(
        &schema,
        &json(r#"{"wide": "1", "delta": 0, "origin": {"x": 0, "y": 0}, "items": [0, 0]}"#),
    )
    .expect_err("string");
    assert!(string.to_string().contains("string"), "{string}");
    assert_eq!(pointer_of(&string), Some("/wide"));

    let short = encode_json(
        &schema,
        &json(r#"{"wide": 0, "delta": 0, "origin": {"x": 0, "y": 0}, "items": [0]}"#),
    )
    .expect_err("short array");
    assert!(short.to_string().contains("array length"), "{short}");
    assert_eq!(pointer_of(&short), Some("/items"));

    let long = encode_json(
        &schema,
        &json(r#"{"wide": 0, "delta": 0, "origin": {"x": 0, "y": 0}, "items": [0, 0, 0]}"#),
    )
    .expect_err("long array");
    assert!(long.to_string().contains("array length"), "{long}");
    assert_eq!(pointer_of(&long), Some("/items"));
}

#[test]
fn unexpected_nested_property_uses_rfc6901_pointer() {
    let error = encode_json(
        &schema(),
        &json(
            r#"{"wide": 0, "delta": 0, "origin": {"x": 0, "y": 0, "extra": 1}, "items": [0, 0]}"#,
        ),
    )
    .expect_err("extra");
    assert!(error.to_string().contains("unexpected property"), "{error}");
    assert_eq!(pointer_of(&error), Some("/origin/extra"));
    assert!(
        first_line(&error).contains("(/origin/extra)"),
        "{}",
        first_line(&error)
    );
}

#[test]
fn parse_time_errors_omit_empty_pointer_parentheses() {
    let error = encode_json(&schema(), &json(r#"{"wide": 1} trailing"#)).expect_err("trailing");
    assert!(error.to_string().contains("trailing"), "{error}");
    assert_eq!(pointer_of(&error), None);
    let line = first_line(&error);
    assert!(
        !line.contains("()"),
        "parse-time error rendered empty pointer parentheses: {line}"
    );
}
