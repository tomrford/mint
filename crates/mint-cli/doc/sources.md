# Data sources

`name` fields and bitmap sub-fields with a `name` read their values from one data source, given with `--xlsx` or `--json` together with `--variants`. A layout without `name` lookups builds without a source.

## Variants

`--variants Debug/Default` looks each name up in `Debug` first and falls back to `Default`. Every listed variant must exist in the source. In Excel, empty and whitespace-only cells fall through; in JSON, missing keys and `null` values fall through. A name with no value in any listed variant is an error. Names match exactly and are case-sensitive.

## Excel

```bash
mint build layout.toml --xlsx data.xlsx --variants Debug/Default
```

The main sheet, `Main` unless `--main-sheet` names another, has a header row with a `Name` column and one column per variant. Names and variant headers must be unique.

| Name | Default | Debug |
| --- | --- | --- |
| DeviceName | MyDevice | DebugDevice |
| Version | 1 | 2 |
| EnableDebug | 0 | 1 |
| Coefficients | #DefaultCoefficients | #DebugCoefficients |
| Matrix | #CalibrationMatrix | |

Scalar fields need numeric or boolean cells and string fields need text cells. An array field's cell holds `#` followed by the name of a sheet with the values below a header row:

- a one-dimensional array reads the first column down to the first empty cell;
- a two-dimensional array takes its width from the non-empty header cells and reads rows until one is entirely empty; a partly empty row is an error.

| C1 | C2 |
| --- | --- |
| 10 | 20 |
| 30 | 40 |

## JSON

```bash
mint build layout.toml --json data.json --variants Debug/Default
mint build layout.toml --json '{"Default":{"Version":1}}' --variants Default
```

`--json` takes a file path, or inline JSON when the trimmed value starts with `{`. The top-level object is keyed by variant, and each variant is an object of names and values: numbers or booleans for scalars, strings, arrays for one-dimensional fields and arrays of arrays for two-dimensional ones.

```json
{
  "Default": {
    "DeviceName": "MyDevice",
    "Version": 1,
    "EnableDebug": 0,
    "Coefficients": [1.0, 2.5, 3.7, 4.2],
    "Matrix": [[10, 20], [30, 40]]
  },
  "Debug": {
    "DeviceName": "DebugDevice",
    "Version": 2,
    "EnableDebug": 1
  }
}
```

JSON suits pipelines that fetch or transform data before calling mint.
