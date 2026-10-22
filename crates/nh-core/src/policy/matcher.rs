//! Does a rule match a request?
//!
//! The matcher is pure: it reads a rule and the facts of a request, and it gives a verdict. A
//! command is normalized (trimmed, runs of white space collapsed) and nothing else. An `allow`
//! rule never matches a command line with many commands, a redirection or a substitution. A
//! `deny` rule matches such a line when it matches any segment of it. A deny always beats an
//! allow.

use nh_proto::api::{calls::system::PolicyDecision, common::Harness};

use super::Rule;

/// What a request is, as far as a rule can see.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subject {
    pub harness: Harness,
    /// The name of the tool, such as `Bash` or `Edit`.
    pub tool: String,
    /// The command line of a shell tool.
    pub command: Option<String>,
    /// The paths that a file tool changes, relative to the root of the workspace. A path outside
    /// the root is absolute, so that no glob matches it.
    pub paths: Vec<String>,
    /// The root of the workspace.
    pub workspace: String,
}

/// What the rules say about a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict<'a> {
    /// No rule matches.
    None,
    Allow(&'a Rule),
    Deny(&'a Rule),
}

impl<'a> Verdict<'a> {
    /// The rule that decided, when one did.
    pub fn rule(&self) -> Option<&'a Rule> {
        match self {
            Verdict::None => None,
            Verdict::Allow(rule) | Verdict::Deny(rule) => Some(rule),
        }
    }
}

/// The shape of the `command` of a rule. A `*` is allowed only at the end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandPattern {
    /// The whole line.
    Exact(String),
    /// A prefix: `git status*`.
    Prefix(String),
    /// A command and its arguments: `pnpm test:*` has `pnpm test`, `pnpm test --watch` and
    /// `pnpm test:unit`, and not `pnpm testing`.
    Word(String),
}

impl CommandPattern {
    /// Reads the text of a rule. The error says what is wrong.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = normalize(text);
        if text.is_empty() {
            return Err("the command of a rule is empty".to_owned());
        }
        if let Some(stem) = text.strip_suffix(":*") {
            if stem.contains('*') {
                return Err(format!("`{text}`: a `*` is allowed only at the end"));
            }
            if stem.is_empty() {
                return Err(format!("`{text}`: the rule names no command"));
            }
            return Ok(Self::Word(stem.to_owned()));
        }
        if let Some(stem) = text.strip_suffix('*') {
            if stem.contains('*') {
                return Err(format!("`{text}`: a `*` is allowed only at the end"));
            }
            if stem.trim().is_empty() {
                return Err(format!("`{text}`: the rule names no command"));
            }
            return Ok(Self::Prefix(stem.to_owned()));
        }
        if text.contains('*') {
            return Err(format!("`{text}`: a `*` is allowed only at the end"));
        }
        Ok(Self::Exact(text))
    }

    /// The text as a rule writes it.
    pub fn text(&self) -> String {
        match self {
            Self::Exact(text) => text.clone(),
            Self::Prefix(stem) => format!("{stem}*"),
            Self::Word(stem) => format!("{stem}:*"),
        }
    }

    /// Whether a normalized command (one segment) fits.
    pub fn fits(&self, command: &str) -> bool {
        match self {
            Self::Exact(text) => command == text,
            Self::Prefix(stem) => command.starts_with(stem.as_str()),
            Self::Word(stem) => command.strip_prefix(stem.as_str()).is_some_and(|rest| {
                rest.is_empty() || rest.starts_with(' ') || rest.starts_with(':')
            }),
        }
    }
}

/// Trims the text and collapses the white space.
pub fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether a line does more than its first words say: many commands, a redirection, a
/// substitution or a new line.
pub fn is_compound(line: &str) -> bool {
    line.contains(['\n', '\r', ';', '|', '&', '>', '<', '`']) || line.contains("$(")
}

/// The commands of a line, split on the operators of [`is_compound`]. A line without operators
/// has one segment.
pub fn segments(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let flush = |current: &mut String, out: &mut Vec<String>| {
        let text = normalize(current);
        if !text.is_empty() {
            out.push(text);
        }
        current.clear();
    };
    while let Some(c) = chars.next() {
        match c {
            '\n' | '\r' | ';' | '|' | '&' | '>' | '<' | '`' | ')' => flush(&mut current, &mut out),
            '$' if chars.peek() == Some(&'(') => {
                chars.next();
                flush(&mut current, &mut out);
            }
            other => current.push(other),
        }
    }
    flush(&mut current, &mut out);
    out
}

/// Whether `path` fits a glob of the rule. `*` and `?` do not cross a `/`, and `**` as a whole
/// segment fits any number of segments.
pub fn glob_matches(pattern: &str, path: &str) -> bool {
    let pattern: Vec<&str> = pattern.split('/').collect();
    let path: Vec<&str> = path.split('/').collect();
    segments_match(&pattern, &path)
}

fn segments_match(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|skip| segments_match(rest, &path[skip..])),
        Some((first, rest)) => match path.split_first() {
            Some((head, tail)) => wildcard(first, head) && segments_match(rest, tail),
            None => false,
        },
    }
}

/// `*` and `?` inside one segment.
fn wildcard(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            mark = t;
            p += 1;
        } else if let Some(s) = star {
            p = s + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

impl Rule {
    /// Whether the rule fits the request, for the decision of the rule.
    pub fn matches(&self, subject: &Subject) -> bool {
        if !self.harness.is_empty() && !self.harness.contains(&subject.harness) {
            return false;
        }
        if self.tool != "*" && self.tool != subject.tool {
            return false;
        }
        if let Some(workspace) = &self.workspace
            && workspace.trim_end_matches('/') != subject.workspace.trim_end_matches('/')
        {
            return false;
        }
        let allow = self.decision == PolicyDecision::Allow;
        if let Some(pattern) = &self.command {
            let Some(raw) = subject.command.as_deref() else {
                return false;
            };
            // The white space collapses in `line`, and a new line is white space. The checks for
            // many commands read the raw text, or `pnpm test` and a second command on the next
            // line would look like one command.
            let line = normalize(raw);
            return if allow {
                !is_compound(raw) && pattern.fits(&line)
            } else {
                pattern.fits(&line) || segments(raw).iter().any(|s| pattern.fits(s))
            };
        }
        if !self.paths.is_empty() {
            if subject.paths.is_empty() {
                return false;
            }
            let fits = |path: &String| self.paths.iter().any(|glob| glob_matches(glob, path));
            return if allow {
                subject.paths.iter().all(fits)
            } else {
                subject.paths.iter().any(fits)
            };
        }
        true
    }
}

/// What the rules say about a request. A matching deny beats a matching allow. Among rules of the
/// same decision the first one counts.
pub fn evaluate<'a>(rules: &'a [Rule], subject: &Subject) -> Verdict<'a> {
    let mut allow = None;
    for rule in rules.iter().filter(|r| r.matches(subject)) {
        match rule.decision {
            PolicyDecision::Deny => return Verdict::Deny(rule),
            PolicyDecision::Allow => {
                allow.get_or_insert(rule);
            }
        }
    }
    allow.map_or(Verdict::None, Verdict::Allow)
}

#[cfg(test)]
mod tests;
