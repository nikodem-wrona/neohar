//! The shims that neohar puts first in the `PATH` of every pane.
//!
//! Only `codex` has one. The core writes it to `<data>/shims/codex` at each start, byte for byte
//! the same, and only when the file differs, so a running pane never sees a half-written file.
//!
//! A login zsh puts other directories in front of the shims (`path_helper` and the startup files of
//! the user). A zsh in a pane thus starts with the `ZDOTDIR` `<data>/shims/.zdotdir`, whose
//! `.zshenv` reads the files of the user and puts the shims first again before each prompt.

use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

/// The shim of Codex.
pub const CODEX: &str = include_str!("shims/codex.sh");

/// The `.zshenv` of the `ZDOTDIR` of a zsh in a pane.
pub const ZSHENV: &str = include_str!("shims/zshenv.zsh");

/// The directory of the shims inside the data directory of a session.
pub fn dir(data: &Path) -> PathBuf {
    data.join("shims")
}

/// The `ZDOTDIR` of a zsh in a pane. A dot keeps it out of the names that the `PATH` finds.
pub fn zdotdir(shims_dir: &Path) -> PathBuf {
    shims_dir.join(".zdotdir")
}

/// Writes the shims into `shims_dir`, and the `.zshenv` into [`zdotdir`].
pub fn ensure(shims_dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(shims_dir)?;
    write(&shims_dir.join("codex"), CODEX, 0o755)?;
    let zdotdir = zdotdir(shims_dir);
    fs::create_dir_all(&zdotdir)?;
    write(&zdotdir.join(".zshenv"), ZSHENV, 0o644)
}

/// Writes `text` to `path` with `mode`, when the file differs.
fn write(path: &Path, text: &str, mode: u32) -> std::io::Result<()> {
    let current = fs::read_to_string(path).ok();
    let mode_ok = fs::metadata(path).is_ok_and(|meta| meta.permissions().mode() & 0o777 == mode);
    if current.as_deref() == Some(text) && mode_ok {
        return Ok(());
    }
    // A temporary file and a rename: a pane that runs the file reads a whole file.
    let name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let temp = path.with_file_name(format!(".{name}.neohar-tmp"));
    let mut file = fs::File::create(&temp)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    fs::set_permissions(&temp, fs::Permissions::from_mode(mode))?;
    fs::rename(&temp, path)
}

#[cfg(test)]
mod tests;
