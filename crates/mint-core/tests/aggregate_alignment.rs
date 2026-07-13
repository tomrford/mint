use mint_core::build::{self, BlockSelector, BuildFromLayoutsRequest, NamedLayout};
use mint_core::layout;
use mint_core::layout::block::BuildOutput;
use mint_core::layout::used_values::NoopValueSink;
use mint_core::output::checksum::calculate_crc;
use std::path::PathBuf;

fn layout(data: &str) -> String {
    format!(
        r#"
[mint]
endianness = "little"

[mint.checksum.crc32]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true

[block.header]
start_address = 0x1000
length = 0x100
padding = 0xEE

[block.data]
{data}
"#
    )
}

fn build_output(data: &str) -> BuildOutput {
    let config = layout::parse_toml_layout(&layout(data)).expect("layout parses");
    let mut value_sink = NoopValueSink;
    config.blocks["block"]
        .build_bytestream(None, &config.mint, false, &mut value_sink)
        .expect("block builds")
}

#[test]
fn nested_aggregate_has_leading_and_tail_padding() {
    let output = build_output(
        r#"
prefix = { value = 0x11, type = "u8" }
group.small = { value = 0x22, type = "u8" }
group.word = { value = 0x44332211, type = "u32" }
after = { value = 0x33, type = "u8" }
"#,
    );

    assert_eq!(
        output.bytestream,
        vec![
            0x11, 0xEE, 0xEE, 0xEE, 0x22, 0xEE, 0xEE, 0xEE, 0x11, 0x22, 0x33, 0x44, 0x33, 0xEE,
            0xEE, 0xEE,
        ]
    );
    assert_eq!(output.padding_count, 9);
}

#[test]
fn aggregates_align_recursively_beyond_one_nesting_depth() {
    let output = build_output(
        r#"
prefix = { value = 0x11, type = "u8" }
outer.inner.byte = { value = 0x22, type = "u8" }
outer.inner.wide = { value = 0x7766554433221100, type = "u64" }
outer.after = { value = 0x3344, type = "u16" }
sibling = { value = 0x55, type = "u8" }
"#,
    );

    assert_eq!(output.bytestream.len(), 40);
    assert_eq!(output.bytestream[0], 0x11);
    assert_eq!(output.bytestream[8], 0x22);
    assert_eq!(
        &output.bytestream[16..24],
        &0x7766554433221100u64.to_le_bytes()
    );
    assert_eq!(&output.bytestream[24..26], &0x3344u16.to_le_bytes());
    assert_eq!(output.bytestream[32], 0x55);
    assert!(
        output
            .bytestream
            .iter()
            .enumerate()
            .filter(|(offset, _)| !matches!(offset, 0 | 8 | 16..=25 | 32))
            .all(|(_, byte)| *byte == 0xEE)
    );
}

#[test]
fn refs_follow_aligned_branch_and_leaf_offsets() {
    let output = build_output(
        r#"
prefix = { value = 0x11, type = "u8" }
group.small = { value = 0x22, type = "u8" }
group.word = { value = 0x44332211, type = "u32" }
after = { value = 0x33, type = "u8" }
branch_ref = { ref = "group", type = "u32" }
leaf_ref = { ref = "group.word", type = "u32" }
"#,
    );

    assert_eq!(output.bytestream.len(), 24);
    assert_eq!(&output.bytestream[16..20], &0x1004u32.to_le_bytes());
    assert_eq!(&output.bytestream[20..24], &0x1008u32.to_le_bytes());
}

#[test]
fn checksum_includes_aggregate_alignment_padding() {
    let source = layout(
        r#"
prefix = { value = 0x11, type = "u8" }
group.word = { value = 0x44332211, type = "u32" }
group.small = { value = 0x22, type = "u8" }
checksum = { checksum = "crc32", type = "u32" }
"#,
    );
    let config = layout::parse_toml_layout(&source).expect("layout parses");
    let mut value_sink = NoopValueSink;
    let output = config.blocks["block"]
        .build_bytestream(None, &config.mint, false, &mut value_sink)
        .expect("block builds");
    let checksum = calculate_crc(&output.bytestream[..12], &config.mint.checksum["crc32"]);

    assert_eq!(
        &output.bytestream[..12],
        &[
            0x11, 0xEE, 0xEE, 0xEE, 0x11, 0x22, 0x33, 0x44, 0x22, 0xEE, 0xEE, 0xEE
        ]
    );
    assert_eq!(&output.bytestream[12..16], &checksum.to_le_bytes());
    assert_eq!(output.checksum_values, vec![checksum]);
}

#[test]
fn root_tail_padding_sets_reserved_size() {
    let config = layout::parse_toml_layout(&layout(
        r#"
word = { value = 0x44332211, type = "u32" }
last = { value = 0x55, type = "u8" }
"#,
    ))
    .expect("layout parses");
    let artifact = build::build_from_layouts(BuildFromLayoutsRequest {
        layouts: vec![NamedLayout {
            name: PathBuf::from("aggregate.toml"),
            config,
        }],
        blocks: vec![BlockSelector::named("aggregate.toml", "block")],
        data_source: None,
        strict: false,
        capture_values: false,
    })
    .expect("build succeeds");

    assert_eq!(artifact.ranges[0].reserved_size, 8);
    assert_eq!(
        artifact.ranges[0].bytestream,
        vec![0x11, 0x22, 0x33, 0x44, 0x55, 0xEE, 0xEE, 0xEE]
    );
}
