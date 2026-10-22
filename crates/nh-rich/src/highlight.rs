//! Highlighted text as spans of classes.
//!
//! `syntect` parses the text with the grammars of Sublime Text. This module turns the scopes that it
//! finds into the small set of classes in [`class`]. The UI gives each class a colour, so a theme is
//! a matter of CSS. The offsets are in UTF-16 code units.

use std::{ops::Range, sync::OnceLock};

use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};

use crate::text::Utf16Map;

/// The classes of a span. The UI makes the CSS class `hl-<name>` from each one, and the markdown
/// renderer writes the same classes in its HTML.
pub mod class {
    pub const KEYWORD: &str = "kw";
    pub const STRING: &str = "str";
    pub const NUMBER: &str = "num";
    pub const COMMENT: &str = "com";
    pub const FUNCTION: &str = "fn";
    pub const TYPE: &str = "ty";
    pub const CONSTANT: &str = "const";
    pub const OPERATOR: &str = "op";
    pub const TAG: &str = "tag";
    pub const ATTRIBUTE: &str = "attr";
    pub const VARIABLE: &str = "var";
    pub const ESCAPE: &str = "esc";
    pub const ERROR: &str = "err";
    /// The program of a shell command.
    pub const COMMAND: &str = "cmd";
    /// A flag of a shell command.
    pub const OPTION: &str = "opt";

    /// Every class, for the sanitizer of the markdown HTML.
    pub const ALL: &[&str] = &[
        KEYWORD, STRING, NUMBER, COMMENT, FUNCTION, TYPE, CONSTANT, OPERATOR, TAG, ATTRIBUTE,
        VARIABLE, ESCAPE, ERROR, COMMAND, OPTION,
    ];
}

/// A range of text with a class, in UTF-16 code units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub class: &'static str,
}

/// Text with highlighted ranges.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Spans {
    pub text: String,
    pub spans: Vec<Span>,
}

impl Spans {
    /// Text with no highlight.
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            spans: Vec::new(),
        }
    }
}

/// Text past this size gets no highlight, and the lines after it are plain.
pub const HIGHLIGHT_CAP: usize = 256 * 1024;
/// A line longer than this gets no highlight. A regex over a minified file is slow.
pub const MAX_LINE: usize = 4096;

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// A language of the grammars that this build holds.
#[derive(Clone, Copy)]
pub struct Language(&'static SyntaxReference);

impl Language {
    pub fn name(&self) -> &'static str {
        &self.0.name
    }
}

impl std::fmt::Debug for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Language({})", self.0.name)
    }
}

/// The language for the name of a code fence (`rust`, `ts`, `sh`) or an extension (`rs`).
///
/// TypeScript has no grammar in the default set, and JavaScript is close enough to colour it.
pub fn language(token: &str) -> Option<Language> {
    let token = token.trim().trim_start_matches('.').to_ascii_lowercase();
    if token.is_empty() {
        return None;
    }
    let token = match token.as_str() {
        "ts" | "tsx" | "typescript" | "jsx" | "mjs" | "cjs" | "node" => "js",
        "shell" | "shellscript" | "zsh" | "console" | "shell-session" => "sh",
        "python3" => "py",
        "yml" => "yaml",
        "jsonc" | "json5" => "json",
        "c++" | "cc" | "hpp" => "cpp",
        "golang" => "go",
        "patch" => "diff",
        "md" => "markdown",
        other => other,
    };
    syntaxes()
        .find_syntax_by_token(token)
        .filter(|syntax| syntax.name != "Plain Text")
        .map(Language)
}

/// The language for a file path, from its name or its extension.
pub fn language_of_path(path: &str) -> Option<Language> {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    if let Some(language) = syntaxes()
        .find_syntax_by_extension(name)
        .filter(|syntax| syntax.name != "Plain Text")
    {
        return Some(Language(language));
    }
    let extension = name.rsplit_once('.')?.1;
    language(extension)
}

/// The scopes that make a class. The first rule that matches wins, so the narrow rules come first.
fn rules() -> &'static [(Scope, &'static str)] {
    static RULES: OnceLock<Vec<(Scope, &'static str)>> = OnceLock::new();
    RULES.get_or_init(|| {
        const TABLE: &[(&str, &str)] = &[
            ("comment", class::COMMENT),
            ("constant.character.escape", class::ESCAPE),
            ("constant.numeric", class::NUMBER),
            ("constant.language", class::CONSTANT),
            ("constant", class::CONSTANT),
            ("string", class::STRING),
            ("keyword.operator", class::OPERATOR),
            ("keyword", class::KEYWORD),
            ("storage", class::KEYWORD),
            ("variable.language", class::KEYWORD),
            ("variable.parameter", class::VARIABLE),
            ("entity.name.function", class::FUNCTION),
            ("entity.name.tag", class::TAG),
            ("entity.name", class::TYPE),
            ("entity.other.attribute-name", class::ATTRIBUTE),
            ("entity.other.inherited-class", class::TYPE),
            ("support.function", class::FUNCTION),
            ("support.type", class::TYPE),
            ("support.class", class::TYPE),
            ("support.constant", class::CONSTANT),
            ("variable.function", class::FUNCTION),
            ("invalid", class::ERROR),
        ];
        TABLE
            .iter()
            .filter_map(|(scope, class)| Scope::new(scope).ok().map(|scope| (scope, *class)))
            .collect()
    })
}

/// The class of the innermost scope that has one. Punctuation and `meta` scopes have none, so a
/// quote has the class of its string.
fn classify(stack: &ScopeStack) -> Option<&'static str> {
    stack.as_slice().iter().rev().find_map(|scope| {
        rules()
            .iter()
            .find(|(prefix, _)| prefix.is_prefix_of(*scope))
            .map(|(_, class)| *class)
    })
}

/// A parser that goes line by line, so a state (a comment that is open, a string with many lines)
/// goes on from one line to the next.
pub struct LineHighlighter {
    parse: ParseState,
    stack: ScopeStack,
    /// A line failed to parse. The lines after it are plain.
    failed: bool,
}

impl LineHighlighter {
    pub fn new(language: Language) -> Self {
        Self {
            parse: ParseState::new(language.0),
            stack: ScopeStack::new(),
            failed: false,
        }
    }

    /// The spans of one line, without its line end. The offsets are in UTF-16 code units from the
    /// start of the line.
    pub fn line(&mut self, line: &str) -> Vec<Span> {
        let pieces = self.pieces(line);
        if pieces.is_empty() {
            return Vec::new();
        }
        let map = Utf16Map::new(line);
        pieces
            .into_iter()
            .map(|(range, class)| Span {
                start: map.at(range.start),
                end: map.at(range.end),
                class,
            })
            .collect()
    }

    /// The classes of one line, as byte ranges. Ranges that touch and have the same class are one.
    fn pieces(&mut self, line: &str) -> Vec<(Range<usize>, &'static str)> {
        if self.failed || line.len() > MAX_LINE {
            return Vec::new();
        }
        let mut text = String::with_capacity(line.len() + 1);
        text.push_str(line);
        text.push('\n');
        let Ok(ops) = self.parse.parse_line(&text, syntaxes()) else {
            self.failed = true;
            return Vec::new();
        };
        let mut pieces: Vec<(Range<usize>, &'static str)> = Vec::new();
        let mut at = 0usize;
        for (position, op) in ops {
            let position = position.min(line.len());
            if position > at {
                push(&mut pieces, &self.stack, at..position);
                at = position;
            }
            if self.stack.apply(&op).is_err() {
                self.failed = true;
                return pieces;
            }
        }
        if at < line.len() {
            push(&mut pieces, &self.stack, at..line.len());
        }
        pieces
    }
}

fn push(pieces: &mut Vec<(Range<usize>, &'static str)>, stack: &ScopeStack, range: Range<usize>) {
    let Some(class) = classify(stack) else {
        return;
    };
    if let Some((last, last_class)) = pieces.last_mut()
        && *last_class == class
        && last.end == range.start
    {
        last.end = range.end;
        return;
    }
    pieces.push((range, class));
}

/// The text of a line without its line end (`\n` or `\r\n`).
pub fn without_line_end(line: &str) -> &str {
    let line = line.strip_suffix('\n').unwrap_or(line);
    line.strip_suffix('\r').unwrap_or(line)
}

/// Highlights `text` in a language. An unknown language gives plain text. The offsets count from
/// the start of the whole text, and a line end is one or two code units.
pub fn highlight(text: &str, lang: &str) -> Spans {
    match language(lang) {
        Some(language) => highlight_with(text, language),
        None => Spans::plain(text),
    }
}

/// Highlights `text` in a known language.
pub fn highlight_with(text: &str, language: Language) -> Spans {
    let mut highlighter = LineHighlighter::new(language);
    let mut spans: Vec<Span> = Vec::new();
    let mut base = 0u32;
    let mut consumed = 0usize;
    for raw in text.split_inclusive('\n') {
        if consumed >= HIGHLIGHT_CAP {
            break;
        }
        consumed += raw.len();
        for span in highlighter.line(without_line_end(raw)) {
            let (start, end) = (base + span.start, base + span.end);
            match spans.last_mut() {
                Some(last) if last.class == span.class && last.end == start => last.end = end,
                _ => spans.push(Span {
                    start,
                    end,
                    class: span.class,
                }),
            }
        }
        base += crate::text::utf16_len(raw);
    }
    Spans {
        text: text.to_owned(),
        spans,
    }
}

/// Escapes text for HTML.
pub(crate) fn escape_html(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
}

/// The text as HTML with a `<span class="hl-…">` around each highlighted range. The text is
/// escaped, and the classes are those of [`class`], so the HTML has no markup but the spans.
pub(crate) fn highlight_html(text: &str, language: Option<Language>) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    let mut highlighter = language.map(LineHighlighter::new);
    let mut consumed = 0usize;
    for raw in text.split_inclusive('\n') {
        let content = without_line_end(raw);
        let pieces = match highlighter.as_mut() {
            Some(highlighter) if consumed < HIGHLIGHT_CAP => highlighter.pieces(content),
            _ => Vec::new(),
        };
        consumed += raw.len();
        let mut at = 0;
        for (range, class) in pieces {
            escape_html(&content[at..range.start], &mut out);
            out.push_str("<span class=\"hl-");
            out.push_str(class);
            out.push_str("\">");
            escape_html(&content[range.clone()], &mut out);
            out.push_str("</span>");
            at = range.end;
        }
        escape_html(&raw[at..], &mut out);
    }
    out
}

#[cfg(test)]
mod tests;
