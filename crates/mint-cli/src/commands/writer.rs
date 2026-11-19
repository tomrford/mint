use mint_core::output::error::OutputError;
use same_file::Handle;
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf, is_separator};

pub struct Destination<'a> {
    original: &'a Path,
    resolved: PathBuf,
    existed: bool,
}

impl Destination<'_> {
    pub fn write(&self, contents: &str) -> Result<(), OutputError> {
        let write = || -> io::Result<()> {
            if let Some(parent) = self
                .resolved
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                fs::create_dir_all(parent)?;
            }
            if self.existed {
                fs::write(&self.resolved, contents)
            } else {
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&self.resolved)?
                    .write_all(contents.as_bytes())
            }
        };
        write().map_err(|error| file_error("write", self.original, error))
    }
}

pub fn inspect<'a>(
    inputs: impl Iterator<Item = &'a Path>,
    outputs: impl Iterator<Item = &'a Path>,
) -> Result<Vec<Destination<'a>>, OutputError> {
    let inputs: BTreeSet<_> = inputs.collect();
    let mut checked: Vec<(Destination<'a>, Option<Handle>)> = Vec::new();
    for output in outputs {
        let resolved = destination(output).map_err(|error| file_error("resolve", output, error))?;
        let id = Handle::from_path(&resolved).ok();
        let overlap = inputs
            .iter()
            .copied()
            .find(|other| resolved == *other || id.is_some() && id == Handle::from_path(other).ok())
            .or_else(|| {
                checked
                    .iter()
                    .find(|(other, handle)| {
                        resolved.starts_with(&other.resolved)
                            || other.resolved.starts_with(&resolved)
                            || id.is_some() && id == *handle
                    })
                    .map(|(other, _)| other.original)
            });
        if let Some(other) = overlap {
            return Err(OutputError::FileError(format!(
                "output '{}' overlaps input or output '{}'",
                output.display(),
                other.display()
            )));
        }
        let dest = Destination {
            original: output,
            resolved,
            existed: id.is_some(),
        };
        checked.push((dest, id));
    }
    Ok(checked.into_iter().map(|(dest, _)| dest).collect())
}

fn destination(path: &Path) -> io::Result<PathBuf> {
    let spelling = path.as_os_str().as_encoded_bytes();
    if spelling.is_empty() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let end = spelling.strip_suffix(b".").unwrap_or(spelling).last();
    if path.file_name().is_none() || end.is_some_and(|&byte| is_separator(byte.into())) {
        return Err(io::ErrorKind::IsADirectory.into());
    }

    let mut target = if path.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir()?.canonicalize()?
    };
    let mut missing = 0usize;
    let mut components = path.components().peekable();
    while let Some(part) = components.next() {
        let is_last = components.peek().is_none();
        match part {
            Component::Prefix(_) => {
                target.push(part);
                if !matches!(components.peek(), Some(Component::RootDir)) {
                    target = target.canonicalize()?;
                }
                missing = 0;
            }
            Component::RootDir => {
                target.push(part);
                target = target.canonicalize()?;
                missing = 0;
            }
            Component::CurDir => {}
            Component::ParentDir if missing > 0 => {
                missing -= 1;
                target.pop();
            }
            Component::ParentDir => {
                target.push(part);
                target = target.canonicalize()?;
            }
            Component::Normal(name) => {
                target.push(name);
                if missing > 0 {
                    missing += 1;
                    continue;
                }
                match target.canonicalize() {
                    Ok(real) => {
                        target = real;
                        if is_last && target.is_dir() {
                            return Err(io::ErrorKind::IsADirectory.into());
                        }
                        if !is_last && !target.is_dir() {
                            return Err(io::ErrorKind::NotADirectory.into());
                        }
                    }
                    Err(error)
                        if error.kind() == io::ErrorKind::NotFound && !target.is_symlink() =>
                    {
                        missing = 1;
                    }
                    Err(error) => return Err(error),
                }
            }
        }
    }
    Ok(target)
}

fn file_error(action: &str, path: &Path, error: io::Error) -> OutputError {
    OutputError::FileError(format!("failed to {action} {}: {error}", path.display()))
}
