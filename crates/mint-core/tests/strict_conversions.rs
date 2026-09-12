use mint_core::data::{DataSource, JsonDataSource};
use mint_core::error::MintError;

#[path = "common/mod.rs"]
mod common;

fn build_inline(
    data: &str,
    strict: bool,
    data_source: Option<&dyn DataSource>,
) -> Result<Vec<u8>, MintError> {
    let layout = common::write_layout_file(
        "strict_conversions",
        &format!(
            r#"
[mint]
abi = "generic-le"

[block.header]
start_address = 0x80000
length = 0x100
padding = 0x00

[block.data]
{data}
"#
        ),
    );
    common::build_block(layout, "block", strict, data_source)
}

#[test]
fn non_strict_integer_conversions_saturate() {
    let bytes = build_inline(
        r#"
overflow.u8_high = { value = 256, type = "u8" }
overflow.u8_low = { value = -1, type = "u8" }
overflow.i8_high = { value = 128, type = "i8" }
overflow.i8_low = { value = -129, type = "i8" }
overflow.u8_float_trunc = { value = 1.5, type = "u8" }
overflow.u8_float_high = { value = 300.0, type = "u8" }
"#,
        false,
        None,
    )
    .expect("non-strict converts");

    assert_eq!(
        &bytes[..6],
        &[0xff, 0x00, 0x7f, 0x80, 0x01, 0xff],
        "non-strict integer conversions should saturate, while floats still truncate"
    );
}

#[test]
fn strict_conversions_reject_lossy_inline_values() {
    for (entry, expected_error) in [
        (
            r#"bad = { value = 16777217.0, type = "f32" }"#,
            "lossy float conversion to f32",
        ),
        (
            r#"bad = { value = 1.5, type = "u8" }"#,
            "float to integer conversion not allowed unless value is an exact integer",
        ),
        (
            r#"bad = { value = 9007199254740993, type = "f64" }"#,
            "lossy integer to float conversion not allowed in strict mode",
        ),
        (
            r#"bad = { value = 18446744073709551616.0, type = "u64" }"#,
            "out of range for u64",
        ),
        (
            r#"bad = { value = 9223372036854775808.0, type = "i64" }"#,
            "out of range for i64",
        ),
    ] {
        let error = build_inline(entry, true, None)
            .expect_err("strict mode should reject a lossy inline conversion");
        let message = common::error_chain(&error);
        assert!(message.contains(expected_error), "{entry}: {message}");
    }
}

#[test]
fn strict_conversions_reject_lossy_u64_json_value() {
    let variants = vec!["Default".to_owned()];
    let data_source =
        JsonDataSource::from_str(r#"{"Default":{"Value":18446744073709551615}}"#, &variants)
            .expect("datasource loads");

    let error = build_inline(
        r#"bad = { name = "Value", type = "f64" }"#,
        true,
        Some(&data_source),
    )
    .expect_err("strict mode should reject a lossy u64 to f64 conversion");
    assert!(
        common::error_chain(&error)
            .contains("lossy integer to float conversion not allowed in strict mode"),
        "{error}"
    );
}

#[test]
fn strict_conversions_accept_exact_values_and_bool_literals() {
    let bytes = build_inline(
        r#"
exact.float_to_i16 = { value = 42.0, type = "i16" }
exact.int_to_f32 = { value = 16777216, type = "f32" }
bools.true_flag = { value = true, type = "u8" }
bools.false_flag = { value = false, type = "u8" }
bools.array_flags = { value = [true, false, true], type = "u8", size = 3 }
"#,
        true,
        None,
    )
    .expect("exact strict conversions and bool literals convert");

    assert_eq!(
        &bytes[..13],
        &[
            0x2A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x4B, 1, 0, 1, 0, 1
        ]
    );
}

#[test]
fn non_finite_inputs_are_rejected_by_encoding_without_json_reporting() {
    use mint_core::layout::{abi::Endianness, scalar_type::ScalarType, value::DataValue};
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for scalar in [ScalarType::U8, ScalarType::F32, ScalarType::F64] {
            for strict in [false, true] {
                let error = DataValue::F64(value)
                    .to_bytes(scalar, Endianness::Little, strict)
                    .unwrap_err();
                assert!(
                    error.to_string().contains("cannot encode non-finite"),
                    "{error}"
                );
            }
        }
    }
}
