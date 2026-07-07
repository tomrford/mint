from pathlib import Path

import pytest

import mint


ROOT = Path(__file__).resolve().parents[3]
LAYOUT = ROOT / "crates" / "mint-core" / "tests" / "data" / "blocks.toml"


def test_file_layout_builds_selected_blocks_and_exports_used_values():
    layout = mint.Layout.from_file(str(LAYOUT))

    result = mint.build(layout.blocks("simple_block"))

    assert result.stats.blocks_processed == 1
    assert result.stats.block_stats[0].layout == str(LAYOUT)
    assert result.stats.block_stats[0].block == "simple_block"
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
    assert {(stat.layout, stat.block) for stat in result.stats.block_stats} == {
        ("generated.toml", "one"),
        ("generated.toml", "two"),
    }
    assert set(result.used_values["generated.toml"]) == {"one", "two"}

    none_result = mint.build(layout.blocks(None))
    assert none_result.stats.blocks_processed == 2


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
    assert [(stat.layout, stat.block) for stat in result.stats.block_stats] == [
        ("generated.toml", "two"),
        ("generated.toml", "one"),
    ]

    list_result = mint.build(layout.blocks(["two", "one"]))
    assert [(stat.layout, stat.block) for stat in list_result.stats.block_stats] == [
        ("generated.toml", "two"),
        ("generated.toml", "one"),
    ]


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


def test_core_build_errors_raise_mint_error():
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
        """,
    )

    with pytest.raises(mint.MintError, match="block not found"):
        mint.build(layout.blocks("missing"))


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

    with pytest.raises(ValueError, match="variants require data"):
        mint.build(layout.blocks("config"), variants=["Debug"])

    result = mint.build(
        layout.blocks("config"),
        data={"Debug": {"Value": 7}},
        variants=["Debug"],
    )

    assert result.ranges[0].data[:2] == b"\x07\x00"
    assert result.used_values["data.toml"]["config"]["value"] == 7


def test_python_data_rejects_non_json_values_before_build():
    layout = mint.Layout.from_string(
        "data.toml",
        """
        [mint]
        endianness = "little"

        [config.header]
        start_address = 0x2000
        length = 0x10

        [config.data]
        value = { name = "Value", type = "u64" }
        """,
    )
    blocks = layout.blocks("config")

    with pytest.raises(ValueError, match="integer values must fit"):
        mint.build(
            blocks,
            data={"Default": {"Value": (1 << 64) + 3}},
            variants=["Default"],
            strict=True,
        )

    with pytest.raises(ValueError, match="floating-point values must be finite"):
        mint.build(
            blocks,
            data={"Default": {"Value": float("inf")}},
            variants=["Default"],
        )

    with pytest.raises(ValueError, match="unsupported data value of type 'bytes'"):
        mint.build(
            blocks,
            data={"Default": {"Value": b"abc"}},
            variants=["Default"],
        )

    with pytest.raises(ValueError, match="data dictionaries must use string keys"):
        mint.build(blocks, data={1: {"Value": 7}}, variants=["Default"])


def test_render_srec_and_record_width_validation():
    layout = mint.Layout.from_file(str(LAYOUT))
    result = mint.build(layout.blocks("simple_block"))

    assert result.to_srec().startswith("S")
    with pytest.raises(mint.MintError, match="Record width must be between 1 and 128"):
        result.to_intel_hex(record_width=0)
