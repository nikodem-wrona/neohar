//! The changed part of a line that was edited: word level, and character level when the words give
//! nothing useful.

use std::ops::Range;

use similar::{Algorithm, DiffOp, capture_diff_slices};

/// A line longer than this gets no ranges. The diff of a minified file would be slow and useless.
const MAX_LINE: usize = 2000;

/// More than this part of a line changed: the whole line is different, and ranges tell nothing.
const WHOLE_LINE: f64 = 0.7;

/// The changed byte ranges of the two lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Intra {
    pub old: Vec<Range<usize>>,
    pub new: Vec<Range<usize>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Word,
    Space,
    Punct,
}

fn kind(ch: char) -> Kind {
    if ch.is_alphanumeric() || ch == '_' {
        Kind::Word
    } else if ch.is_whitespace() {
        Kind::Space
    } else {
        Kind::Punct
    }
}

/// Words, runs of white space, and single other characters, as byte ranges.
fn tokens(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        let class = kind(ch);
        let mut end = start + ch.len_utf8();
        if class != Kind::Punct {
            while let Some(&(at, next)) = chars.peek() {
                if kind(next) != class {
                    break;
                }
                end = at + next.len_utf8();
                chars.next();
            }
        }
        out.push(start..end);
    }
    out
}

/// Joins ranges that touch.
fn merge(ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        match out.last_mut() {
            Some(last) if last.end >= range.start => last.end = last.end.max(range.end),
            _ => out.push(range),
        }
    }
    out
}

fn changed_bytes(text: &str, ranges: &[Range<usize>]) -> usize {
    ranges
        .iter()
        .map(|range| {
            text[range.clone()]
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .map(char::len_utf8)
                .sum::<usize>()
        })
        .sum()
}

fn content_bytes(text: &str) -> usize {
    text.chars()
        .filter(|ch| !ch.is_whitespace())
        .map(char::len_utf8)
        .sum()
}

/// The ranges that differ between two lines, or `None` when they are equal, too long, or so
/// different that the whole line is the change.
pub(super) fn ranges(old: &str, new: &str) -> Option<Intra> {
    if old == new || old.len() > MAX_LINE || new.len() > MAX_LINE {
        return None;
    }
    let old_tokens = tokens(old);
    let new_tokens = tokens(new);
    let old_words: Vec<&str> = old_tokens.iter().map(|r| &old[r.clone()]).collect();
    let new_words: Vec<&str> = new_tokens.iter().map(|r| &new[r.clone()]).collect();
    let ops = capture_diff_slices(Algorithm::Myers, &old_words, &new_words);
    let mut found = Intra {
        old: Vec::new(),
        new: Vec::new(),
    };
    let mut any_equal = false;
    for op in &ops {
        match *op {
            DiffOp::Equal { .. } => any_equal = true,
            DiffOp::Delete {
                old_index, old_len, ..
            } => found
                .old
                .push(old_tokens[old_index].start..old_tokens[old_index + old_len - 1].end),
            DiffOp::Insert {
                new_index, new_len, ..
            } => found
                .new
                .push(new_tokens[new_index].start..new_tokens[new_index + new_len - 1].end),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                found
                    .old
                    .push(old_tokens[old_index].start..old_tokens[old_index + old_len - 1].end);
                found
                    .new
                    .push(new_tokens[new_index].start..new_tokens[new_index + new_len - 1].end);
            }
        }
    }
    if !any_equal {
        found = characters(old, new);
    }
    found.old = merge(found.old);
    found.new = merge(found.new);
    let old_part = changed_bytes(old, &found.old) as f64 / content_bytes(old).max(1) as f64;
    let new_part = changed_bytes(new, &found.new) as f64 / content_bytes(new).max(1) as f64;
    if old_part > WHOLE_LINE && new_part > WHOLE_LINE {
        return None;
    }
    Some(found)
}

/// The part between the common start and the common end of the two lines.
fn characters(old: &str, new: &str) -> Intra {
    let prefix: usize = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    let suffix: usize = old[prefix..]
        .chars()
        .rev()
        .zip(new[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    let old_range = prefix..old.len() - suffix;
    let new_range = prefix..new.len() - suffix;
    Intra {
        old: if old_range.is_empty() {
            vec![]
        } else {
            vec![old_range]
        },
        new: if new_range.is_empty() {
            vec![]
        } else {
            vec![new_range]
        },
    }
}

#[cfg(test)]
mod tests;
