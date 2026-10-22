//! Proof that a recording leaves the real configuration of the user alone.
//!
//! The sandbox (`sandbox.rs`) makes the real `~/.claude`, `~/.claude.json` and `~/.codex` closed
//! to the harness. This module is the check that does not trust the sandbox: it takes checksums of
//! the configuration before and after a recording and looks for traces of the recording.
//!
//! Two kinds of path are watched:
//!
//! - **Settings** (`STABLE_PATHS`): files and directories that only the user changes. A hook
//!   installed into the real `settings.json` or `hooks.json` would show here. The checksums must
//!   be equal.
//! - **Everything else** under the real directories. A running Claude Code or Codex (maybe the one
//!   that you work in) writes transcripts, history and databases there all the time, so checksums
//!   would fail for the wrong reason. Instead the check looks at the paths that are **new** after
//!   the recording, and finds the **marker**, the name of the scratch root (`nhg-`), in their name
//!   or their content. A harness that ran unconfined would start a project directory
//!   (`projects/-private-tmp-nhg-claude-work/`) or a session file with its working directory in
//!   it. A running session that appends to an old file does not count, so that your own session
//!   cannot fail the check. One that starts a new file with the marker in it during a recording
//!   can, and the next run passes.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use crate::{GoldenError, recipe::Harness};

/// The text that every scratch root of a recording contains.
pub const MARKER: &str = "nhg-";

/// Settings that only the user changes, relative to the home directory, for each harness.
pub fn stable_paths(harness: Harness) -> &'static [&'static str] {
    match harness {
        Harness::Claude => &[
            ".claude/settings.json",
            ".claude/settings.local.json",
            ".claude/CLAUDE.md",
            ".claude/keybindings.json",
            ".claude/commands",
            ".claude/agents",
            ".claude/skills",
            ".claude/output-styles",
        ],
        Harness::Codex => &[
            ".codex/config.toml",
            ".codex/hooks.json",
            ".codex/auth.json",
            ".codex/AGENTS.md",
            ".codex/instructions.md",
            ".codex/rules",
            ".codex/prompts",
            ".codex/skills",
        ],
    }
}

/// The directories and files that hold the state of a harness, relative to the home directory.
/// The check looks at their top level only: `~/.codex/worktrees` alone can hold 300,000 paths.
pub fn state_roots(harness: Harness) -> &'static [&'static str] {
    match harness {
        Harness::Claude => &[".claude", ".claude.json"],
        Harness::Codex => &[".codex"],
    }
}

/// The directories where an unconfined harness would leave a new file for its session, relative to
/// the home directory. The check looks at everything below them.
pub fn session_dirs(harness: Harness) -> &'static [&'static str] {
    match harness {
        Harness::Claude => &[
            ".claude/projects",
            ".claude/sessions",
            ".claude/session-env",
            ".claude/shell-snapshots",
            ".claude/todos",
            ".claude/backups",
        ],
        Harness::Codex => &[
            ".codex/sessions",
            ".codex/archived_sessions",
            ".codex/shell_snapshots",
            ".codex/log",
            ".codex/memories",
        ],
    }
}

/// The checksums of the settings of a harness: path (relative to `home`) to a description of the
/// content. A path that does not exist is not in the map, so that "did not exist" is a state too.
pub type Checksums = BTreeMap<String, String>;

fn describe_file(path: &Path) -> String {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            format!("link to {:?}", fs::read_link(path).unwrap_or_default())
        }
        Ok(meta) if meta.is_file() => match fs::read(path) {
            Ok(bytes) => format!(
                "{} bytes, blake3 {}",
                bytes.len(),
                crate::meta::blake3_hex(&bytes)
            ),
            Err(e) => format!("unreadable: {e}"),
        },
        Ok(_) => "other".to_owned(),
        Err(e) => format!("unreadable: {e}"),
    }
}

fn walk(path: &Path, home: &Path, into: &mut Checksums) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return;
    };
    let relative = path
        .strip_prefix(home)
        .unwrap_or(path)
        .display()
        .to_string();
    if meta.is_dir() {
        into.insert(relative, "directory".to_owned());
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        let mut names: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
        names.sort();
        for child in names {
            walk(&child, home, into);
        }
    } else {
        into.insert(relative, describe_file(path));
    }
}

/// Takes the checksums of the settings of `harness` under `home`.
pub fn checksums(harness: Harness, home: &Path) -> Checksums {
    let mut sums = Checksums::new();
    for path in stable_paths(harness) {
        walk(&home.join(path), home, &mut sums);
    }
    sums
}

/// What changed between two sets of checksums, one line for each path.
pub fn changes(before: &Checksums, after: &Checksums) -> Vec<String> {
    let mut lines = Vec::new();
    for (path, was) in before {
        match after.get(path) {
            None => lines.push(format!("{path}: removed (was {was})")),
            Some(now) if now != was => lines.push(format!("{path}: changed from {was} to {now}")),
            Some(_) => {}
        }
    }
    for path in after.keys().filter(|p| !before.contains_key(*p)) {
        lines.push(format!("{path}: created"));
    }
    lines
}

/// What the real directories held at one time.
#[derive(Clone, Debug)]
pub struct Before {
    sums: Checksums,
    /// Every path under the state directories, relative to the home directory.
    listing: BTreeSet<String>,
}

/// Adds `path` and what is below it, down to `depth` levels. `usize::MAX` has no limit.
fn list(path: &Path, home: &Path, depth: usize, into: &mut BTreeSet<String>) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return;
    };
    into.insert(
        path.strip_prefix(home)
            .unwrap_or(path)
            .display()
            .to_string(),
    );
    if meta.is_dir()
        && depth > 0
        && let Ok(entries) = fs::read_dir(path)
    {
        for entry in entries.filter_map(Result::ok) {
            list(&entry.path(), home, depth.saturating_sub(1), into);
        }
    }
}

/// The paths that the check watches for new entries.
fn listing(harness: Harness, home: &Path) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for root in state_roots(harness) {
        list(&home.join(root), home, 1, &mut paths);
    }
    for dir in session_dirs(harness) {
        list(&home.join(dir), home, usize::MAX, &mut paths);
    }
    paths
}

/// Takes the state of the real directories of `harness` under `home`.
pub fn take(harness: Harness, home: &Path) -> Before {
    Before {
        sums: checksums(harness, home),
        listing: listing(harness, home),
    }
}

/// Finds the paths that are new since `before` and hold the marker.
pub fn traces(harness: Harness, home: &Path, before: &Before) -> Vec<String> {
    let now = listing(harness, home);
    let mut found = Vec::new();
    for relative in now.difference(&before.listing) {
        let path = home.join(relative);
        if relative.contains(MARKER) {
            found.push(format!("{relative}: the name holds {MARKER:?}"));
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_file() && meta.len() <= 64 << 20 {
            let mut text = Vec::new();
            if fs::File::open(&path)
                .and_then(|mut f| f.read_to_end(&mut text))
                .is_ok()
                && text.windows(MARKER.len()).any(|w| w == MARKER.as_bytes())
            {
                found.push(format!("{relative}: the content holds {MARKER:?}"));
            }
        }
    }
    found
}

/// Fails if a recording changed the real settings or left a trace. `before` was taken before it.
pub fn verify(harness: Harness, home: &Path, before: &Before) -> Result<(), GoldenError> {
    let mut problems = changes(&before.sums, &checksums(harness, home));
    problems.extend(traces(harness, home, before));
    if problems.is_empty() {
        Ok(())
    } else {
        Err(GoldenError::Setup(format!(
            "the recording touched the real configuration of {}:\n{}",
            harness.name(),
            problems.join("\n")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home(name: &str) -> PathBuf {
        let home = std::env::temp_dir().join(format!("nhg-guard-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(home.join(".claude/commands")).expect("dirs");
        fs::write(home.join(".claude/settings.json"), "{\"a\":1}").expect("file");
        fs::write(home.join(".claude/commands/x.md"), "x").expect("file");
        home
    }

    #[test]
    fn unchanged_settings_pass_and_a_changed_file_is_named() {
        let home = temp_home("a");
        let before = take(Harness::Claude, &home);
        // settings.json, commands and commands/x.md
        assert_eq!(before.sums.len(), 3, "{:?}", before.sums);
        verify(Harness::Claude, &home, &before).expect("nothing changed");
        // The live state of a running harness is not a setting, and an old file that grows is not
        // a new file.
        fs::write(home.join(".claude/history.jsonl"), "{}").expect("file");
        let before_history = take(Harness::Claude, &home);
        fs::write(
            home.join(".claude/history.jsonl"),
            "{\"project\":\"nhg-x\"}",
        )
        .expect("file");
        verify(Harness::Claude, &home, &before_history).expect("an old file may grow");
        fs::write(home.join(".claude/settings.json"), "{\"hooks\":{}}").expect("file");
        let error = verify(Harness::Claude, &home, &before).expect_err("a changed setting");
        assert!(
            error.to_string().contains(".claude/settings.json: changed"),
            "{error}"
        );
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn a_created_or_removed_setting_is_found() {
        let home = temp_home("b");
        let before = checksums(Harness::Claude, &home);
        fs::write(home.join(".claude/settings.local.json"), "{}").expect("file");
        fs::remove_file(home.join(".claude/commands/x.md")).expect("remove");
        let lines = changes(&before, &checksums(Harness::Claude, &home));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("settings.local.json: created")),
            "{lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("x.md: removed")),
            "{lines:?}"
        );
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn a_new_path_with_the_marker_is_a_trace_in_its_name_and_in_its_content() {
        let home = temp_home("c");
        let before = take(Harness::Claude, &home);
        assert!(traces(Harness::Claude, &home, &before).is_empty());
        fs::create_dir_all(home.join(".claude/projects/-private-tmp-nhg-claude-work"))
            .expect("dir");
        fs::write(
            home.join(".claude/session.jsonl"),
            "{\"cwd\":\"/private/tmp/nhg-claude/work\"}",
        )
        .expect("file");
        fs::write(home.join(".claude/unrelated.jsonl"), "{}").expect("file");
        let found = traces(Harness::Claude, &home, &before);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().any(|l| l.contains("the name holds")));
        assert!(
            found
                .iter()
                .any(|l| l.contains("session.jsonl: the content holds"))
        );
        let _ = fs::remove_dir_all(&home);
    }
}
