//! The screen detector: the rules of a manifest, run on a screen.
//!
//! The detector is pure. The core copies the rows that [`Detector::rows_needed`] names out of the
//! engine of a pane, at most ten times a second, and calls [`Detector::evaluate`]. A rule with
//! `hold_ms` counts only after it stayed true for that long, so a spinner that blinks does not
//! flip the state. The detector gives every rule that holds, and the arbiter picks.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nh_proto::api::model::InteractionKind;
use regex::Regex;

use super::{
    dsl::{
        Accelerators, CommandFrom, DialogRule, Normalize, RowRange, Rule, ScreenKind,
        ScreenManifest, ScreenState, StyleMatcher,
    },
    view::{Cell, Screen},
};
use nh_proto::style::{Attrs, Color};

/// How often the core may call [`Detector::evaluate`] for a pane.
pub const MAX_RATE_HZ: u32 = 10;

/// The text of the hint for a dialog that no rule reads.
pub const UNKNOWN_DIALOG_HINT: &str = "neohar can't read this dialog";

/// The id of the match that the heuristic for unknown dialogs makes.
pub const UNKNOWN_DIALOG_RULE: &str = "unknown_dialog";

/// One option of a dialog on the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogOption {
    /// The number that the screen shows, when it shows one.
    pub number: Option<u32>,
    pub label: String,
    /// The row of the screen.
    pub row: u16,
    /// The pointer is on this option.
    pub pointed: bool,
    /// A checkbox of a list with many choices, when the screen shows one.
    pub checked: Option<bool>,
}

/// The geometry of a dialog, for the keystroke verifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dialog {
    pub options: Vec<DialogOption>,
    /// The index in `options` of the option that the pointer is on.
    pub pointer: Option<usize>,
    pub accelerators: bool,
    /// The number of the option for `allow`, `allow_always` and `deny`, from the manifest.
    pub map: BTreeMap<String, u32>,
    /// The index in `options` of the option that the labels name: `allow`, `allow_always`,
    /// `deny`.
    pub choices: BTreeMap<String, usize>,
    pub multi_toggle: Option<String>,
    /// The rows that the options cover.
    pub rows: (u16, u16),
    /// The title that the rule read: the question of the dialog, or the header of a question.
    pub title: Option<String>,
    /// What the dialog asks for, as the screen shows it: the command, or the text in the box above
    /// the question. The rows of the screen are joined with `\n`. The keystroke verifier compares
    /// it with the request of the interaction before it sends a key.
    pub command: Option<String>,
}

/// What a rule says about the screen.
#[derive(Clone, Debug, PartialEq)]
pub struct ScreenMatch {
    pub rule_id: String,
    pub state: Option<ScreenState>,
    pub opens: Option<InteractionKind>,
    pub confidence: f32,
    /// What the rule read: the named groups of its patterns, `title` and `command`.
    pub captures: BTreeMap<String, String>,
    pub dialog: Option<Dialog>,
    /// A keystroke may answer the dialog. The heuristic for unknown dialogs never allows it.
    pub keystroke_answerable: bool,
}

/// The result of one evaluation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Evaluation {
    pub matches: Vec<ScreenMatch>,
    /// A rule is true and its `hold_ms` is not over. Evaluate again after this time.
    pub recheck_after: Option<Duration>,
}

impl Evaluation {
    /// The match of a rule.
    pub fn rule(&self, id: &str) -> Option<&ScreenMatch> {
        self.matches.iter().find(|m| m.rule_id == id)
    }

    /// The state that the screen shows, from the rule with the highest confidence.
    pub fn state(&self) -> Option<(ScreenState, f32)> {
        self.matches
            .iter()
            .filter_map(|m| m.state.map(|s| (s, m.confidence)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// The dialog that the screen shows, from the rule with the highest confidence.
    pub fn dialog(&self) -> Option<&ScreenMatch> {
        self.matches
            .iter()
            .filter(|m| m.opens.is_some())
            .max_by(|a, b| a.confidence.total_cmp(&b.confidence))
    }
}

/// The rules of a manifest and the time that each has been true.
pub struct Detector {
    manifest: Arc<ScreenManifest>,
    /// Since when each rule has been true without a break.
    since: Vec<Option<Instant>>,
}

impl Detector {
    pub fn new(manifest: Arc<ScreenManifest>) -> Self {
        let since = vec![None; manifest.rules.len()];
        Self { manifest, since }
    }

    pub fn manifest(&self) -> &ScreenManifest {
        &self.manifest
    }

    /// The rows that the rules read on a screen with `total` rows. The core copies only these.
    /// The unknown-dialog heuristic reads the bottom 40 rows too.
    pub fn rows_needed(&self, total: u16) -> RangeInclusive<u16> {
        let mut from = total.saturating_sub(40);
        let mut to = total.saturating_sub(1);
        let mut widen = |range: Option<RangeInclusive<u16>>| {
            if let Some(range) = range {
                from = from.min(*range.start());
                to = to.max(*range.end());
            }
        };
        for rule in &self.manifest.rules {
            let rows = rule.region.rows.or(self.manifest.default_rows);
            widen(rows.unwrap_or(RowRange::ALL).resolve(total));
        }
        from..=to
    }

    /// Runs the rules on the screen.
    pub fn evaluate(&mut self, screen: &Screen, now: Instant) -> Evaluation {
        let mut matches = Vec::new();
        let mut recheck: Option<Duration> = None;
        for (index, rule) in self.manifest.rules.iter().enumerate() {
            let found = rule_matches(&self.manifest, rule, screen);
            let Some(found) = found else {
                self.since[index] = None;
                continue;
            };
            let since = *self.since[index].get_or_insert(now);
            let held = now.saturating_duration_since(since);
            let needed = Duration::from_millis(rule.hold_ms);
            if held < needed {
                let left = needed - held;
                recheck = Some(recheck.map_or(left, |r| r.min(left)));
                continue;
            }
            matches.push(found);
        }
        if matches.iter().all(|m| m.opens.is_none())
            && let Some(unknown) = unknown_dialog(&self.manifest, screen)
        {
            matches.push(unknown);
        }
        Evaluation {
            matches,
            recheck_after: recheck,
        }
    }
}

// --- the text of a region ---------------------------------------------------------------------

/// The normalized rows of a region, with the number of each row.
fn rows_of(manifest: &ScreenManifest, rule: &Rule, screen: &Screen) -> Vec<(u16, String)> {
    let range = rule
        .region
        .rows
        .or(manifest.default_rows)
        .unwrap_or(RowRange::ALL)
        .resolve(screen.total_rows);
    let Some(range) = range else {
        return Vec::new();
    };
    range
        .filter(|y| screen.row(*y).is_some())
        .map(|y| (y, manifest.normalize.apply(&screen.row_text(y))))
        .collect()
}

fn joined(rows: &[(u16, String)]) -> String {
    rows.iter()
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn style_holds(matcher: &StyleMatcher, cell: &Cell) -> bool {
    let rgb = |color: Color| match color {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        _ => None,
    };
    let attr = |flag: Attrs| cell.style.attrs.contains(flag);
    matcher
        .fg
        .is_none_or(|want| rgb(cell.style.fg) == Some(want))
        && matcher
            .bg
            .is_none_or(|want| rgb(cell.style.bg) == Some(want))
        && matcher.bold.is_none_or(|want| attr(Attrs::BOLD) == want)
        && matcher.dim.is_none_or(|want| attr(Attrs::DIM) == want)
        && matcher
            .italic
            .is_none_or(|want| attr(Attrs::ITALIC) == want)
        && matcher
            .inverse
            .is_none_or(|want| attr(Attrs::INVERSE) == want)
}

/// The match of a rule on the screen, or `None`.
fn rule_matches(manifest: &ScreenManifest, rule: &Rule, screen: &Screen) -> Option<ScreenMatch> {
    match rule.region.screen {
        ScreenKind::AltOnly if !screen.alt_screen => return None,
        ScreenKind::PrimaryOnly if screen.alt_screen => return None,
        _ => {}
    }
    if let Some(title) = &rule.title
        && !screen.title.as_deref().is_some_and(|t| title.is_match(t))
    {
        return None;
    }
    let rows = rows_of(manifest, rule, screen);
    if rule.cursor_in_region && !rows.iter().any(|(y, _)| *y == screen.cursor.0) {
        return None;
    }
    if let Some(matcher) = &rule.style {
        let hit = rows.iter().any(|(y, _)| {
            screen
                .row(*y)
                .is_some_and(|cells| cells.iter().any(|c| style_holds(matcher, c)))
        });
        if !hit {
            return None;
        }
    }
    let text = joined(&rows);
    if rule.not.iter().any(|re| re.is_match(&text)) {
        return None;
    }
    if !rule.all.iter().all(|re| re.is_match(&text)) {
        return None;
    }
    if !rule.any.is_empty() && !rule.any.iter().any(|re| re.is_match(&text)) {
        return None;
    }
    let mut captures = BTreeMap::new();
    for re in rule.all.iter().chain(&rule.any) {
        collect_named(re, &text, &mut captures);
    }
    let mut dialog = None;
    // The question of the dialog. A harness draws its dialog at the bottom of the screen, and the
    // text above it (the output of a tool, for example) can look like a dialog too. So the last
    // row that fits is the question.
    let title_from = rule
        .interaction
        .as_ref()
        .and_then(|i| i.title_from.as_ref());
    let question = title_from.and_then(|t| {
        rows.iter()
            .rev()
            .find(|(_, row)| t.regex.is_match(row))
            .map(|(y, row)| (*y, t, row))
    });
    let title_row = question.map(|(y, _, _)| y);
    if let Some((_, title, row)) = question
        && let Some(found) = title.regex.captures(row)
        && let Some(group) = found.get(title.group)
    {
        captures.insert("title".to_owned(), group.as_str().trim().to_owned());
    }
    // The options of a dialog are below its question.
    if let Some(rule_dialog) = &rule.dialog {
        let below: Vec<(u16, String)> = rows
            .iter()
            .filter(|(y, _)| title_row.is_none_or(|t| *y >= t))
            .cloned()
            .collect();
        dialog = read_dialog(rule_dialog, &below);
    }
    if let Some((from, trim)) = rule.interaction.as_ref().and_then(|i| i.command_from) {
        // The command is the one of this dialog, and not a text that a pattern found elsewhere.
        captures.remove("command");
        let end = dialog.as_ref().map_or(u16::MAX, |d| d.rows.0);
        let command = title_row.and_then(|y| match from {
            CommandFrom::BoxAboveQuestion => box_above(screen, manifest.normalize, y, trim),
            CommandFrom::ShellBelowQuestion => shell_below(&rows, y, end, trim),
        });
        if let Some(command) = command {
            captures.insert("command".to_owned(), command);
        }
    }
    if let Some(dialog) = dialog.as_mut() {
        dialog.title = captures.get("title").cloned();
        dialog.command = captures.get("command").cloned();
    }
    Some(ScreenMatch {
        rule_id: rule.id.clone(),
        state: rule.state,
        opens: rule.opens,
        confidence: rule.confidence,
        captures,
        keystroke_answerable: dialog.is_some(),
        dialog,
    })
}

fn collect_named(re: &Regex, text: &str, into: &mut BTreeMap<String, String>) {
    let names: Vec<&str> = re.capture_names().flatten().collect();
    if names.is_empty() {
        return;
    }
    if let Some(found) = re.captures(text) {
        for name in names {
            if let Some(group) = found.name(name) {
                into.entry(name.to_owned())
                    .or_insert_with(|| group.as_str().trim().to_owned());
            }
        }
    }
}

// --- the box above a question ------------------------------------------------------------------

/// A line made of the characters that frame a box.
fn is_rule_line(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty()
        && t.chars().all(|c| {
            matches!(
                c,
                '─' | '╌'
                    | '━'
                    | '┄'
                    | '┈'
                    | '═'
                    | '-'
                    | '='
                    | '╭'
                    | '╮'
                    | '╰'
                    | '╯'
                    | '│'
            )
        })
        && t.chars().filter(|c| !c.is_whitespace()).count() >= 3
}

/// The lines above row `question`: the rules directly above it are skipped, and the text goes up to
/// the next rule.
fn box_above(screen: &Screen, normalize: Normalize, question: u16, trim: bool) -> Option<String> {
    let mut y = question;
    let mut lines: Vec<String> = Vec::new();
    // The rules that frame the box, directly above the question.
    while y > 0 {
        y -= 1;
        let text = normalize.apply(&screen.row_text(y));
        if is_rule_line(&text) {
            continue;
        }
        y += 1;
        break;
    }
    while y > 0 {
        y -= 1;
        let text = normalize.apply(&screen.row_text(y));
        if is_rule_line(&text) {
            break;
        }
        lines.push(text);
    }
    lines.reverse();
    let text = lines.join("\n");
    let text = if trim { text.trim().to_owned() } else { text };
    (!text.is_empty()).then_some(text)
}

/// The command between the question (row `question`) and the options (row `end`): the first line
/// that starts with `$ `, without the `$ `, and the lines below it up to an empty line.
fn shell_below(rows: &[(u16, String)], question: u16, end: u16, trim: bool) -> Option<String> {
    let mut lines: Vec<&str> = Vec::new();
    for (_, text) in rows.iter().filter(|(y, _)| *y > question && *y < end) {
        if lines.is_empty() {
            if let Some(rest) = text.trim_start().strip_prefix("$ ") {
                lines.push(rest);
            }
            continue;
        }
        if text.trim().is_empty() {
            break;
        }
        lines.push(text);
    }
    let text = lines.join("\n");
    let text = if trim { text.trim().to_owned() } else { text };
    (!text.is_empty()).then_some(text)
}

// --- dialogs -----------------------------------------------------------------------------------

fn read_dialog(rule: &DialogRule, rows: &[(u16, String)]) -> Option<Dialog> {
    let mut options = Vec::new();
    for (y, text) in rows {
        let Some(found) = rule.options.captures(text) else {
            continue;
        };
        let label = found.name("label")?.as_str().trim().to_owned();
        let pointer_text = found.name("ptr").map(|m| m.as_str().trim());
        let pointed = match (&rule.pointer, pointer_text) {
            (Some(glyph), Some(seen)) => seen == glyph,
            (None, Some(seen)) => !seen.is_empty(),
            _ => false,
        };
        let number = found
            .name("n")
            .and_then(|m| m.as_str().trim().parse::<u32>().ok());
        let checked = found.name("check").map(|m| {
            let mark = m.as_str().trim();
            matches!(mark, "☒" | "✔" | "[x]" | "[X]" | "◉" | "●")
        });
        options.push(DialogOption {
            number,
            label,
            row: *y,
            pointed,
            checked,
        });
    }
    if options.is_empty() {
        return None;
    }
    // A dialog numbers its options in order. Numbers that do not grow are two lists below the
    // question, for example a list in the text and the list of the dialog: no key may answer that.
    let numbers: Vec<u32> = options.iter().filter_map(|o| o.number).collect();
    if numbers.windows(2).any(|pair| pair[1] <= pair[0]) {
        return None;
    }
    let first = options.first().map_or(0, |o| o.row);
    let last = options.last().map_or(0, |o| o.row);
    let pointer = options.iter().position(|o| o.pointed);
    let choices = rule
        .labels
        .iter()
        .filter_map(|(name, pattern)| {
            let at = options.iter().position(|o| pattern.is_match(&o.label))?;
            Some((name.clone(), at))
        })
        .collect();
    Some(Dialog {
        options,
        pointer,
        accelerators: rule.accelerators == Accelerators::Digits,
        map: rule.map.clone(),
        choices,
        multi_toggle: rule.multi_toggle.clone(),
        rows: (first, last),
        title: None,
        command: None,
    })
}

/// The pointers that a menu of a terminal program draws.
const POINTERS: &[char] = &['❯', '›', '>', '▶', '»', '→'];

/// A dialog that no rule reads: a boxed region with numbered options, one of them
/// with a pointer. It opens a provisional question that no keystroke may answer.
fn unknown_dialog(manifest: &ScreenManifest, screen: &Screen) -> Option<ScreenMatch> {
    let from = screen.total_rows.saturating_sub(40);
    let mut options: Vec<DialogOption> = Vec::new();
    let mut framed = false;
    for y in from..screen.total_rows {
        let text = manifest.normalize.apply(&screen.row_text(y));
        if is_rule_line(&text) {
            framed = true;
            continue;
        }
        let trimmed = text.trim_start();
        let (pointed, rest) = match trimmed.chars().next() {
            Some(c) if POINTERS.contains(&c) => (true, trimmed[c.len_utf8()..].trim_start()),
            _ => (false, trimmed),
        };
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let after = &rest[digits.len()..];
        let numbered = !digits.is_empty()
            && digits.len() <= 2
            && (after.starts_with(". ") || after.starts_with(") "));
        if !numbered {
            // A gap ends a list; a description line under an option does not.
            if text.trim().is_empty() && !options.is_empty() {
                break;
            }
            continue;
        }
        options.push(DialogOption {
            number: digits.parse().ok(),
            label: after[2..].trim().to_owned(),
            row: y,
            pointed,
            checked: None,
        });
    }
    let consecutive = options.windows(2).all(|w| w[1].number > w[0].number);
    let pointer = options.iter().position(|o| o.pointed);
    if options.len() < 2 || !consecutive || pointer.is_none() || !framed {
        return None;
    }
    let first = options.first().map_or(0, |o| o.row);
    let last = options.last().map_or(0, |o| o.row);
    let mut captures = BTreeMap::new();
    captures.insert("hint".to_owned(), UNKNOWN_DIALOG_HINT.to_owned());
    Some(ScreenMatch {
        rule_id: UNKNOWN_DIALOG_RULE.to_owned(),
        state: None,
        opens: Some(InteractionKind::Question),
        confidence: 0.5,
        captures,
        dialog: Some(Dialog {
            options,
            pointer,
            accelerators: false,
            map: BTreeMap::new(),
            choices: BTreeMap::new(),
            multi_toggle: None,
            rows: (first, last),
            title: None,
            command: None,
        }),
        keystroke_answerable: false,
    })
}

#[cfg(test)]
mod tests;
