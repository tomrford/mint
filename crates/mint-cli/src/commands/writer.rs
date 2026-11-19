use mint_core::output::error::OutputError;
use same_file::Handle;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf, is_separator};

pub fn inspect<'a>(
    inputs: impl Iterator<Item = &'a Path>,
    outputs: impl Iterator<Item = &'a Path>,
) -> Result<Vec<PathBuf>, OutputError> {
    let inputs = inputs.collect::<BTreeSet<_>>();
    let mut checked: Vec<PathBuf> = Vec::new();
    for output in outputs {
        let path = destination(output).map_err(|error| file_error("resolve", output, error))?;
        let id = Handle::from_path(&path).ok();
        if let Some(other) = inputs
            .iter()
            .copied()
            .chain(checked.iter().map(PathBuf::as_path))
            .find(|other| path == *other || id.is_some() && id == Handle::from_path(other).ok())
        {
            return Err(OutputError::FileError(format!(
                "output '{}' overlaps input or output '{}'",
                output.display(),
                other.display()
            )));
        }
        checked.push(path);
    }
    Ok(checked)
}

fn destination(path: &Path) -> io::Result<PathBuf> {
    let spelling = path.as_os_str().as_encoded_bytes();
    let end = spelling.strip_suffix(b".").unwrap_or(spelling).last();
    let path = std::path::absolute(path)?;
    for ancestor in path.ancestors() {
        let mut target = match ancestor.canonicalize() {
            Ok(target) => target,
            Err(error) if error.kind() == io::ErrorKind::NotFound && !ancestor.is_symlink() => {
                continue;
            }
            Err(error) => return Err(error),
        };
        if ancestor == path && target.is_dir() || end.is_some_and(|&byte| is_separator(byte.into()))
        {
            return Err(io::ErrorKind::IsADirectory.into());
        }
        let mut cancelled = false;
        for part in path.components().skip(ancestor.components().count()) {
            if part == Component::ParentDir {
                target.pop();
                cancelled = true;
            } else {
                target.push(part);
            }
        }
        return if cancelled {
            destination(&target)
        } else {
            Ok(target)
        };
    }
    Err(io::ErrorKind::NotFound.into())
}

pub fn write(path: &Path, contents: &str) -> Result<(), OutputError> {
    fs::create_dir_all(path.parent().unwrap_or(Path::new("")))
        .and_then(|()| fs::write(path, contents))
        .map_err(|error| file_error("write", path, error))
}

fn file_error(action: &str, path: &Path, error: io::Error) -> OutputError {
    OutputError::FileError(format!("failed to {action} {}: {error}", path.display()))
}
