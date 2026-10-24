//! Diffs for the approval cards.
//!
//! A diff has three sources: the `old_string` and `new_string` of an edit tool, the whole content
//! of a file that a tool writes, and a unified patch (the `fileChange` of Codex). All three give
//! the same [`FileDiff`]: hunks with 3 lines of context, a language from the path, highlighted
//! lines, and the changed part of each line that was edited.

mod intra;
mod patch;

use std::{ops::Range, time::Duration};

use serde::Serialize;
use similar::{Algorithm, DiffOp, DiffTag, TextDiff};

use crate::{
    highlight::{self, Language, LineHighlighter, Span, Spans, without_line_end},
    text::{Utf16Map, cap},
};

/// The most lines of one file that a diff holds. The rest is `truncated`. [`diff_capped`] and
/// [`between_capped`] take another limit.
pub const MAX_LINES: usize = 2000;
/// The lines of context around a change.
pub const CONTEXT: usize = 3;
/// Text larger than this is cut before the diff, because the time of a diff grows with the size.
const INPUT_CAP: usize = 2 * 1024 * 1024;
/// The longest time that one diff may take. After it, the diff is correct but not the smallest.
const TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Ctx,
    Add,
    Del,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffLine {
    pub kind: LineKind,
    /// The line number in the old file. A context line and a deleted line have one, when the
    /// diff knows the numbers.
    pub old: Option<u32>,
    /// The line number in the new file.
    pub new: Option<u32>,
    pub text: Spans,
    /// The changed ranges inside the line, in UTF-16 code units of `text`.
    pub intra: Option<Vec<(u32, u32)>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Stats {
    pub add: u32,
    pub del: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileDiff {
    pub path: String,
    pub status: FileStatus,
    pub hunks: Vec<Hunk>,
    pub stats: Stats,
    /// The diff of this file was cut at [`MAX_LINES`].
    pub truncated: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Diff {
    pub files: Vec<FileDiff>,
}

/// One replacement of an edit tool.
#[derive(Debug, Clone, Copy)]
pub struct Edit<'a> {
    pub old: &'a str,
    pub new: &'a str,
    /// Every place of `old` changes, not only the one place.
    pub replace_all: bool,
}

/// A change that a tool asks to make.
#[derive(Debug, Clone, Copy)]
pub enum Change<'a> {
    /// `Edit` and `MultiEdit`. With the content of the file (`file`), the diff is of the whole
    /// file, with the real line numbers and the lines around each change. Without it, or when an
    /// old text is not in the file in the place that the edit says, the diff is of the texts of
    /// the edits, and the lines have no numbers.
    Edits {
        path: &'a str,
        edits: &'a [Edit<'a>],
        file: Option<&'a str>,
    },
    /// `Write`: the whole content of a file. `before` is the content now, when the file exists.
    Write {
        path: &'a str,
        before: Option<&'a str>,
        after: &'a str,
    },
    /// A file that goes away. `before` is its content, when it is known.
    Remove {
        path: &'a str,
        before: Option<&'a str>,
    },
    /// A unified patch of one file or many. `path` and `status` fill what the patch does not say.
    Patch {
        path: Option<&'a str>,
        status: Option<FileStatus>,
        text: &'a str,
    },
}

/// The diff of the changes, one entry per file in the order given.
pub fn diff(changes: &[Change<'_>]) -> Diff {
    diff_capped(changes, MAX_LINES)
}

/// [`diff`] with another limit of the lines of one file.
pub fn diff_capped(changes: &[Change<'_>], limit: usize) -> Diff {
    let mut files = Vec::new();
    for change in changes {
        match *change {
            Change::Edits { path, edits, file } => {
                files.push(edits_diff(path, edits, file, limit));
            }
            Change::Write {
                path,
                before,
                after,
            } => {
                let status = if before.is_some() {
                    FileStatus::Modified
                } else {
                    FileStatus::Added
                };
                files.push(texts_diff(
                    path,
                    status,
                    before.unwrap_or(""),
                    after,
                    Some(0),
                    limit,
                ));
            }
            Change::Remove { path, before } => {
                files.push(texts_diff(
                    path,
                    FileStatus::Deleted,
                    before.unwrap_or(""),
                    "",
                    Some(0),
                    limit,
                ));
            }
            Change::Patch { path, status, text } => {
                files.extend(patch_diff(path, status, text, limit));
            }
        }
    }
    Diff { files }
}

/// The diff of two texts, for `rich.diff`. `lang` is the name of a language; `path` gives the
/// language when `lang` does not.
pub fn between(old: &str, new: &str, path: Option<&str>, lang: Option<&str>) -> Diff {
    between_capped(old, new, path, lang, MAX_LINES)
}

/// [`between`] with another limit of the lines of the file.
pub fn between_capped(
    old: &str,
    new: &str,
    path: Option<&str>,
    lang: Option<&str>,
    limit: usize,
) -> Diff {
    let language = lang
        .and_then(highlight::language)
        .or_else(|| path.and_then(highlight::language_of_path));
    Diff {
        files: vec![build(
            path.unwrap_or(""),
            FileStatus::Modified,
            old,
            new,
            Some(0),
            language,
            limit,
        )],
    }
}

/// The whole diff as unified text, with no cap. The core keeps it as a blob when a file in the card
/// is `truncated`, and "open full" shows it.
pub fn unified(changes: &[Change<'_>]) -> String {
    let mut out = String::new();
    for change in changes {
        match *change {
            Change::Edits { path, edits, .. } => {
                for edit in edits {
                    out.push_str(&unified_text(path, edit.old, edit.new));
                }
            }
            Change::Write {
                path,
                before,
                after,
            } => {
                out.push_str(&unified_text(path, before.unwrap_or(""), after));
            }
            Change::Remove { path, before } => {
                out.push_str(&unified_text(path, before.unwrap_or(""), ""));
            }
            Change::Patch { text, .. } => {
                out.push_str(text);
                if !text.ends_with('\n') {
                    out.push('\n');
                }
            }
        }
    }
    out
}

fn unified_text(path: &str, old: &str, new: &str) -> String {
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Myers)
        .timeout(TIMEOUT)
        .diff_lines(old, new);
    diff.unified_diff()
        .context_radius(CONTEXT)
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string()
}

fn language_for(path: &str) -> Option<Language> {
    highlight::language_of_path(path)
}

fn edits_diff(path: &str, edits: &[Edit<'_>], file: Option<&str>, limit: usize) -> FileDiff {
    if let Some(file) = file
        && let Some(after) = apply_edits(file, edits)
    {
        return texts_diff(path, FileStatus::Modified, file, &after, Some(0), limit);
    }
    let language = language_for(path);
    let mut out = FileDiff {
        path: path.to_owned(),
        status: FileStatus::Modified,
        hunks: Vec::new(),
        stats: Stats::default(),
        truncated: false,
    };
    let mut lines_left = limit;
    for edit in edits {
        let part = build(
            path,
            FileStatus::Modified,
            edit.old,
            edit.new,
            None,
            language,
            limit,
        );
        out.stats.add += part.stats.add;
        out.stats.del += part.stats.del;
        out.truncated |= part.truncated;
        for mut hunk in part.hunks {
            if lines_left == 0 {
                out.truncated = true;
                break;
            }
            if hunk.lines.len() > lines_left {
                hunk.lines.truncate(lines_left);
                out.truncated = true;
            }
            lines_left -= hunk.lines.len();
            out.hunks.push(hunk);
        }
    }
    out
}

/// The file after the edits, in order, as the tool applies them. `None` when an edit does not find
/// its old text once (or at all, for `replace_all`): the tool would fail, or the place is unclear.
fn apply_edits(file: &str, edits: &[Edit<'_>]) -> Option<String> {
    let mut text = file.to_owned();
    for edit in edits {
        if edit.old.is_empty() {
            return None;
        }
        let count = text.matches(edit.old).count();
        if count == 0 || (count > 1 && !edit.replace_all) {
            return None;
        }
        text = if edit.replace_all {
            text.replace(edit.old, edit.new)
        } else {
            text.replacen(edit.old, edit.new, 1)
        };
    }
    Some(text)
}

fn texts_diff(
    path: &str,
    status: FileStatus,
    old: &str,
    new: &str,
    offset: Option<u32>,
    limit: usize,
) -> FileDiff {
    build(path, status, old, new, offset, language_for(path), limit)
}

fn has_nul(text: &str) -> bool {
    text.as_bytes().contains(&0)
}

/// A diff of two texts. `offset` is the number of lines before the texts in their files, or `None`
/// when the numbers are not known.
fn build(
    path: &str,
    status: FileStatus,
    old: &str,
    new: &str,
    offset: Option<u32>,
    language: Option<Language>,
    limit: usize,
) -> FileDiff {
    let mut out = FileDiff {
        path: path.to_owned(),
        status,
        hunks: Vec::new(),
        stats: Stats::default(),
        truncated: false,
    };
    if has_nul(old) || has_nul(new) {
        out.hunks.push(Hunk {
            header: "Binary file".to_owned(),
            lines: Vec::new(),
        });
        return out;
    }
    let old = cap(old, INPUT_CAP);
    let new = cap(new, INPUT_CAP);
    out.truncated = old.truncated || new.truncated;
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Myers)
        .timeout(TIMEOUT)
        .diff_lines(old.text, new.text);
    for op in diff.ops() {
        match op.as_tag_tuple() {
            (DiffTag::Delete, old_range, _) => out.stats.del += count(&old_range),
            (DiffTag::Insert, _, new_range) => out.stats.add += count(&new_range),
            (DiffTag::Replace, old_range, new_range) => {
                out.stats.del += count(&old_range);
                out.stats.add += count(&new_range);
            }
            (DiffTag::Equal, _, _) => {}
        }
    }
    let groups = diff.grouped_ops(CONTEXT);
    let old_lines: Vec<&str> = diff.iter_old_slices().collect();
    let new_lines: Vec<&str> = diff.iter_new_slices().collect();
    // The highlight needs every line before the last one that a hunk shows.
    let (mut old_hl, mut new_hl) = (Lines::new(language), Lines::new(language));
    let mut shown = 0usize;
    for group in &groups {
        let mut hunk = Hunk {
            header: header(group, offset),
            lines: Vec::new(),
        };
        for op in group {
            // One line more than the room left shows that lines were cut.
            let room = limit - shown + 1;
            let lines = op_lines(
                op,
                &old_lines,
                &new_lines,
                offset,
                room,
                &mut old_hl,
                &mut new_hl,
            );
            let take = lines.len().min(limit - shown);
            if take < lines.len() {
                out.truncated = true;
            }
            shown += take;
            hunk.lines.extend(lines.into_iter().take(take));
            if out.truncated {
                break;
            }
        }
        out.hunks.push(hunk);
        if out.truncated {
            break;
        }
    }
    out
}

fn count(range: &Range<usize>) -> u32 {
    u32::try_from(range.len()).unwrap_or(u32::MAX)
}

/// `@@ -1,3 +1,4 @@` for the lines of a group. Without line numbers the header is `@@`.
fn header(group: &[DiffOp], offset: Option<u32>) -> String {
    let (Some(offset), Some(first), Some(last)) = (offset, group.first(), group.last()) else {
        return "@@".to_owned();
    };
    let old = first.old_range().start..last.old_range().end;
    let new = first.new_range().start..last.new_range().end;
    let start = |range: &Range<usize>| {
        // An empty range names the line before it, as `diff` does.
        let first = if range.is_empty() {
            range.start
        } else {
            range.start + 1
        };
        u64::from(offset) + first as u64
    };
    format!(
        "@@ -{},{} +{},{} @@",
        start(&old),
        old.len(),
        start(&new),
        new.len()
    )
}

/// The highlighted lines of one side of a diff, read in order.
struct Lines {
    highlighter: Option<LineHighlighter>,
    done: Vec<Vec<Span>>,
    /// Bytes read, to stop at [`highlight::HIGHLIGHT_CAP`].
    read: usize,
}

impl Lines {
    fn new(language: Option<Language>) -> Self {
        Self {
            highlighter: language.map(LineHighlighter::new),
            done: Vec::new(),
            read: 0,
        }
    }

    /// Reads the lines up to `index`, which a highlight of a later line needs.
    fn ensure(&mut self, all: &[&str], index: usize) {
        let Some(highlighter) = self.highlighter.as_mut() else {
            return;
        };
        while self.done.len() <= index {
            let Some(line) = all.get(self.done.len()) else {
                return;
            };
            if self.read >= highlight::HIGHLIGHT_CAP {
                self.done.push(Vec::new());
                continue;
            }
            self.read += line.len();
            self.done.push(highlighter.line(without_line_end(line)));
        }
    }

    /// The spans of line `index`.
    fn spans(&mut self, all: &[&str], index: usize) -> Vec<Span> {
        self.ensure(all, index);
        self.done.get(index).cloned().unwrap_or_default()
    }
}

/// The lines of one operation of a group, at most `room` of them.
fn op_lines(
    op: &DiffOp,
    old_lines: &[&str],
    new_lines: &[&str],
    offset: Option<u32>,
    room: usize,
    old_hl: &mut Lines,
    new_hl: &mut Lines,
) -> Vec<DiffLine> {
    let number = |index: usize| -> Option<u32> {
        let index = u32::try_from(index).ok()?;
        Some(offset? + index + 1)
    };
    let clip = |range: Range<usize>| range.start..range.end.min(range.start + room);
    let mut out = Vec::new();
    match op.as_tag_tuple() {
        (DiffTag::Equal, old_range, new_range) => {
            for (old_index, new_index) in clip(old_range).zip(clip(new_range)) {
                // Context is on both sides, so the old side reads it too.
                old_hl.ensure(old_lines, old_index);
                out.push(DiffLine {
                    kind: LineKind::Ctx,
                    old: number(old_index),
                    new: number(new_index),
                    text: spans_of(new_lines[new_index], new_hl.spans(new_lines, new_index)),
                    intra: None,
                });
            }
        }
        (DiffTag::Delete, old_range, _) => {
            for index in clip(old_range) {
                out.push(DiffLine {
                    kind: LineKind::Del,
                    old: number(index),
                    new: None,
                    text: spans_of(old_lines[index], old_hl.spans(old_lines, index)),
                    intra: None,
                });
            }
        }
        (DiffTag::Insert, _, new_range) => {
            for index in clip(new_range) {
                out.push(DiffLine {
                    kind: LineKind::Add,
                    old: None,
                    new: number(index),
                    text: spans_of(new_lines[index], new_hl.spans(new_lines, index)),
                    intra: None,
                });
            }
        }
        (DiffTag::Replace, old_range, new_range) => {
            let mut dels: Vec<DiffLine> = clip(old_range)
                .map(|index| DiffLine {
                    kind: LineKind::Del,
                    old: number(index),
                    new: None,
                    text: spans_of(old_lines[index], old_hl.spans(old_lines, index)),
                    intra: None,
                })
                .collect();
            let mut adds: Vec<DiffLine> = clip(new_range)
                .map(|index| DiffLine {
                    kind: LineKind::Add,
                    old: None,
                    new: number(index),
                    text: spans_of(new_lines[index], new_hl.spans(new_lines, index)),
                    intra: None,
                })
                .collect();
            for (del, add) in dels.iter_mut().zip(adds.iter_mut()) {
                mark_intra(del, add);
            }
            out.extend(dels);
            out.extend(adds);
        }
    }
    out
}

fn spans_of(line: &str, spans: Vec<Span>) -> Spans {
    Spans {
        text: without_line_end(line).to_owned(),
        spans,
    }
}

/// Fills `intra` of a deleted line and the added line that replaces it.
fn mark_intra(del: &mut DiffLine, add: &mut DiffLine) {
    let Some(found) = intra::ranges(&del.text.text, &add.text.text) else {
        return;
    };
    let convert = |text: &str, ranges: &[Range<usize>]| -> Option<Vec<(u32, u32)>> {
        if ranges.is_empty() {
            return None;
        }
        let map = Utf16Map::new(text);
        Some(
            ranges
                .iter()
                .map(|range| (map.at(range.start), map.at(range.end)))
                .collect(),
        )
    };
    del.intra = convert(&del.text.text, &found.old);
    add.intra = convert(&add.text.text, &found.new);
}

/// The files of a patch as [`FileDiff`]s.
fn patch_diff(
    path: Option<&str>,
    status: Option<FileStatus>,
    text: &str,
    limit: usize,
) -> Vec<FileDiff> {
    patch::parse(text)
        .into_iter()
        .map(|file| patch_file(path, status, file, limit))
        .collect()
}

fn patch_file(
    path: Option<&str>,
    status: Option<FileStatus>,
    file: patch::PatchFile,
    limit: usize,
) -> FileDiff {
    let name = file
        .new_path
        .clone()
        .or_else(|| file.old_path.clone())
        .or_else(|| path.map(str::to_owned))
        .unwrap_or_default();
    let status = status.or(file.status).unwrap_or(FileStatus::Modified);
    let language = language_for(&name);
    let mut out = FileDiff {
        path: name,
        status,
        hunks: Vec::new(),
        stats: Stats::default(),
        truncated: false,
    };
    if file.binary && file.hunks.is_empty() {
        out.hunks.push(Hunk {
            header: "Binary file".to_owned(),
            lines: Vec::new(),
        });
        return out;
    }
    let mut shown = 0usize;
    for hunk in file.hunks {
        // The totals count every line, also those that the cap cuts.
        for (kind, _) in &hunk.lines {
            match kind {
                LineKind::Add => out.stats.add += 1,
                LineKind::Del => out.stats.del += 1,
                LineKind::Ctx => {}
            }
        }
        if shown >= limit {
            out.truncated = true;
            continue;
        }
        let room = limit - shown;
        if hunk.lines.len() > room {
            out.truncated = true;
        }
        let mut lines = patch_lines(&hunk, language, room);
        pair_changes(&mut lines);
        shown += lines.len();
        out.hunks.push(Hunk {
            header: hunk.header,
            lines,
        });
    }
    out
}

/// The lines of a hunk with numbers and highlight, at most `room` of them.
fn patch_lines(hunk: &patch::PatchHunk, language: Option<Language>, room: usize) -> Vec<DiffLine> {
    let mut old_hl = language.map(LineHighlighter::new);
    let mut new_hl = language.map(LineHighlighter::new);
    let read = |highlighter: &mut Option<LineHighlighter>, content: &str| -> Vec<Span> {
        highlighter
            .as_mut()
            .map(|highlighter| highlighter.line(content))
            .unwrap_or_default()
    };
    let (mut old_no, mut new_no) = (hunk.old_start, hunk.new_start);
    let mut out = Vec::new();
    for (kind, content) in hunk.lines.iter().take(room) {
        let (old, new, spans) = match kind {
            LineKind::Ctx => {
                read(&mut old_hl, content);
                (Some(old_no), Some(new_no), read(&mut new_hl, content))
            }
            LineKind::Del => (Some(old_no), None, read(&mut old_hl, content)),
            LineKind::Add => (None, Some(new_no), read(&mut new_hl, content)),
        };
        if *kind != LineKind::Add {
            old_no += 1;
        }
        if *kind != LineKind::Del {
            new_no += 1;
        }
        out.push(DiffLine {
            kind: *kind,
            old,
            new,
            text: Spans {
                text: content.clone(),
                spans,
            },
            intra: None,
        });
    }
    out
}

/// A run of deleted lines followed by a run of added lines: the first of each pair belong together.
fn pair_changes(lines: &mut [DiffLine]) {
    let mut at = 0;
    while at < lines.len() {
        if lines[at].kind != LineKind::Del {
            at += 1;
            continue;
        }
        let dels_end = lines[at..]
            .iter()
            .position(|line| line.kind != LineKind::Del)
            .map_or(lines.len(), |n| at + n);
        let adds_end = lines[dels_end..]
            .iter()
            .position(|line| line.kind != LineKind::Add)
            .map_or(lines.len(), |n| dels_end + n);
        let pairs = (dels_end - at).min(adds_end - dels_end);
        for n in 0..pairs {
            let (left, right) = lines.split_at_mut(dels_end);
            mark_intra(&mut left[at + n], &mut right[n]);
        }
        at = adds_end.max(at + 1);
    }
}

#[cfg(test)]
mod tests;
