//! A unified patch (`diff -u`, `git diff`) as files with hunks.
//!
//! The patch comes from a program that neohar does not control, so the parser reads what it can and
//! never fails. Text it does not understand is skipped.

use super::{FileStatus, LineKind};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct PatchFile {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub status: Option<FileStatus>,
    pub binary: bool,
    pub hunks: Vec<PatchHunk>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PatchHunk {
    /// The line that starts the hunk, `@@ -1,3 +1,4 @@ fn name`.
    pub header: String,
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<(LineKind, String)>,
}

/// The numbers of a hunk header: the start and the length of each side.
fn hunk_numbers(header: &str) -> Option<(u32, u32, u32, u32)> {
    let rest = header.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let (new, _) = rest.split_once(" @@").unwrap_or((rest, ""));
    let pair = |text: &str| -> Option<(u32, u32)> {
        match text.split_once(',') {
            Some((start, len)) => Some((start.trim().parse().ok()?, len.trim().parse().ok()?)),
            None => Some((text.trim().parse().ok()?, 1)),
        }
    };
    let (old_start, old_len) = pair(old)?;
    let (new_start, new_len) = pair(new)?;
    Some((old_start, old_len, new_start, new_len))
}

/// A path of a file header: no time stamp, no `a/` or `b/` prefix, `/dev/null` is no path.
fn header_path(text: &str) -> Option<String> {
    let text = text.split('\t').next().unwrap_or(text).trim();
    let text = text.trim_matches('"');
    if text.is_empty() || text == "/dev/null" {
        return None;
    }
    let text = text
        .strip_prefix("a/")
        .or_else(|| text.strip_prefix("b/"))
        .unwrap_or(text);
    Some(text.to_owned())
}

/// The new path of `diff --git a/x b/y`.
fn git_header_path(rest: &str) -> Option<String> {
    let (_, new) = rest.rsplit_once(" b/")?;
    Some(new.to_owned())
}

/// The body of the hunk that is being read.
struct Body {
    /// The lines that the header promised on each side. `None` when the header had no numbers.
    left: Option<(u32, u32)>,
}

impl Body {
    /// Takes the line when it belongs to the hunk. `false` means the hunk ended before this line.
    fn take(&mut self, line: &str, hunk: &mut PatchHunk) -> bool {
        // The marker "\ No newline at end of file" follows the line that it is about.
        if line.starts_with('\\') {
            return true;
        }
        if self.left == Some((0, 0)) {
            return false;
        }
        let (kind, text) = match line.as_bytes().first() {
            Some(b' ') => (LineKind::Ctx, &line[1..]),
            Some(b'+') => (LineKind::Add, &line[1..]),
            Some(b'-') => (LineKind::Del, &line[1..]),
            // Some tools strip the space of an empty context line.
            None if self.left.is_some() => (LineKind::Ctx, ""),
            _ => return false,
        };
        if let Some((old, new)) = self.left.as_mut() {
            match kind {
                LineKind::Ctx => {
                    *old = old.saturating_sub(1);
                    *new = new.saturating_sub(1);
                }
                LineKind::Add => *new = new.saturating_sub(1),
                LineKind::Del => *old = old.saturating_sub(1),
            }
        }
        hunk.lines.push((kind, text.to_owned()));
        true
    }
}

/// The files of a patch. A patch with hunks and no file header gives one file with no path.
pub(super) fn parse(text: &str) -> Vec<PatchFile> {
    let mut files: Vec<PatchFile> = Vec::new();
    let mut body: Option<Body> = None;
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(open) = body.as_mut() {
            let hunk = files.last_mut().and_then(|file| file.hunks.last_mut());
            if hunk.is_some_and(|hunk| open.take(line, hunk)) {
                continue;
            }
            body = None;
        }
        if let Some(rest) = line.strip_prefix("diff --git ") {
            files.push(PatchFile {
                new_path: git_header_path(rest),
                ..PatchFile::default()
            });
        } else if let Some(rest) = line.strip_prefix("--- ") {
            // In the header of a file that has no hunk yet, the line belongs to that file.
            if files.last().is_none_or(|file| !file.hunks.is_empty()) {
                files.push(PatchFile::default());
            }
            if let Some(file) = files.last_mut() {
                file.old_path = header_path(rest);
                if file.old_path.is_none() {
                    file.status = Some(FileStatus::Added);
                }
            }
        } else if let Some(rest) = line.strip_prefix("+++ ") {
            if files.is_empty() {
                files.push(PatchFile::default());
            }
            if let Some(file) = files.last_mut() {
                file.new_path = header_path(rest);
                if file.new_path.is_none() {
                    file.status = Some(FileStatus::Deleted);
                }
            }
        } else if line.starts_with("new file mode") {
            set_status(&mut files, FileStatus::Added);
        } else if line.starts_with("deleted file mode") {
            set_status(&mut files, FileStatus::Deleted);
        } else if let Some(rest) = line.strip_prefix("rename from ") {
            if let Some(file) = files.last_mut() {
                file.old_path = Some(rest.to_owned());
                file.status = Some(FileStatus::Renamed);
            }
        } else if let Some(rest) = line.strip_prefix("rename to ") {
            if let Some(file) = files.last_mut() {
                file.new_path = Some(rest.to_owned());
                file.status = Some(FileStatus::Renamed);
            }
        } else if line.starts_with("Binary files ") || line.starts_with("GIT binary patch") {
            if let Some(file) = files.last_mut() {
                file.binary = true;
            }
        } else if line.starts_with("@@") {
            let numbers = hunk_numbers(line);
            if files.is_empty() {
                files.push(PatchFile::default());
            }
            if let Some(file) = files.last_mut() {
                file.hunks.push(PatchHunk {
                    header: line.to_owned(),
                    old_start: numbers.map_or(1, |n| n.0),
                    new_start: numbers.map_or(1, |n| n.2),
                    lines: Vec::new(),
                });
            }
            body = Some(Body {
                left: numbers.map(|n| (n.1, n.3)),
            });
        }
    }
    files
}

fn set_status(files: &mut [PatchFile], status: FileStatus) {
    if let Some(file) = files.last_mut() {
        file.status = Some(status);
    }
}

#[cfg(test)]
mod tests;
