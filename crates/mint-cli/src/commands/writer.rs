use mint_core::output::error::OutputError;
use std::path::{Path, PathBuf};

/// Validate the complete set of destinations before writing any file.
pub fn write_files(inputs: &[&Path], outputs: &[(&Path, &str)]) -> Result<(), OutputError> {
    let destinations = outputs
        .iter()
        .map(|(path, _)| destination(path))
        .collect::<Result<Vec<_>, _>>()?;

    for (index, path) in destinations.iter().enumerate() {
        for other in inputs
            .iter()
            .copied()
            .chain(destinations[..index].iter().map(PathBuf::as_path))
        {
            if path == other || same_file::is_same_file(path, other).unwrap_or(false) {
                return Err(OutputError::FileError(format!(
                    "output '{}' overlaps input or output '{}'",
                    outputs[index].0.display(),
                    other.display()
                )));
            }
        }
    }

    for (path, contents) in outputs {
        std::fs::write(path, contents).map_err(|error| {
            OutputError::FileError(format!("failed to write {}: {error}", path.display()))
        })?;
    }
    Ok(())
}

fn destination(path: &Path) -> Result<PathBuf, OutputError> {
    let resolve = || -> std::io::Result<PathBuf> {
        let path = std::path::absolute(path)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        match path.canonicalize() {
            Ok(path) => Ok(path),
            // Reject dangling symlinks; only a genuinely new filename uses the parent fallback.
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && path.symlink_metadata().is_err() =>
            {
                match (path.parent(), path.file_name()) {
                    (Some(parent), Some(name)) => Ok(parent.canonicalize()?.join(name)),
                    _ => Err(error),
                }
            }
            Err(error) => Err(error),
        }
    };
    resolve().map_err(|error| {
        OutputError::FileError(format!(
            "failed to resolve output {}: {error}",
            path.display()
        ))
    })
}
