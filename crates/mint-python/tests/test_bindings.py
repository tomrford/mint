from pathlib import Path

import pytest

import mint


ROOT = Path(__file__).resolve().parents[3]
LAYOUT = ROOT / "crates" / "mint-core" / "tests" / "data" / "blocks.toml"


def test_file_layout_builds_selected_blocks_and_exports_used_values():
    layout = mint.Layout.from_file(str(LAYOUT))

    result = mint.build(layout.blocks("simple_block"))

    assert result.stats.blocks_processed == 1
    assert result.stats.block_stats[0].name == "simple_block"
    assert result.ranges[0].start_address == 0x8000
    assert result.used_values[str(LAYOUT)]["simple_block"]["device"]["id"] == 0x1234
    assert result.to_intel_hex().startswith(":")


def test_from_string_builds_all_blocks_when_no_names_are_given():
    layout = mint.Layout.from_string(
        "generated.toml",
        """
        [mint]
        endianness = "little"

        [one.header]
        start_address = 0x1000
        length = 0x10

        [one.data]
        value = { value = 1, type = "u8" }

        [two.header]
        start_address = 0x1010
        length = 0x10

        [two.data]
        value = { value = 2, type = "u8" }
        """,
    )

    result = mint.build(layout.blocks())

    assert result.stats.blocks_processed == 2
    assert [r.start_address for r in result.ranges] == [0x1000, 0x1010]
    assert set(result.used_values["generated.toml"]) == {"one", "two"}


def test_from_string_accepts_varargs_block_names():
    layout = mint.Layout.from_string(
        "generated.toml",
        """
        [mint]
        endianness = "little"

        [one.header]
        start_address = 0x1000
        length = 0x10

        [one.data]
        value = { value = 1, type = "u8" }

        [two.header]
        start_address = 0x1010
        length = 0x10

        [two.data]
        value = { value = 2, type = "u8" }
        """,
    )

    result = mint.build(layout.blocks("two", "one"))

    assert result.stats.blocks_processed == 2
    assert [stat.name for stat in result.stats.block_stats] == ["two", "one"]


def test_duplicate_layout_name_with_different_source_fails():
    first = mint.Layout.from_string(
        "generated.toml",
        """
        [mint]
        endianness = "little"

        [one.header]
        start_address = 0x1000
        length = 0x10

        [one.data]
        value = { value = 1, type = "u8" }
        """,
    )
    second = mint.Layout.from_string(
        "generated.toml",
        """
        [mint]
        endianness = "little"

        [two.header]
        start_address = 0x1010
        length = 0x10

        [two.data]
        value = { value = 2, type = "u8" }
        """,
    )

    with pytest.raises(ValueError, match="multiple sources"):
        mint.build(first.blocks("one") + second.blocks("two"))


def test_json_data_requires_and_uses_explicit_variants():
    layout = mint.Layout.from_string(
        "data.toml",
        """
        [mint]
        endianness = "little"

        [config.header]
        start_address = 0x2000
        length = 0x10

        [config.data]
        value = { name = "Value", type = "u16" }
        """,
    )

    with pytest.raises(ValueError, match="variants are required"):
        mint.build(layout.blocks("config"), data={"Debug": {"Value": 7}})

    result = mint.build(
        layout.blocks("config"),
        data={"Debug": {"Value": 7}},
        variants=["Debug"],
    )

    assert result.ranges[0].data[:2] == b"\x07\x00"
    assert result.used_values["data.toml"]["config"]["value"] == 7


def test_render_srec_and_record_width_validation():
    layout = mint.Layout.from_file(str(LAYOUT))
    result = mint.build(layout.blocks("simple_block"))

    assert result.to_srec().startswith("S")
    with pytest.raises(ValueError, match="Record width must be between 1 and 128"):
        result.to_intel_hex(record_width=0)
