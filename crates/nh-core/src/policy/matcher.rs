//! Does a rule match a request?
//!
//! The matcher is pure: it reads a rule and the facts of a request, and it gives a verdict. A
//! command is normalized (trimmed, runs of white space collapsed) and nothing else. An `allow`
//! rule matches only a plain command line ([`is_plain`]): words of letters, digits and
//! `_./:=@%+,-`, and quoted strings of the same characters. Any other character can make the
//! shell run more than the first words say (`;`, `|`, `$(`, a glob qualifier of zsh such as
//! `*(e:…:)`, a process substitution such as `=(…)`), so such a line goes to a person. A `deny`
//! rule matches a line when it matches the line or any command in it, also after the quotes,
//! the path of the program and the wrappers such as `env` are taken off ([`deny_candidates`]). A
//! deny always beats an allow.

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

/// A character that the shell takes as it is, outside and inside quotes.
fn plain_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(c, '_' | '.' | '/' | ':' | '=' | '@' | '%' | '+' | ',' | '-')
}

/// Whether a line is one command that the shell runs as its words say: words of letters, digits
/// and `_./:=@%+,-`, spaces and tabs, and strings in single or double quotes of the same
/// characters and spaces. Every other character (`;`, `|`, `&`, `<`, `>`, `$`, `` ` ``, `(`, `)`,
/// `{`, `}`, `*`, `?`, `[`, `]`, `~`, `!`, `\`, `#`, `^`, a new line, a character that is not
/// ASCII) can run another command or change the words, so a line with one is not plain.
pub fn is_plain(line: &str) -> bool {
    let mut quote: Option<char> = None;
    for c in line.chars() {
        match quote {
            Some(open) if c == open => quote = None,
            Some(_) if plain_char(c) || c == ' ' || c == '\t' => {}
            Some(_) => return false,
            None if c == '\'' || c == '"' => quote = Some(c),
            None if plain_char(c) || c == ' ' || c == '\t' => {}
            None => return false,
        }
    }
    quote.is_none()
}

/// The characters that end a command, or start one inside another, for a deny.
fn ends_command(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r' | ';' | '|' | '&' | '>' | '<' | '`' | '(' | ')' | '{' | '}'
    )
}

/// The commands of a line, split on the characters that end a command or start one inside
/// another: `;`, `|`, `&`, `<`, `>`, `` ` ``, brackets and braces, and a new line. A line without
/// them has one segment.
pub fn segments(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let flush = |current: &mut String, out: &mut Vec<String>| {
        // `$(` and `=(` leave their first character behind.
        let text = normalize(current.trim_end_matches(['$', '=']));
        if !text.is_empty() {
            out.push(text);
        }
        current.clear();
    };
    for c in line.chars() {
        if ends_command(c) {
            flush(&mut current, &mut out);
        } else {
            current.push(c);
        }
    }
    flush(&mut current, &mut out);
    out
}

/// Words that come before a program and are not the program: the reserved words of the shell
/// and the modifiers of zsh.
const BEFORE_PROGRAM: &[&str] = &[
    "!",
    "if",
    "then",
    "else",
    "elif",
    "while",
    "until",
    "do",
    "time",
    "coproc",
    "noglob",
    "nocorrect",
    "-",
];

/// Programs that run the words after them as a command.
const WRAPPERS: &[&str] = &[
    "env",
    "sudo",
    "doas",
    "su",
    "pkexec",
    "runuser",
    "nice",
    "nohup",
    "timeout",
    "gtimeout",
    "stdbuf",
    "unbuffer",
    "xargs",
    "parallel",
    "time",
    "watch",
    "caffeinate",
    "exec",
    "command",
    "builtin",
    "eval",
    "sh",
    "bash",
    "zsh",
    "dash",
    "ksh",
    "fish",
    "chroot",
    "setsid",
    "flock",
    "ionice",
    "taskset",
    "script",
    "arch",
    "sandbox-exec",
    "find",
    "ssh",
];

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    })
}

fn basename(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}

/// The command lines that a deny compares with its pattern: each segment of the line as it is,
/// and each segment with the quotes and backslashes taken out of its words, with the reserved
/// words and the variables in front taken off, and with the program by its name and not its path.
/// After a wrapper (`env`, `sudo`, `timeout`, `sh -c`), each run of words to the end is a command
/// too. A deny matches more than the command runs, never less.
pub fn deny_candidates(line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let add = |words: &[String], out: &mut Vec<String>| {
        if let Some((first, rest)) = words.split_first() {
            let mut text = basename(first).to_owned();
            for word in rest {
                text.push(' ');
                text.push_str(word);
            }
            if !out.contains(&text) {
                out.push(text);
            }
        }
    };
    for segment in segments(line) {
        if !out.contains(&segment) {
            out.push(segment.clone());
        }
        let words: Vec<String> = segment
            .split_whitespace()
            .map(|w| w.replace(['\'', '"', '\\'], ""))
            .filter(|w| !w.is_empty())
            .collect();
        let start = words
            .iter()
            .position(|w| !BEFORE_PROGRAM.contains(&w.as_str()) && !is_assignment(w))
            .unwrap_or(words.len());
        let words = &words[start..];
        add(words, &mut out);
        if let Some(at) = words.iter().position(|w| WRAPPERS.contains(&basename(w))) {
            for from in at + 1..words.len() {
                add(&words[from..], &mut out);
            }
        }
    }
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
            // The white space collapses in `line`, and a new line is white space. The check of
            // the characters reads the raw text, or `pnpm test` and a second command on the next
            // line would look like one command.
            let line = normalize(raw);
            return if allow {
                is_plain(raw) && pattern.fits(&line)
            } else {
                pattern.fits(&line) || deny_candidates(raw).iter().any(|s| pattern.fits(s))
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
