use std::path::Path;

use serde_json::Value;

use crate::output::error::OutputError;

/// Write used values JSON report to disk.
pub fn write_used_values_json(path: &Path, report: &Value) -> Result<(), OutputError> {
    let contents = serde_json::to_string_pretty(report)
        .map_err(|e| OutputError::FileError(format!("failed to serialize JSON report: {}", e)))?;

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| {
            OutputError::FileError(format!(
                "failed to create report directory {}: {}",
                parent.display(),
                e
            ))
        })?;
    }

    std::fs::write(path, contents).map_err(|e| {
        OutputError::FileError(format!(
            "failed to write JSON report {}: {}",
            path.display(),
            e
        ))
    })?;

    Ok(())
}
