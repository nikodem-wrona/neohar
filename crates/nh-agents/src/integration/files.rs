//! The files of an installer: read, back up, write atomically.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use super::{
    Error, Result, diff,
    hooks::{self, EventState, Spec},
};

/// What an installer did to a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Created,
    Modified,
    Removed,
    Unchanged,
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub path: PathBuf,
    pub action: Action,
    /// The copy of the file that was made before the first change.
    pub backup: Option<PathBuf>,
    /// The change as a unified diff. It is there also for a dry run.
    pub diff: String,
}

/// The content of a file, or `None` when there is none.
fn read(path: &Path) -> Result<Option<String>> {
    match fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| Error::Format(format!("{} is not UTF-8 text", path.display()))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::Io(format!(
            "cannot read {}: {error}",
            path.display()
        ))),
    }
}

fn seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Writes `new` to `path` through a temporary file in the same directory. The write stops when the
/// file is not what `expected` says (`None` means no file), because someone changed it after it was
/// read.
pub fn write_atomic(path: &Path, new: &str, expected: Option<&str>) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let temp = dir.join(format!(
        ".{name}.neohar-{}-{}.tmp",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos())
    ));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(new.as_bytes())?;
        file.sync_all()?;
        if let Ok(meta) = fs::metadata(path) {
            // The file keeps its mode.
            let _ = fs::set_permissions(&temp, meta.permissions());
        }
        if read(path)?.as_deref() != expected {
            return Err(Error::Changed(path.display().to_string()));
        }
        fs::rename(&temp, path)?;
        // The rename is durable when the directory is. A failure here is not worth an error.
        if let Ok(dir) = File::open(dir) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Copies a file to `<file>.neohar-bak-<seconds>`. A copy with that name that exists stays.
fn backup(path: &Path) -> Result<PathBuf> {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".neohar-bak-{}", seconds()));
    let target = PathBuf::from(name);
    if !target.exists() {
        fs::copy(path, &target)?;
    }
    Ok(target)
}

/// Applies a new text to a file: the backup, the write, and the outcome.
fn apply(path: &Path, old: Option<&str>, new: &str, dry_run: bool) -> Result<Outcome> {
    let shown = path.display().to_string();
    let change = diff::unified(
        &old.map_or_else(|| "/dev/null".to_owned(), |_| shown.clone()),
        &shown,
        old.unwrap_or(""),
        new,
    );
    if old == Some(new) {
        return Ok(Outcome {
            path: path.to_owned(),
            action: Action::Unchanged,
            backup: None,
            diff: String::new(),
        });
    }
    let action = if old.is_some() {
        Action::Modified
    } else {
        Action::Created
    };
    if dry_run {
        return Ok(Outcome {
            path: path.to_owned(),
            action,
            backup: None,
            diff: change,
        });
    }
    let backup = if old.is_some() {
        Some(backup(path)?)
    } else {
        None
    };
    write_atomic(path, new, old)?;
    Ok(Outcome {
        path: path.to_owned(),
        action,
        backup,
        diff: change,
    })
}

/// The text of a file, or `None` when there is none. It reads and never writes.
pub fn read_text(path: &Path) -> Result<Option<String>> {
    read(path)
}

/// Puts the hooks of neohar in the settings of a harness at `path`.
pub fn install_hooks(spec: &Spec, path: &Path, bin: &str, dry_run: bool) -> Result<Outcome> {
    let old = read(path)?;
    let new = hooks::install(spec, old.as_deref(), bin)?;
    apply(path, old.as_deref(), &new, dry_run)
}

/// Takes the hooks of neohar out of the settings at `path`.
pub fn uninstall_hooks(spec: &Spec, path: &Path, dry_run: bool) -> Result<Outcome> {
    let Some(old) = read(path)? else {
        return Ok(Outcome {
            path: path.to_owned(),
            action: Action::Unchanged,
            backup: None,
            diff: String::new(),
        });
    };
    let new = hooks::uninstall(spec, &old)?;
    apply(path, Some(&old), &new, dry_run)
}

/// How the hooks stand in the settings at `path`.
pub fn inspect_hooks(spec: &Spec, path: &Path, bin: &str) -> Result<Vec<EventState>> {
    hooks::inspect(spec, read(path)?.as_deref(), bin)
}

#[cfg(test)]
mod tests;
