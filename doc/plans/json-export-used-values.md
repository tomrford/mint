# Plan: Export Used Values as JSON

## Overview

Add an optional feature to export a list of all values used during the build process as a JSON file. This enables building reports based on the selections made from data sources.

## Motivation

- **Traceability**: Know exactly which values were pulled from data sources for a given build
- **Reporting**: Generate reports showing configuration values embedded in firmware
- **Debugging**: Verify correct data source lookups and value mappings
- **Audit Trail**: Document what data went into each firmware build

## Proposed Design

### 1. Data Structure for Used Values

Add a new structure to track value usage in `src/commands/stats.rs`:

```rust
/// Represents a single value that was used during the build
#[derive(Debug, Clone, Serialize)]
pub struct UsedValue {
    /// The field path from the layout (e.g., "device.info.version")
    pub field_path: String,
    /// The name used to look up the value in the data source
    pub source_name: String,
    /// The resolved value(s)
    pub value: UsedValueData,
    /// The data type specified in the layout
    pub data_type: String,
    /// Which block this value belongs to
    pub block_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum UsedValueData {
    Single(serde_json::Value),
    Array1D(Vec<serde_json::Value>),
    Array2D(Vec<Vec<serde_json::Value>>),
}
```

### 2. Extend BuildStats

Add used values collection to the existing stats structure:

```rust
pub struct BuildStats {
    pub blocks_processed: usize,
    pub total_allocated: usize,
    pub total_used: usize,
    pub total_duration: Duration,
    pub block_stats: Vec<BlockStat>,
    pub used_values: Vec<UsedValue>,  // NEW: track all used values
}
```

### 3. JSON Export Structure

The exported JSON would have this format:

```json
{
  "build_info": {
    "timestamp": "2026-01-13T10:30:00Z",
    "layout_file": "firmware.toml",
    "data_source": "config.xlsx",
    "output_file": "out.hex"
  },
  "summary": {
    "blocks_processed": 3,
    "total_allocated_bytes": 4096,
    "total_used_bytes": 2048,
    "efficiency_percent": 50.0,
    "build_duration_ms": 150
  },
  "blocks": [
    {
      "name": "config",
      "start_address": "0x1000",
      "allocated_size": 1024,
      "used_size": 512,
      "crc_value": "0xABCD1234"
    }
  ],
  "used_values": [
    {
      "field_path": "device.id",
      "source_name": "DeviceID",
      "value": 4660,
      "data_type": "u32",
      "block_name": "config"
    },
    {
      "field_path": "device.name",
      "source_name": "DeviceName",
      "value": "SensorUnit",
      "data_type": "string",
      "block_name": "config"
    },
    {
      "field_path": "coefficients",
      "source_name": "CalCoeffs",
      "value": [1.0, 2.5, 3.7, 4.2],
      "data_type": "f32[]",
      "block_name": "calibration"
    }
  ]
}
```

### 4. CLI Interface

Add a new optional argument to `src/output/args.rs`:

```rust
pub struct OutputArgs {
    // ... existing fields ...

    /// Export build report with used values as JSON
    #[arg(long, value_name = "FILE")]
    pub export_json: Option<PathBuf>,
}
```

Usage:
```bash
mint build layout.toml --data config.xlsx --export-json report.json
```

## Implementation Steps

### Phase 1: Value Tracking Infrastructure

1. **Add data structures** (`src/commands/stats.rs`)
   - Add `UsedValue` and `UsedValueData` structs
   - Add `used_values: Vec<UsedValue>` to `BuildStats`
   - Derive `Serialize` for JSON output

2. **Create value collector** (`src/commands/value_collector.rs` - new file)
   - Implement a thread-safe collector for gathering used values
   - Use `Arc<Mutex<Vec<UsedValue>>>` or similar for parallel block processing

### Phase 2: Capture Values During Build

3. **Modify entry processing** (`src/layout/entry.rs`)
   - Update `emit_bytes()` and related functions to record used values
   - Pass collector reference through the call chain
   - Capture field path, source name, resolved value, and type

4. **Update block building** (`src/layout/block.rs`)
   - Thread the value collector through block resolution
   - Associate values with their block names

5. **Integrate with build command** (`src/commands/mod.rs`)
   - Create collector before parallel block processing
   - Merge collected values into `BuildStats`

### Phase 3: JSON Export

6. **Add CLI argument** (`src/output/args.rs`)
   - Add `--export-json <FILE>` option
   - Update argument parsing

7. **Implement JSON writer** (`src/visuals/mod.rs` or new `src/export/json.rs`)
   - Create `BuildReport` struct for full JSON structure
   - Implement serialization with proper formatting
   - Handle file writing with error handling

8. **Wire up export** (`src/commands/mod.rs`)
   - Check for `--export-json` flag after build completes
   - Generate and write JSON report

## Files to Modify

| File | Changes |
|------|---------|
| `src/commands/stats.rs` | Add `UsedValue`, extend `BuildStats` |
| `src/commands/mod.rs` | Integrate value collection, trigger export |
| `src/layout/entry.rs` | Record values during `emit_bytes()` |
| `src/layout/block.rs` | Thread collector through block building |
| `src/output/args.rs` | Add `--export-json` CLI option |
| `src/visuals/mod.rs` | Add JSON export function (or new file) |

## New Files

| File | Purpose |
|------|---------|
| `src/commands/value_collector.rs` | Thread-safe value collection |
| `src/export/mod.rs` (optional) | JSON export module |

## Considerations

### Thread Safety
- Block building happens in parallel via `par_iter()`
- Value collector must be thread-safe (`Arc<Mutex<_>>` or lock-free structure)
- Consider using `parking_lot::Mutex` for better performance

### Literal vs. Data Source Values
- Decide whether to include literal values defined in the layout
- Option: Add a `source_type: "literal" | "datasource"` field

### Value Representation
- Numeric values: preserve type info (u32 vs i64 vs f64)
- Strings: direct JSON strings
- Arrays: nested JSON arrays
- Consider hex formatting for addresses/IDs

### Optional Enhancements
- Filter by block name: `--export-json report.json --json-blocks config,calibration`
- Include/exclude literals: `--json-include-literals`
- Pretty vs. compact: `--json-compact`

## Testing

1. **Unit tests**: Value collector accumulation
2. **Integration tests**: End-to-end build with JSON export
3. **Example**: Add example in `doc/examples/` showing JSON output

## Example Usage

```bash
# Basic usage
mint build firmware.toml --data params.xlsx --export-json build-report.json

# With other options
mint build firmware.toml --data params.xlsx -o firmware.hex --stats --export-json report.json

# Quiet mode with just JSON export
mint build firmware.toml --data params.xlsx --quiet --export-json report.json
```

## Success Criteria

- [ ] JSON export contains all values retrieved from data sources
- [ ] Field paths match layout structure
- [ ] Values are correctly typed in JSON
- [ ] Thread-safe collection works with parallel block processing
- [ ] Export works with supported data source types (Excel, JSON)
- [ ] Documentation updated with new CLI option
