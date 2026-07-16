use mint_core::build::{BlockSelector, BuildFromLayoutsRequest, NamedLayout};
use mint_core::data::JsonDataSource;
use mint_core::layout;
use mint_core::layout::resolved::ResolvedLayout;
use std::path::PathBuf;

#[test]
fn resolution_matches_builder_offsets_and_total_size() {
    let config = layout::parse_toml_layout(
        r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x1000
length = 0x200
padding = 0xEE

[block.data]
prefix = { value = 0x11, type = "u8" }
header = { value = 0x2233, type = "u16" }
group.byte = { value = 0x44, type = "u8" }
group.half = { value = 0x5566, type = "u16" }
group.deep.word = { value = 0x778899AA, type = "u32" }
group.deep.wide = { value = 0x1122334455667788, type = "u64" }
group.deep.floating = { value = 1.5, type = "f64" }
arrays.one = { value = [1, 2, 3], type = "u16", size = 3 }
arrays.two = { name = "Matrix", type = "u32", size = [2, 2] }
flags = { type = "u32", bitmap = [
    { bits = 1, value = 1 },
    { bits = 7, value = 2 },
    { bits = 24, value = 3 },
] }
fixed.qvalue = { value = -1.25, type = "q7.8" }
fixed.uqvalue = { value = 2.5, type = "uq16.16" }
schema = { fingerprint = true, type = "u64" }
refs.prefix = { ref = "prefix", type = "u32" }
refs.header = { ref = "header", type = "u32" }
refs.group = { ref = "group", type = "u32" }
refs.byte = { ref = "group.byte", type = "u32" }
refs.half = { ref = "group.half", type = "u32" }
refs.deep = { ref = "group.deep", type = "u32" }
refs.word = { ref = "group.deep.word", type = "u32" }
refs.wide = { ref = "group.deep.wide", type = "u32" }
refs.floating = { ref = "group.deep.floating", type = "u32" }
refs.one = { ref = "arrays.one", type = "u32" }
refs.array = { ref = "arrays.two", type = "u32" }
refs.bitmap = { ref = "flags", type = "u32" }
refs.qvalue = { ref = "fixed.qvalue", type = "u32" }
refs.fixed = { ref = "fixed.uqvalue", type = "u32" }
refs.fingerprint = { ref = "schema", type = "u32" }
"#,
    )
    .expect("layout parses");

    let resolved = ResolvedLayout::new(&config.blocks["block"].data).expect("layout resolves");
    let expected_leaf_order = [
        "prefix",
        "header",
        "group.byte",
        "group.half",
        "group.deep.word",
        "group.deep.wide",
        "group.deep.floating",
        "arrays.one",
        "arrays.two",
        "flags",
        "fixed.qvalue",
        "fixed.uqvalue",
        "schema",
        "refs.prefix",
        "refs.header",
        "refs.group",
        "refs.byte",
        "refs.half",
        "refs.deep",
        "refs.word",
        "refs.wide",
        "refs.floating",
        "refs.one",
        "refs.array",
        "refs.bitmap",
        "refs.qvalue",
        "refs.fixed",
        "refs.fingerprint",
    ];
    assert_eq!(
        resolved.leaves().map(|leaf| leaf.path).collect::<Vec<_>>(),
        expected_leaf_order
    );

    let start_address = config.blocks["block"].header.start_address;
    let resolved_refs = [
        ("refs.prefix", "prefix"),
        ("refs.header", "header"),
        ("refs.group", "group"),
        ("refs.byte", "group.byte"),
        ("refs.half", "group.half"),
        ("refs.deep", "group.deep"),
        ("refs.word", "group.deep.word"),
        ("refs.wide", "group.deep.wide"),
        ("refs.floating", "group.deep.floating"),
        ("refs.one", "arrays.one"),
        ("refs.array", "arrays.two"),
        ("refs.bitmap", "flags"),
        ("refs.qvalue", "fixed.qvalue"),
        ("refs.fixed", "fixed.uqvalue"),
        ("refs.fingerprint", "schema"),
    ]
    .map(|(reference, target)| {
        (
            reference,
            target,
            resolved
                .coordinates(reference)
                .unwrap_or_else(|| panic!("resolved reference {reference}"))
                .offset,
            resolved
                .coordinates(target)
                .unwrap_or_else(|| panic!("resolved target {target}"))
                .offset,
        )
    });
    let resolved_size = resolved.total_size();
    let data_source = JsonDataSource::from_str(
        r#"{"Default":{"Matrix":[[4,5],[6,7]]}}"#,
        &["Default".to_owned()],
    )
    .expect("data source parses");

    let artifact = mint_core::build::build_from_layouts(BuildFromLayoutsRequest {
        layouts: vec![NamedLayout {
            name: PathBuf::from("resolved.toml"),
            config,
        }],
        blocks: vec![BlockSelector::named("resolved.toml", "block")],
        data_source: Some(&data_source),
        strict: false,
        capture_values: false,
    })
    .expect("block builds");
    let bytes = &artifact.ranges[0].bytestream;

    for (reference, target, reference_offset, target_offset) in resolved_refs {
        let actual_address = u32::from_le_bytes(
            bytes[reference_offset..reference_offset + 4]
                .try_into()
                .expect("four-byte ref"),
        );
        assert_eq!(
            actual_address,
            start_address + target_offset as u32,
            "builder and resolution disagree for {reference} targeting {target}"
        );
    }

    assert_eq!(resolved_size, bytes.len());
}
