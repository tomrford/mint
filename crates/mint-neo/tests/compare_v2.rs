use mint_core::build::{BlockSelector, BuildFromLayoutsRequest, NamedLayout};
use mint_core::data::JsonDataSource;
use mint_core::layout;
use mint_neo::{Source, compile_header, encode_json};
use std::path::PathBuf;

#[test]
fn flat_scalars_match_mint_v2_bytes() {
    let toml = r#"
[mint]
abi = "generic-le"

[config.header]
start_address = 0x8000
length = 8

[config.data]
id = { name = "id", type = "u32" }
flags = { name = "flags", type = "u16" }
reserved = { name = "reserved", type = "u16" }
"#;
    let config = layout::parse_toml_layout(toml).expect("toml");
    let data = JsonDataSource::from_str(
        r#"{"Default":{"id":1,"flags":2,"reserved":3}}"#,
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
    assert_eq!(bytes, artifact.ranges[0].bytestream);
}

#[test]
fn tricore_u64_alignment_matches_mint_v2() {
    let toml = r#"
[mint]
abi = "tricore-eabi-le"

[config.header]
start_address = 0
length = 12

[config.data]
small = { name = "small", type = "u8" }
wide = { name = "wide", type = "u64" }
"#;
    let config = layout::parse_toml_layout(toml).expect("toml");
    let data = JsonDataSource::from_str(
        r#"{"Default":{"small":1,"wide":2}}"#,
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
    assert_eq!(
        neo.layout.root_layout().size,
        artifact.ranges[0].bytestream.len()
    );
    assert_eq!(bytes, artifact.ranges[0].bytestream);
}
