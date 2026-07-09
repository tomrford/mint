from pathlib import Path

import pytest

import mint


ROOT = Path(__file__).resolve().parents[3]
LAYOUT = ROOT / "crates" / "mint-core" / "tests" / "data" / "blocks.toml"
PYTHON_CRATE = Path(__file__).resolve().parents[1]
XLSX_FIXTURE = PYTHON_CRATE.parent / "mint-core" / "tests" / "data" / "data.xlsx"


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


def test_xlsx_path_builds_with_named_fixture_value():
    layout = mint.Layout.from_string(
        "xlsx.toml",
        """
        [mint]
        endianness = "little"

        [config.header]
        start_address = 0x2000
        length = 0x10

        [config.data]
        temperature_max = { name = "TemperatureMax", type = "u8" }
        """,
    )

    result = mint.build(
        layout.blocks("config"),
        xlsx_path=str(XLSX_FIXTURE),
        variants=["Default"],
    )

    assert result.ranges[0].data[:1] == b"\x32"


def test_json_path_builds_from_file(tmp_path):
    json_path = tmp_path / "data.json"
    json_path.write_text('{"Default":{"Value":513}}')
    layout = mint.Layout.from_string(
        "json-path.toml",
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

    result = mint.build(
        layout.blocks("config"),
        json_path=str(json_path),
        variants=["Default"],
    )

    assert result.ranges[0].data[:2] == b"\x01\x02"


def test_strict_rejects_lossy_python_data_conversion():
    layout = mint.Layout.from_string(
        "strict.toml",
        """
        [mint]
        endianness = "little"

        [config.header]
        start_address = 0x2000
        length = 0x10

        [config.data]
        value = { name = "Value", type = "u8" }
        """,
    )

    with pytest.raises(mint.MintError):
        mint.build(
            layout.blocks("config"),
            data={"Default": {"Value": 300}},
            variants=["Default"],
            strict=True,
        )


def test_python_data_builds_2d_array():
    layout = mint.Layout.from_string(
        "array.toml",
        """
        [mint]
        endianness = "little"

        [config.header]
        start_address = 0x2000
        length = 0x10

        [config.data]
        matrix = { name = "Matrix", type = "u16", size = [2, 2] }
        """,
    )

    result = mint.build(
        layout.blocks("config"),
        data={"Default": {"Matrix": [[1, 2], [3, 4]]}},
        variants=["Default"],
    )

    assert result.ranges[0].data[:8] == b"\x01\x00\x02\x00\x03\x00\x04\x00"


def test_main_sheet_requires_xlsx_path():
    layout = mint.Layout.from_string(
        "main-sheet.toml",
        """
        [mint]
        endianness = "little"

        [config.header]
        start_address = 0x2000
        length = 0x10

        [config.data]
        value = { name = "Value", type = "u8" }
        """,
    )

    with pytest.raises(ValueError, match="main_sheet requires xlsx_path"):
        mint.build(
            layout.blocks("config"),
            data={"Default": {"Value": 1}},
            variants=["Default"],
            main_sheet="Config",
        )


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
