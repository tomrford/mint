use mint_core::output::error::OutputError;
use same_file::Handle;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf, is_separator};

pub struct Destination<'a> {
    original: &'a Path,
    resolved: PathBuf,
}

impl Destination<'_> {
    pub fn write(&self, contents: &str) -> Result<(), OutputError> {
        if let Some(parent) = self
            .resolved
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .map_err(|error| file_error("write", self.original, error))?;
        }
        fs::write(&self.resolved, contents)
            .map_err(|error| file_error("write", self.original, error))
    }
}

pub fn inspect<'a>(
    inputs: impl Iterator<Item = &'a Path>,
    outputs: impl Iterator<Item = &'a Path>,
) -> Result<Vec<Destination<'a>>, OutputError> {
    let inputs = inputs.collect::<BTreeSet<_>>();
    let mut input_handles: Option<Vec<(&Path, Option<Handle>)>> = None;
    let mut checked: Vec<(Destination<'a>, Option<Handle>)> = Vec::new();
    for output in outputs {
        let resolved = destination(output).map_err(|error| file_error("resolve", output, error))?;
        let id = Handle::from_path(&resolved).ok();
        let overlapping_input = inputs
            .iter()
            .copied()
            .find(|other| resolved == *other)
            .or_else(|| {
                let id = id.as_ref()?;
                let handles = input_handles.get_or_insert_with(|| {
                    inputs
                        .iter()
                        .copied()
                        .map(|path| (path, Handle::from_path(path).ok()))
                        .collect()
                });
                handles
                    .iter()
                    .find(|(_, handle)| handle.as_ref() == Some(id))
                    .map(|(path, _)| *path)
            });
        let overlapping = overlapping_input.or_else(|| {
            checked
                .iter()
                .find(|(other, handle)| resolved == other.resolved || id.is_some() && id == *handle)
                .map(|(other, _)| other.original)
        });
        if let Some(other) = overlapping {
            return Err(OutputError::FileError(format!(
                "output '{}' overlaps input or output '{}'",
                output.display(),
                other.display()
            )));
        }
        checked.push((
            Destination {
                original: output,
                resolved,
            },
            id,
        ));
    }
    Ok(checked.into_iter().map(|(dest, _)| dest).collect())
}

fn destination(path: &Path) -> io::Result<PathBuf> {
    let spelling = path.as_os_str().as_encoded_bytes();
    let end = spelling.strip_suffix(b".").unwrap_or(spelling).last();
    let path = std::path::absolute(path)?;
    if path.file_name().is_none() || end.is_some_and(|&byte| is_separator(byte.into())) {
        return Err(io::ErrorKind::IsADirectory.into());
    }
    for ancestor in path.ancestors() {
        let mut target = match ancestor.canonicalize() {
            Ok(target) => target,
            Err(error) if error.kind() == io::ErrorKind::NotFound && !ancestor.is_symlink() => {
                continue;
            }
            Err(error) => return Err(error),
        };
        if ancestor == path {
            return if target.is_dir() {
                Err(io::ErrorKind::IsADirectory.into())
            } else {
                Ok(target)
            };
        }
        if !target.is_dir() {
            return Err(io::ErrorKind::NotADirectory.into());
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

fn file_error(action: &str, path: &Path, error: io::Error) -> OutputError {
    OutputError::FileError(format!("failed to {action} {}: {error}", path.display()))
}
