# Data Sources

mint supports two data source types: Excel workbooks and raw JSON. A source is not strictly necessary; if a layout contains only values it will build without one. You cannot use more than one source in a single build.

## Excel (`-x, --xlsx`)

```bash
mint layout.toml --xlsx data.xlsx -v Default
```

### Main Sheet Structure

The `Main` sheet (or one specified via `--main-sheet`) contains variant data:

| Name        | Default              | Debug               |
| ----------- | -------------------- | ------------------- |
| DeviceName  | MyDevice             | DebugDevice         |
| Version     | 1                    | 2                   |
| Counter     | 1000                 | 0                   |
| EnableDebug | 0                    | 1                   |
| RegionCode  | 5                    | 12                  |
| Coefficients| #DefaultCoefficients | #DebugCoefficients  |
| Matrix      | #CalibrationMatrix   | #CalibrationMatrix  |

- **Name column**: lookup key used by layout files
- **Variant columns**: values for each variant (e.g., `Default`, `Debug`)
- **Precedence**: follows the explicit `-v` order; first non-empty value wins
- **Sheet references**: cells starting with `#` reference array sheets (e.g., `#DefaultCoefficients`)

### Array Sheets

For 1D/2D arrays, reference a sheet by name with `#` prefix:

| C1  | C2  | C3  |
| --- | --- | --- |
| 1   | 2   | 3   |
| 4   | 5   | 6   |
| 7   | 8   | 9   |

- First row ignored as headers (and defines width for 2D arrays)
- Values read row-by-row until an empty cell is encountered
- Strings and undersized arrays are padded by default; use `SIZE` (uppercase) in layout to enforce strict length

---

## JSON (`-j, --json`)

```bash
mint layout.toml --json data.json -v Debug/Default
# or inline:
mint layout.toml --json '{"Default":{"DeviceName":"MyDevice","Version":1,"Counter":1000},"Debug":{"DeviceName":"DebugDevice","Version":2}}' -v Debug/Default
```

### Format

The JSON data source expects an object where:

- **Top-level keys** are variant names (e.g., `"Default"`, `"Debug"`, `"Production"`)
- **Each variant's value** is an object containing name:value pairs

```json
{
  "Default": {
    "DeviceName": "MyDevice",
    "Version": 1,
    "Counter": 1000,
    "EnableDebug": 0,
    "RegionCode": 5,
    "Coefficients": [1.0, 2.5, 3.7, 4.2],
    "Matrix": [[10, 20], [30, 40]]
  },
  "Debug": {
    "DeviceName": "DebugDevice",
    "Version": 2,
    "Counter": 0,
    "EnableDebug": 1
  }
}
```

Use this when your build pipeline already fetches or transforms data before invoking mint. Generate the version-object JSON in your script, then pass it to mint as a file or inline string.

### Value Types

- **Scalars**: numbers, booleans, strings
- **1D Arrays**: native JSON arrays (e.g., `[1, 2, 3]`)
- **2D Arrays**: arrays of arrays (native JSON only)

### Variant Priority

Values are resolved using the variant priority order specified by `-v`. The first non-empty value found wins.
