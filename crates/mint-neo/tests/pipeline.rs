use mint_neo::{InspectFormat, Source, compile_header, encode_json, inspect, render_hex};

fn header(text: &str) -> Source {
    Source::new("config.h", text)
}

fn json(text: &str) -> Source {
    Source::new("config.json", text)
}

fn blocked(prelude: &str, root: &str) -> String {
    format!(
        "#include <stdint.h>\n{prelude}/**\n * @mint block\n * @mint abi generic-le\n * @mint start-address 0\n */\n{root}\n"
    )
}

const FLAT: &str = r#"
#pragma once
#include <stdint.h>

/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0x8000
 */
typedef struct {
    uint32_t id;
    uint16_t flags;
    uint16_t reserved;
} config_t;
"#;

#[test]
fn compiles_flat_record_and_inspects_layout() {
    let schema = compile_header(header(FLAT)).expect("header");
    assert_eq!(schema.layout().abi.name(), "generic-le");
    assert_eq!(schema.layout().start_address, 0x8000);
    assert_eq!(schema.layout().root_layout().size, 8);
    assert_eq!(schema.layout().root_layout().alignment, 4);
    let text = inspect(&schema, InspectFormat::Text).unwrap();
    assert!(text.contains("id"));
    assert!(text.contains("uint32_t"));
    assert!(text.contains("fingerprint:"));
    assert_eq!(
        mint_neo::schema_fingerprint_hex(&schema),
        "e38f2d7a4d9caaaf"
    );
}

#[test]
fn encodes_json_and_writes_i32hex() {
    let schema = compile_header(header(FLAT)).expect("header");
    let bytes =
        encode_json(&schema, &json(r#"{"id": 1, "flags": 2, "reserved": 3}"#)).expect("json");
    assert_eq!(bytes, [1, 0, 0, 0, 2, 0, 3, 0]);
    let hex = render_hex(&schema, &bytes).expect("hex");
    assert!(hex.starts_with(":020000040000FA\n"));
    assert!(
        hex.contains(":08800000010000000200030072"),
        "hex was {hex:?}"
    );
    assert!(hex.ends_with(":00000001FF\n"));
    assert!(!hex.contains("\r\n"));
}

#[test]
fn rejects_missing_and_extra_json_and_fingerprint_presence() {
    let schema = compile_header(header(
        r#"
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct {
    /** @mint fingerprint */
    uint64_t fingerprint;
    uint32_t id;
} config_t;
"#,
    ))
    .expect("header");
    let extra = encode_json(&schema, &json(r#"{"id": 1, "fingerprint": 0}"#)).unwrap_err();
    assert!(extra.to_string().contains("fingerprint"));
    let missing = encode_json(&schema, &json(r#"{}"#)).unwrap_err();
    assert!(missing.to_string().contains("missing"));
    let bytes = encode_json(&schema, &json(r#"{"id": 7}"#)).expect("json");
    assert_eq!(&bytes[8..12], &[7, 0, 0, 0]);
    assert_eq!(&bytes[0..8], &schema.fingerprint().to_le_bytes());
    assert_eq!(&bytes[12..16], &[0xFF; 4], "default tail padding");
}

#[test]
fn nested_records_and_arrays_round_trip() {
    let schema = compile_header(header(
        r#"
#include <stdint.h>
#define CHANNEL_COUNT 2u

typedef struct {
    uint16_t x;
    uint16_t y;
} point_t;

/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0x100
 */
typedef struct {
    uint32_t id;
    point_t origin;
    point_t samples[CHANNEL_COUNT];
} config_t;
"#,
    ))
    .expect("header");
    assert_eq!(schema.layout().root_layout().size, 16);
    let bytes = encode_json(
        &schema,
        &json(
            r#"{
              "id": 42,
              "origin": {"x": 10, "y": 20},
              "samples": [{"x": 1, "y": 2}, {"x": 3, "y": 4}]
            }"#,
        ),
    )
    .expect("json");
    assert_eq!(bytes, [42, 0, 0, 0, 10, 0, 20, 0, 1, 0, 2, 0, 3, 0, 4, 0]);
}

#[test]
fn typedef_array_matches_direct_array_fingerprint() {
    let direct = compile_header(header(
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct {
    uint16_t grid[3][4];
} config_t;
"#,
    ))
    .expect("direct");
    let aliased = compile_header(header(
        r#"
#include <stdint.h>
typedef uint16_t row_t[4];
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct {
    row_t grid[3];
} config_t;
"#,
    ))
    .expect("alias");
    assert_eq!(direct.fingerprint(), aliased.fingerprint());
    assert_eq!(direct.layout().root_layout().size, 24);
}

#[test]
fn layout_equivalent_dimensions_hash_differently() {
    let a = compile_header(header(
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct { uint16_t values[2][6]; } config_t;
"#,
    ))
    .unwrap();
    let b = compile_header(header(
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct { uint16_t values[12]; } config_t;
"#,
    ))
    .unwrap();
    assert_ne!(a.fingerprint(), b.fingerprint());
}

#[test]
fn names_comments_start_and_padding_do_not_change_fingerprint() {
    let a = compile_header(header(
        r#"
#include <stdint.h>
/**
 * config A
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0x10
 * @mint padding 0xAA
 */
typedef struct {
    uint32_t alpha;
} first_t;
"#,
    ))
    .unwrap();
    let b = compile_header(header(
        r#"
#include <stdint.h>
/**
 * config B
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0x2000
 */
typedef struct {
    uint32_t beta;
} second_t;
"#,
    ))
    .unwrap();
    assert_eq!(a.fingerprint(), b.fingerprint());
}

#[test]
fn abi_and_scalar_changes_change_fingerprint() {
    let le = compile_header(header(
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct { uint32_t id; } config_t;
"#,
    ))
    .unwrap();
    let be = compile_header(header(
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-be
 * @mint start-address 0
 */
typedef struct { uint32_t id; } config_t;
"#,
    ))
    .unwrap();
    let other = compile_header(header(
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct { uint16_t id; } config_t;
"#,
    ))
    .unwrap();
    assert_ne!(le.fingerprint(), be.fingerprint());
    assert_ne!(le.fingerprint(), other.fingerprint());
}

#[test]
fn rejects_excluded_reachable_constructs() {
    let cases = [
        (
            "pointer",
            blocked("", "typedef struct { uint32_t *ptr; } config_t;"),
            "pointers are not supported",
        ),
        (
            "union",
            blocked(
                "",
                "typedef struct {\n    union { uint32_t a; uint16_t b; } value;\n} config_t;",
            ),
            "unions are not supported",
        ),
        (
            "bitfield",
            blocked("", "typedef struct { uint32_t flags : 3; } config_t;"),
            "bitfields are not supported",
        ),
        (
            "enum member",
            blocked(
                "typedef enum { A = 1 } e_t;\n",
                "typedef struct { e_t kind; } config_t;",
            ),
            "enum-typed members are not supported",
        ),
        (
            "duplicate enumerator",
            blocked(
                "enum { ITEM_COUNT = 1, ITEM_COUNT = 2 };\n",
                "typedef struct { uint16_t items[ITEM_COUNT]; } config_t;",
            ),
            "duplicate enumerator",
        ),
        (
            "pragma pack",
            "#pragma pack(1)\n".to_owned(),
            "unsupported preprocessor directive (pragma)",
        ),
        (
            "postfix packed helper",
            blocked(
                "typedef struct {\n    uint8_t a;\n    uint32_t b;\n} packed_t __attribute__((packed));\n",
                "typedef struct { packed_t item; } config_t;",
            ),
            "attributes and explicit alignment",
        ),
        (
            "tagged packed struct",
            blocked(
                "struct __attribute__((packed)) Foo {\n    uint8_t a;\n    uint32_t b;\n};\n",
                "typedef struct { struct Foo item; } config_t;",
            ),
            "attributes and explicit alignment",
        ),
        (
            "aligned typedef",
            blocked(
                "typedef _Alignas(16) uint32_t aligned_t;\n",
                "typedef struct { uint8_t lead; aligned_t id; } config_t;",
            ),
            "_Alignas",
        ),
        (
            "atomic helper",
            blocked(
                "typedef _Atomic uint32_t atomic_t;\n",
                "typedef struct { atomic_t id; } config_t;",
            ),
            "_Atomic",
        ),
    ];
    for (name, source, needle) in cases {
        let error = compile_header(header(&source)).expect_err(name);
        let message = error.to_string();
        assert!(
            message.contains(needle),
            "{name}: expected {needle:?} in {message}"
        );
    }
}

#[test]
fn rejects_duplicate_json_keys_and_null() {
    let schema = compile_header(header(FLAT)).unwrap();
    let dup = encode_json(&schema, &json(r#"{"id":1,"id":2,"flags":0,"reserved":0}"#));
    assert!(dup.is_err());
    let null = encode_json(&schema, &json(r#"{"id":null,"flags":0,"reserved":0}"#));
    assert!(null.is_err());
}

#[test]
fn tricore_raises_aggregate_alignment() {
    let schema = compile_header(header(
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi tricore-eabi-le
 * @mint start-address 0
 */
typedef struct {
    uint8_t a;
    uint8_t b;
} config_t;
"#,
    ))
    .unwrap();
    assert_eq!(schema.layout().root_layout().alignment, 2);
    assert_eq!(schema.layout().root_layout().size, 2);
}

#[test]
fn block_tags_and_shape_enums() {
    let schema = compile_header(header(
        r#"
#include <stdint.h>
typedef enum { AXIS_COUNT = 3 } dimensions_t;
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct {
    int16_t axes[AXIS_COUNT];
} config_t;
"#,
    ))
    .expect("header");
    assert_eq!(schema.layout().root_layout().size, 6);
    let bytes = encode_json(&schema, &json(r#"{"axes":[1,2,3]}"#)).unwrap();
    assert_eq!(bytes, [1, 0, 2, 0, 3, 0]);
}

#[test]
fn hex_emits_ela_when_upper_address_changes() {
    let schema = compile_header(header(
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0xFFF0
 */
typedef struct { uint8_t bytes[32]; } config_t;
"#,
    ))
    .unwrap();
    let bytes = encode_json(
        &schema,
        &json(r#"{"bytes":[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31]}"#),
    )
    .unwrap();
    let hex = render_hex(&schema, &bytes).unwrap();
    assert!(hex.contains(":020000040000FA\n"));
    assert!(hex.contains(":020000040001F9\n"), "{hex}");
}

#[test]
fn c28x_rejects_uint8_and_encodes_float32() {
    assert!(
        compile_header(header(
            r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi ti-c28x-eabi
 * @mint start-address 0
 */
typedef struct { uint8_t x; } config_t;
"#,
        ))
        .is_err()
    );
    let schema = compile_header(header(
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-be
 * @mint start-address 0
 */
typedef struct { float32_t gain; } config_t;
"#,
    ))
    .unwrap();
    let bytes = encode_json(&schema, &json(r#"{"gain": 1.0}"#)).unwrap();
    assert_eq!(bytes, [0x3F, 0x80, 0x00, 0x00]);
}
