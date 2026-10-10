use mint_core::build::{self, BlockSelector, BuildFromLayoutsRequest, NamedLayout};
use mint_core::layout;
use std::path::PathBuf;

fn layout(data: &str) -> String {
    layout_for_abi("generic-le", data)
}

fn layout_for_abi(abi: &str, data: &str) -> String {
    format!(
        r#"
[mint]
abi = "{abi}"

[block.header]
start_address = 0x1000
length = 0x100
padding = 0xEE

[block.data]
{data}
"#
    )
}

fn build_output(data: &str) -> Vec<u8> {
    build_output_for_abi("generic-le", data)
}

fn build_output_for_abi(abi: &str, data: &str) -> Vec<u8> {
    let config = layout::parse_toml_layout(&layout_for_abi(abi, data)).expect("layout parses");
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
    .expect("block builds");
    artifact.ranges[0].bytestream.clone()
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

    assert_eq!(output.len(), 40);
    assert_eq!(output[0], 0x11);
    assert_eq!(output[8], 0x22);
    assert_eq!(&output[16..24], &0x7766554433221100u64.to_le_bytes());
    assert_eq!(&output[24..26], &0x3344u16.to_le_bytes());
    assert_eq!(output[32], 0x55);
    assert!(
        output
            .iter()
            .enumerate()
            .filter(|(offset, _)| !matches!(offset, 0 | 8 | 16..=25 | 32))
            .all(|(_, byte)| *byte == 0xEE)
    );
}

#[test]
fn tricore_uses_four_byte_alignment_and_eight_byte_array_stride_for_u64() {
    let output = build_output_for_abi(
        "tricore-eabi-le",
        r#"
word = { value = 0x44332211, type = "u32" }
wide = { value = 0x7766554433221100, type = "u64" }
values = { value = [1, 2], type = "u64", size = 2 }
tail = { value = 0xAA55, type = "u16" }
"#,
    );

    assert_eq!(output.len(), 32);
    assert_eq!(&output[0..4], &0x44332211u32.to_le_bytes());
    assert_eq!(&output[4..12], &0x7766554433221100u64.to_le_bytes());
    assert_eq!(&output[12..20], &1u64.to_le_bytes());
    assert_eq!(&output[20..28], &2u64.to_le_bytes());
    assert_eq!(&output[28..30], &0xAA55u16.to_le_bytes());
}

#[test]
fn tricore_aligns_multi_octet_byte_aggregates_to_two_octets() {
    let output = build_output_for_abi(
        "tricore-eabi-le",
        r#"
first = { value = 0x11, type = "u8" }
single.only = { value = 0x22, type = "u8" }
group.a = { value = 0x33, type = "u8" }
group.b = { value = 0x44, type = "u8" }
group.c = { value = 0x55, type = "u8" }
last = { value = 0x66, type = "u8" }
"#,
    );

    assert_eq!(output, vec![0x11, 0x22, 0x33, 0x44, 0x55, 0xEE, 0x66, 0xEE]);
}

#[test]
fn c28x_matches_compiler_probed_aggregate_shapes() {
    let u16_u64 = build_output_for_abi(
        "ti-c28x-eabi",
        r#"
first = { value = 0x1122, type = "u16" }
second = { value = 0x7766554433221100, type = "u64" }
"#,
    );
    assert_eq!(u16_u64.len(), 12);
    assert_eq!(&u16_u64[4..12], &0x7766554433221100u64.to_le_bytes());

    let u64_u16 = build_output_for_abi(
        "ti-c28x-eabi",
        r#"
first = { value = 0x7766554433221100, type = "u64" }
second = { value = 0x1122, type = "u16" }
"#,
    );
    assert_eq!(u64_u16.len(), 12);
    assert_eq!(&u64_u16[8..10], &0x1122u16.to_le_bytes());

    let u16_f32 = build_output_for_abi(
        "ti-c28x-eabi",
        r#"
first = { value = 0x1122, type = "u16" }
second = { value = 1.5, type = "f32" }
"#,
    );
    assert_eq!(u16_f32.len(), 8);
    assert_eq!(&u16_f32[4..8], &1.5f32.to_le_bytes());
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

    assert_eq!(output.len(), 24);
    assert_eq!(&output[16..20], &0x1004u32.to_le_bytes());
    assert_eq!(&output[20..24], &0x1008u32.to_le_bytes());
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
