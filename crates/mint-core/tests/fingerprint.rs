use mint_core::build::{BlockSelector, BuildFromLayoutsRequest, NamedLayout};
use mint_core::fingerprint;
use mint_core::layout;
use std::path::PathBuf;

#[path = "common/mod.rs"]
mod common;

fn fingerprint_of(source: &str) -> u64 {
    let config = layout::parse_toml_layout(source).expect("layout parses");
    fingerprint::calculate(&config).expect("fingerprint calculates")[0].value
}

fn layout_with(data: &str) -> String {
    format!(
        r#"
[mint]
abi = "generic-le"

[block.header]
start_address = 0x1000
length = 0x100

[block.data]
{data}
"#
    )
}

#[test]
fn names_values_and_producer_sources_do_not_change_the_abi_fingerprint() {
    let first = fingerprint_of(
        r#"
[mint]
abi = "generic-le"

[first.header]
start_address = 0x1000
length = 0x100
padding = 0xFF

[first.data]
alpha = { value = 1, type = "u16" }
nested.beta = { value = [1, 2], type = "u8", size = 2 }
flags = { type = "u8", bitmap = [{ bits = 1, value = 0 }, { bits = 7, name = "Mode" }] }
target = { value = 3, type = "u32" }
pointer = { ref = "target", type = "u32" }
schema = { fingerprint = true, type = "u64" }
"#,
    );
    let second = fingerprint_of(
        r#"
[mint]
abi = "generic-le"

[renamed.header]
start_address = 0x9000
length = 0x400
padding = 0x00

[renamed.data]
different = { name = "External", type = "u16" }
group.items = { value = [8, 9], type = "u8", SIZE = 2 }
bits = { type = "u8", bitmap = [{ bits = 1, name = "Enabled" }, { bits = 7, value = 4 }] }
destination = { const = "renamed.length", type = "u32" }
address = { ref = "destination", type = "u32" }
compatibility = { fingerprint = true, type = "u64" }
"#,
    );

    assert_eq!(first, second);

    let automatic = fingerprint_of(&layout_with(
        "schema = { fingerprint = true, type = \"u64\" }",
    ));
    let literal = fingerprint_of(&layout_with("renamed = { value = 123, type = \"u64\" }"));
    assert_eq!(automatic, literal);
}

#[test]
fn type_shape_abi_and_ref_topology_change_the_fingerprint() {
    let scalar = fingerprint_of(&layout_with("value = { value = 1, type = \"u32\" }"));
    let floating = fingerprint_of(&layout_with("value = { value = 1, type = \"f32\" }"));
    let array = fingerprint_of(&layout_with(
        "value = { value = [1, 2], type = \"u16\", size = 2 }",
    ));
    let big_endian = fingerprint_of(
        &layout_with("value = { value = 1, type = \"u32\" }")
            .replace("abi = \"generic-le\"", "abi = \"generic-be\""),
    );
    let left_ref = fingerprint_of(&layout_with(
        r#"
prefix = { value = 0, type = "u8" }
left = { value = 1, type = "u32" }
right = { value = 2, type = "u32" }
pointer = { ref = "left", type = "u32" }
"#,
    ));
    let right_ref = fingerprint_of(&layout_with(
        r#"
prefix = { value = 0, type = "u8" }
left = { value = 1, type = "u32" }
right = { value = 2, type = "u32" }
pointer = { ref = "right", type = "u32" }
"#,
    ));

    assert_ne!(scalar, floating);
    assert_ne!(scalar, array);
    assert_ne!(scalar, big_endian);
    assert_ne!(left_ref, right_ref);
}

#[test]
fn profile_names_do_not_affect_fingerprints_but_effective_layout_does() {
    let generic_source = layout_with(
        r#"
word = { value = 1, type = "u32" }
wide = { value = 2, type = "u64" }
"#,
    );
    let generic = fingerprint_of(&generic_source);
    let arm =
        fingerprint_of(&generic_source.replace("abi = \"generic-le\"", "abi = \"arm-aapcs32-le\""));
    let tricore = fingerprint_of(
        &generic_source.replace("abi = \"generic-le\"", "abi = \"tricore-eabi-le\""),
    );

    assert_eq!(generic, arm);
    assert_ne!(generic, tricore);

    let generic_u32 = layout_with("word = { value = 1, type = \"u32\" }");
    let tricore_u32 = generic_u32.replace("abi = \"generic-le\"", "abi = \"tricore-eabi-le\"");
    assert_eq!(fingerprint_of(&generic_u32), fingerprint_of(&tricore_u32));
}

#[test]
fn self_and_cross_block_fingerprints_are_injected_from_one_intrinsic_map() {
    let source = r#"
[mint]
abi = "generic-le"

[config.header]
start_address = 0x1000
length = 0x20

[config.data]
schema = { fingerprint = true, type = "u64" }
value = { value = 7, type = "u16" }

[manifest.header]
start_address = 0x2000
length = 0x20

[manifest.data]
config_schema = { fingerprint = "config", type = "u64" }
manifest_schema = { fingerprint = true, type = "u64" }
"#;
    let config = layout::parse_toml_layout(source).expect("layout parses");
    let values = fingerprint::calculate(&config).expect("fingerprints calculate");
    let config_fingerprint = values
        .iter()
        .find(|value| value.block == "config")
        .expect("config fingerprint")
        .value;
    let manifest_fingerprint = values
        .iter()
        .find(|value| value.block == "manifest")
        .expect("manifest fingerprint")
        .value;

    let artifact = mint_core::build::build_from_layouts(BuildFromLayoutsRequest {
        layouts: vec![NamedLayout {
            name: PathBuf::from("fingerprints.toml"),
            config,
        }],
        blocks: vec![BlockSelector::all("fingerprints.toml")],
        data_source: None,
        strict: false,
        capture_values: true,
    })
    .expect("build succeeds");

    assert_eq!(
        &artifact.ranges[0].bytestream[..8],
        &config_fingerprint.to_le_bytes()
    );
    assert_eq!(
        &artifact.ranges[1].bytestream[..8],
        &config_fingerprint.to_le_bytes()
    );
    assert_eq!(
        &artifact.ranges[1].bytestream[8..16],
        &manifest_fingerprint.to_le_bytes()
    );
    let used = artifact.used_values.expect("used values captured");
    assert_eq!(
        used["fingerprints.toml"]["manifest"]["config_schema"].as_u64(),
        Some(config_fingerprint)
    );
}

#[test]
fn fingerprint_has_a_stable_v2_golden_value() {
    let value = fingerprint_of(&layout_with(
        r#"
schema = { fingerprint = true, type = "u64" }
version = { value = 1, type = "u16" }
payload = { value = [1, 2, 3], type = "u8", size = 3 }
"#,
    ));

    assert_eq!(format!("{value:016x}"), "9f2050e2faa654d7");
}

#[test]
fn fingerprint_targets_are_validated_at_parse_time() {
    let error = layout::parse_toml_layout(&layout_with(
        "schema = { fingerprint = false, type = \"u64\" }",
    ))
    .expect_err("false target fails to parse");
    assert!(
        error
            .to_string()
            .contains("fingerprint must be `true` for the containing block or a block name"),
        "unexpected error: {error}"
    );

    let error = layout::parse_toml_layout(&layout_with(
        "schema = { fingerprint = \"\", type = \"u64\" }",
    ))
    .expect_err("empty target fails to parse");
    assert!(
        error
            .to_string()
            .contains("fingerprint block name must not be empty"),
        "unexpected error: {error}"
    );
}

#[test]
fn selected_blocks_build_despite_invalid_non_target_siblings() {
    let source = r#"
[mint]
abi = "generic-le"

[good.header]
start_address = 0x1000
length = 0x20

[good.data]
schema = { fingerprint = true, type = "u64" }
value = { value = 7, type = "u16" }

[dependent.header]
start_address = 0x1800
length = 0x20

[dependent.data]
bad_schema = { fingerprint = "bad", type = "u64" }

[bad.header]
start_address = 0x2000
length = 0x20

[bad.data]
pointer = { ref = "missing", type = "u32" }
"#;
    let config = layout::parse_toml_layout(source).expect("layout parses");
    fingerprint::calculate(&config).expect_err("whole-file calculation rejects the bad ref");

    let artifact = mint_core::build::build_from_layouts(BuildFromLayoutsRequest {
        layouts: vec![NamedLayout {
            name: PathBuf::from("siblings.toml"),
            config,
        }],
        blocks: vec![BlockSelector::named("siblings.toml", "good")],
        data_source: None,
        strict: false,
        capture_values: false,
    })
    .expect("selected block builds without touching the invalid sibling");
    assert_eq!(artifact.ranges.len(), 1);

    let config = layout::parse_toml_layout(source).expect("layout reparses");
    let error = mint_core::build::build_from_layouts(BuildFromLayoutsRequest {
        layouts: vec![NamedLayout {
            name: PathBuf::from("siblings.toml"),
            config,
        }],
        blocks: vec![BlockSelector::named("siblings.toml", "dependent")],
        data_source: None,
        strict: false,
        capture_values: false,
    })
    .expect_err("selected block fails when the invalid sibling is its fingerprint target");
    let message = common::error_chain(&error);
    assert!(
        message.contains("ref target 'missing' not found in block"),
        "{message}"
    );
}

#[test]
fn fingerprints_reject_blocks_that_exceed_their_configured_length() {
    let config = layout::parse_toml_layout(
        r#"
[mint]
abi = "generic-le"

[block.header]
start_address = 0x1000
length = 4

[block.data]
value = { value = 1, type = "u64" }
"#,
    )
    .expect("layout parses");

    let error = fingerprint::calculate_block(&config, "block").expect_err("oversized block fails");
    assert!(
        error
            .to_string()
            .contains("resolved layout size (8 bytes) exceeds configured block length (4 bytes)"),
        "{error}"
    );
}

#[test]
fn fingerprints_reject_selector_static_invalidity() {
    let checksum_at_zero = layout::parse_toml_layout(
        r#"
[mint]
abi = "generic-le"
[mint.checksum.crc32]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true
[block.header]
start_address = 0
length = 16
[block.data]
checksum = { checksum = "crc32", type = "u32" }
"#,
    )
    .expect("layout parses");
    let error = fingerprint::calculate_block(&checksum_at_zero, "block")
        .expect_err("checksum at zero fails static validation");
    let message = common::error_chain(&error);
    assert!(
        message.contains("Checksum must follow at least one data byte"),
        "{message}"
    );

    let ref_overflow = layout::parse_toml_layout(
        r#"
[mint]
abi = "generic-le"
[block.header]
start_address = 0x10000
length = 16
[block.data]
target = { value = 1, type = "u32" }
pointer = { ref = "target", type = "u16" }
"#,
    )
    .expect("layout parses");
    let error = fingerprint::calculate_block(&ref_overflow, "block")
        .expect_err("ref address overflow fails static validation");
    let message = common::error_chain(&error);
    assert!(
        message.contains("ref 'pointer' target 'target'")
            && message.contains("does not fit storage type u16"),
        "{message}"
    );

    let range_overflow = layout::parse_toml_layout(
        r#"
[mint]
abi = "generic-le"
[block.header]
start_address = 0xFFFFFFFF
length = 8
[block.data]
value = { value = 1, type = "u64" }
"#,
    )
    .expect("layout parses");
    let error = fingerprint::calculate_block(&range_overflow, "block")
        .expect_err("emitted range overflow fails static validation");
    assert!(
        error
            .to_string()
            .contains("exceeds the 32-bit address space"),
        "{error}"
    );

    let allocated_overflow = layout::parse_toml_layout(
        r#"
[mint]
abi = "generic-le"
[block.header]
start_address = 0xFFFFF000
length = 0x2000
[block.data]
value = { value = 1, type = "u8" }
"#,
    )
    .expect("layout parses");
    let error = fingerprint::calculate_block(&allocated_overflow, "block")
        .expect_err("allocated range overflow fails static validation like the build output stage");
    assert!(
        error
            .to_string()
            .contains("exceeds the 32-bit address space"),
        "{error}"
    );
}

#[test]
fn fingerprint_static_range_accepts_the_last_address() {
    let config = layout::parse_toml_layout(
        r#"
[mint]
abi = "generic-le"
[block.header]
start_address = 0xFFFFFFFF
length = 1
[block.data]
value = { value = 1, type = "u8" }
"#,
    )
    .expect("layout parses");

    fingerprint::calculate_block(&config, "block").expect("last address is valid");
}

#[test]
fn fingerprint_fields_reject_invalid_storage_and_unknown_blocks() {
    let wrong_type = layout::parse_toml_layout(&layout_with(
        "schema = { fingerprint = true, type = \"u32\" }",
    ))
    .expect("layout parses");
    let error = fingerprint::calculate(&wrong_type).expect_err("wrong type fails");
    assert!(error.to_string().contains("Fingerprint type must be u64"));

    let unknown = layout::parse_toml_layout(&layout_with(
        "schema = { fingerprint = \"missing\", type = \"u64\" }",
    ))
    .expect("layout parses");
    let error = fingerprint::calculate(&unknown).expect_err("unknown block fails");
    assert!(error.to_string().contains("fingerprint target 'missing'"));
}
