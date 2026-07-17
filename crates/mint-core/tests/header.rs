#[path = "common/mod.rs"]
mod common;

use mint_core::build::BlockSelector;

fn generate(name: &str, layout: &str, blocks: impl FnOnce(&str) -> Vec<BlockSelector>) -> String {
    let path = common::write_layout_file(name, layout);
    mint_core::header::generate(&blocks(&path)).expect("header generates")
}

fn error(name: &str, layout: &str) -> String {
    let path = common::write_layout_file(name, layout);
    let error = mint_core::header::generate(&[BlockSelector::all(path)])
        .expect_err("header generation should fail");
    common::error_chain(&error)
}

#[test]
fn maps_all_scalar_fixed_point_and_storage_types() {
    let header = generate(
        "header-types",
        r#"
[mint]
endianness = "little"

[mint.checksum.crc32]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true

[types.header]
start_address = 0
length = 0x200

[types.data]
u8_value = { name = "U8", type = "u8" }
u16_value = { name = "U16", type = "u16" }
u32_value = { name = "U32", type = "u32" }
u64_value = { name = "U64", type = "u64" }
i8_value = { name = "I8", type = "i8" }
i16_value = { name = "I16", type = "i16" }
i32_value = { name = "I32", type = "i32" }
i64_value = { name = "I64", type = "i64" }
f32_value = { name = "F32", type = "f32" }
f64_value = { name = "F64", type = "f64" }
q8_value = { name = "Q8", type = "q3.4" }
q16_value = { name = "Q16", type = "q7.8" }
q32_value = { name = "Q32", type = "q15.16" }
q64_value = { name = "Q64", type = "q31.32" }
uq8_value = { name = "UQ8", type = "uq4.4" }
uq16_value = { name = "UQ16", type = "uq8.8" }
uq32_value = { name = "UQ32", type = "uq16.16" }
uq64_value = { name = "UQ64", type = "uq32.32" }
target = { value = 1, type = "u8" }
pointer = { ref = "target", type = "u32" }
checksum = { checksum = "crc32", type = "u32" }
bitmap = { type = "u64", bitmap = [{ bits = 64, name = "WholeField" }] }
"#,
        |path| vec![BlockSelector::all(path)],
    );

    for declaration in [
        "uint8_t u8_value;",
        "uint16_t u16_value;",
        "uint32_t u32_value;",
        "uint64_t u64_value;",
        "int8_t i8_value;",
        "int16_t i16_value;",
        "int32_t i32_value;",
        "int64_t i64_value;",
        "float f32_value;",
        "double f64_value;",
        "int8_t q8_value; /* q3.4 */",
        "int16_t q16_value; /* q7.8 */",
        "int32_t q32_value; /* q15.16 */",
        "int64_t q64_value; /* q31.32 */",
        "uint8_t uq8_value; /* uq4.4 */",
        "uint16_t uq16_value; /* uq8.8 */",
        "uint32_t uq32_value; /* uq16.16 */",
        "uint64_t uq64_value; /* uq32.32 */",
        "uint32_t pointer;",
        "uint32_t checksum;",
        "uint64_t bitmap; /* bitmap storage */",
    ] {
        assert!(
            header.contains(declaration),
            "missing {declaration}\n{header}"
        );
    }
    assert!(header.contains("#define TYPES_BITMAP_WHOLE_FIELD_SHIFT 0u"));
    assert!(header.contains("#define TYPES_BITMAP_WHOLE_FIELD_MASK UINT64_C(0xFFFFFFFFFFFFFFFF)"));
}

#[test]
fn emits_fingerprint_values_for_firmware_comparison() {
    let header = generate(
        "header-fingerprint",
        r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x1000
length = 0x100

[block.data]
schema = { fingerprint = true, type = "u64" }
version = { value = 1, type = "u16" }
payload = { value = [1, 2, 3], type = "u8", size = 3 }

[invalid.header]
start_address = 0x2000
length = 0x20

[invalid.data]
pointer = { ref = "missing", type = "u32" }
"#,
        |path| vec![BlockSelector::named(path, "block")],
    );

    assert!(header.contains("#define BLOCK_SCHEMA_FINGERPRINT UINT64_C(0x636CA69EB274AAFA)"));
    assert!(header.contains("uint64_t schema; /* fingerprint */"));
}

#[test]
fn emits_array_macros_nested_structs_and_selected_order_deterministically() {
    let layout = r#"
[mint]
endianness = "little"

[first.header]
start_address = 0
length = 0x100

[first.data]
outer.inner.value = { value = 1, type = "u32" }
outer.tail = { value = 2, type = "u16" }
name = { name = "Name", type = "u8", SIZE = 16 }
name_padded = { name = "NamePadded", type = "u8", size = 16 }
matrix = { name = "Matrix", type = "i16", size = [2, 3] }

[second.header]
start_address = 0x100
length = 0x100

[second.data]
value = { value = 3, type = "u8" }
"#;
    let path = common::write_layout_file("header-order", layout);
    let selectors = [
        BlockSelector::named(&path, "second"),
        BlockSelector::named(&path, "first"),
    ];
    let first = mint_core::header::generate(&selectors).expect("header generates");
    let second = mint_core::header::generate(&selectors).expect("header regenerates");

    assert_eq!(first, second);
    assert!(first.contains("#define FIRST_NAME_LEN 16u"));
    assert!(first.contains("#define FIRST_NAME_PADDED_LEN 16u"));
    assert!(first.contains("#define FIRST_MATRIX_ROWS 2u"));
    assert!(first.contains("#define FIRST_MATRIX_COLS 3u"));
    assert!(first.contains("uint8_t name[FIRST_NAME_LEN];"));
    assert!(first.contains("uint8_t name_padded[FIRST_NAME_PADDED_LEN];"));
    assert!(first.contains("int16_t matrix[FIRST_MATRIX_ROWS][FIRST_MATRIX_COLS];"));
    assert!(first.contains(
        "struct {\n    struct {\n      uint32_t value;\n    } inner;\n    uint16_t tail;\n  } outer;"
    ));
    assert!(
        first.find("} second_t;").expect("second typedef")
            < first.find("} first_t;").expect("first typedef")
    );
}

#[test]
fn rejects_names_that_collapse_to_the_same_macro() {
    let array_collision = error(
        "header-array-collision",
        r#"
[mint]
endianness = "little"
[block.header]
start_address = 0
length = 32
[block.data]
fooBar = { name = "One", type = "u8", size = 2 }
foo_bar = { name = "Two", type = "u8", size = 2 }
"#,
    );
    assert!(array_collision.contains("both generate macro 'BLOCK_FOO_BAR_LEN'"));

    let bitmap_collision = error(
        "header-bitmap-collision",
        r#"
[mint]
endianness = "little"
[block.header]
start_address = 0
length = 16
[block.data]
flags = { type = "u8", bitmap = [
  { bits = 4, name = "fooBar" },
  { bits = 4, name = "foo_bar" },
] }
"#,
    );
    assert!(bitmap_collision.contains("both generate macro 'BLOCK_FLAGS_FOO_BAR_SHIFT'"));

    let block_collision = error(
        "header-block-collision",
        r#"
[mint]
endianness = "little"
[fooBar.header]
start_address = 0
length = 16
[fooBar.data]
value = { value = 1, type = "u8" }
[foo_bar.header]
start_address = 16
length = 16
[foo_bar.data]
value = { value = 2, type = "u8" }
"#,
    );
    assert!(block_collision.contains("both convert to macro prefix 'FOO_BAR'"));
}

#[test]
fn delegates_dangling_const_and_oversized_block_validation() {
    let oversized = error(
        "header-oversized",
        r#"
[mint]
endianness = "little"
[block.header]
start_address = 0
length = 4
[block.data]
value = { value = 1, type = "u64" }
"#,
    );
    assert!(
        oversized
            .contains("resolved layout size (8 bytes) exceeds configured block length (4 bytes)"),
        "{oversized}"
    );

    let missing_const = error(
        "header-missing-const",
        r#"
[mint]
endianness = "little"
[block.header]
start_address = 0
length = 16
[block.data]
nested.value = { const = "missing", type = "u32" }
"#,
    );
    assert!(
        missing_const.contains("in field 'nested': in field 'value'"),
        "{missing_const}"
    );
    assert!(
        missing_const.contains("Const 'missing' not found in [mint.const]"),
        "{missing_const}"
    );
}
