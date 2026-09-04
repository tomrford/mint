use mint_core::build::{BlockSelector, BuildFromLayoutsRequest, NamedLayout};
use mint_core::data::JsonDataSource;
use mint_core::layout;
use mint_neo::{Source, compile_header, encode_json};
use std::path::PathBuf;

fn v2_range(toml: &str, json_body: &str) -> Vec<u8> {
    let config = layout::parse_toml_layout(toml).expect("toml");
    let data = JsonDataSource::from_str(
        &format!(r#"{{"Default":{json_body}}}"#),
        &["Default".to_owned()],
    )
    .expect("json");
    let artifact = mint_core::build::build_from_layouts(BuildFromLayoutsRequest {
        layouts: vec![NamedLayout {
            name: PathBuf::from("v2"),
            config,
        }],
        blocks: vec![BlockSelector::named("v2", "config")],
        data_source: Some(&data),
        strict: true,
        capture_values: false,
    })
    .expect("v2 build");
    artifact.ranges[0].bytestream.clone()
}

#[test]
fn flat_scalars_match_mint_v2_bytes() {
    let expected = v2_range(
        r#"
[mint]
abi = "generic-le"

[config.header]
start_address = 0x8000
length = 8

[config.data]
id = { name = "id", type = "u32" }
flags = { name = "flags", type = "u16" }
reserved = { name = "reserved", type = "u16" }
"#,
        r#"{"id":1,"flags":2,"reserved":3}"#,
    );

    let neo = compile_header(Source::new(
        "config.h",
        r#"
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
"#,
    ))
    .expect("neo header");
    let bytes = encode_json(
        &neo,
        &Source::new("config.json", r#"{"id":1,"flags":2,"reserved":3}"#),
    )
    .expect("neo json");
    assert_eq!(bytes, expected);
}

#[test]
fn tricore_u64_alignment_matches_mint_v2() {
    let expected = v2_range(
        r#"
[mint]
abi = "tricore-eabi-le"

[config.header]
start_address = 0
length = 12

[config.data]
small = { name = "small", type = "u8" }
wide = { name = "wide", type = "u64" }
"#,
        r#"{"small":1,"wide":2}"#,
    );

    let neo = compile_header(Source::new(
        "config.h",
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi tricore-eabi-le
 * @mint start-address 0
 */
typedef struct {
    uint8_t small;
    uint64_t wide;
} config_t;
"#,
    ))
    .expect("neo header");
    let bytes = encode_json(&neo, &Source::new("config.json", r#"{"small":1,"wide":2}"#))
        .expect("neo json");
    assert_eq!(neo.layout.root_layout().size, expected.len());
    assert_eq!(bytes, expected);
}

#[test]
fn every_neo_scalar_layout_matches_mint_profiles() {
    use mint_core::layout::{abi::Abi, scalar_type::ScalarType};
    let scalars = [
        ScalarType::U8,
        ScalarType::U16,
        ScalarType::U32,
        ScalarType::U64,
        ScalarType::I8,
        ScalarType::I16,
        ScalarType::I32,
        ScalarType::I64,
        ScalarType::F32,
        ScalarType::F64,
    ];
    let c_names = [
        "uint8_t", "uint16_t", "uint32_t", "uint64_t", "int8_t", "int16_t", "int32_t", "int64_t",
        "float", "double",
    ];
    for abi in Abi::ALL {
        for (scalar, c_name) in scalars.into_iter().zip(c_names) {
            let schema = compile_header(Source::new(
                "config.h",
                format!(
                    "/**\n * @mint block\n * @mint abi {}\n * @mint start-address 0\n */\ntypedef struct {{ uint16_t prefix; {c_name} values[2]; }} config_t;",
                    abi.name()
                ),
            ));
            let Ok(expected) = abi.scalar(scalar) else {
                assert!(schema.is_err());
                continue;
            };
            let schema = schema.unwrap();
            let field = &schema.layout.root_fields()[1];
            assert_eq!(
                field.size,
                2 * expected.storage_size,
                "{} {c_name}",
                abi.name()
            );
            assert_eq!(
                field.alignment,
                expected.alignment,
                "{} {c_name}",
                abi.name()
            );
            assert_eq!(
                match &schema.layout.layouts[field.type_id.0].kind {
                    mint_neo::LayoutKind::Array(array) => array.stride,
                    _ => panic!("array expected"),
                },
                expected.array_stride,
                "{} {c_name}",
                abi.name()
            );
        }
    }
}

#[test]
fn floating_point_bytes_and_padding_match_mint_on_every_abi() {
    use mint_core::layout::abi::{Abi, AbiFamily};
    for abi in Abi::ALL {
        let length = match abi.family() {
            AbiFamily::NaturalAlign4 => 16,
            AbiFamily::GenericNatural => 24,
            _ => panic!("add comparison for the new ABI family"),
        };
        let data = r#"{"prefix":7,"wide":-1.5,"gain":2.25}"#;
        let expected = v2_range(
            &format!(
                r#"
[mint]
abi = "{abi}"
[config.header]
start_address = 0
length = {length}
[config.data]
prefix = {{ name = "prefix", type = "u16" }}
wide = {{ name = "wide", type = "f64" }}
gain = {{ name = "gain", type = "f32" }}
"#,
                abi = abi.name()
            ),
            data,
        );
        let schema = compile_header(Source::new("config.h", format!(
            "/**\n * @mint block\n * @mint abi {}\n * @mint start-address 0\n */\ntypedef struct {{ uint16_t prefix; double wide; float gain; }} config_t;", abi.name()))).unwrap();
        assert_eq!(
            encode_json(&schema, &Source::new("config.json", data)).unwrap(),
            expected,
            "{}",
            abi.name()
        );
    }
}
