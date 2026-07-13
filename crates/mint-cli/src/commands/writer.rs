use mint_core::output::OutputFile;
use mint_core::output::error::OutputError;
use std::path::Path;

use crate::output_args::OutputArgs;

/// Write a single output file to the path specified in args.
pub fn write_output(file: &OutputFile, args: &OutputArgs) -> Result<(), OutputError> {
    let contents = file.render()?;
    write_text(&args.out, &contents)
}

pub fn write_text(path: &Path, contents: &str) -> Result<(), OutputError> {
    // Create parent directory if it doesn't exist
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| {
            OutputError::FileError(format!(
                "failed to create directory {}: {}",
                parent.display(),
                e
            ))
        })?;
    }

    std::fs::write(path, contents).map_err(|e| {
        OutputError::FileError(format!("failed to write {}: {}", path.display(), e))
    })?;
    Ok(())
}
