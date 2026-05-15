use mint_core::output::OutputFile;
use mint_core::output::error::OutputError;

use crate::output_args::OutputArgs;

/// Write a single output file to the path specified in args.
pub fn write_output(file: &OutputFile, args: &OutputArgs) -> Result<(), OutputError> {
    let contents = file.render()?;

    // Create parent directory if it doesn't exist
    if let Some(parent) = args.out.parent()
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

    std::fs::write(&args.out, contents).map_err(|e| {
        OutputError::FileError(format!("failed to write {}: {}", args.out.display(), e))
    })?;
    Ok(())
}
