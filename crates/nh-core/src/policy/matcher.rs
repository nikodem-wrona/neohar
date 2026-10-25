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

/// How deep a deny reads commands inside commands: substitutions, `sh -c`, wrappers.
const MAX_DEPTH: usize = 4;

/// The position after the matching `)` of a `(` before `from`, or the end.
fn after_paren(chars: &[char], from: usize) -> usize {
    let mut depth = 1;
    let mut at = from;
    while let Some(&c) = chars.get(at) {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return at + 1;
                }
            }
            _ => {}
        }
        at += 1;
    }
    chars.len()
}

/// The position of the next `close` from `from`, or the end.
fn find_char(chars: &[char], from: usize, close: char) -> usize {
    chars
        .get(from..)
        .and_then(|rest| rest.iter().position(|&c| c == close))
        .map_or(chars.len(), |n| from + n)
}

fn text_of(chars: &[char], from: usize, to: usize) -> String {
    chars
        .get(from..to.max(from))
        .unwrap_or(&[])
        .iter()
        .collect()
}

/// The commands of a line, as a shell reads them: the words of each command with their quotes
/// and backslashes taken out, split on the characters that end a command outside quotes
/// ([`ends_command`]). The commands in `$(…)` and `` `…` ``, also inside double quotes, are
/// commands too. A `;` inside quotes is part of a word, so a commit message does not split.
pub fn commands(line: &str) -> Vec<Vec<String>> {
    commands_at(line, 0)
}

fn commands_at(line: &str, depth: usize) -> Vec<Vec<String>> {
    let chars: Vec<char> = line.chars().collect();
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut nested: Vec<String> = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let end_word = |word: &mut String, in_word: &mut bool, words: &mut Vec<String>| {
        if *in_word {
            words.push(std::mem::take(word));
        }
        *in_word = false;
    };
    let end_command = |words: &mut Vec<String>, out: &mut Vec<Vec<String>>| {
        if !words.is_empty() {
            out.push(std::mem::take(words));
        }
    };
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        match c {
            '\\' => {
                if let Some(&next) = chars.get(i + 1)
                    && next != '\n'
                {
                    word.push(next);
                }
                in_word = true;
                i += 2;
            }
            '\'' => {
                let close = find_char(&chars, i + 1, '\'');
                word.push_str(&text_of(&chars, i + 1, close));
                in_word = true;
                i = close + 1;
            }
            '"' => {
                let mut j = i + 1;
                while let Some(&inner) = chars.get(j) {
                    match inner {
                        '"' => break,
                        '\\' => {
                            word.extend(chars.get(j + 1));
                            j += 2;
                        }
                        '$' if chars.get(j + 1) == Some(&'(') => {
                            let close = after_paren(&chars, j + 2);
                            nested.push(text_of(&chars, j + 2, close.saturating_sub(1)));
                            j = close;
                        }
                        '`' => {
                            let close = find_char(&chars, j + 1, '`');
                            nested.push(text_of(&chars, j + 1, close));
                            j = close + 1;
                        }
                        other => {
                            word.push(other);
                            j += 1;
                        }
                    }
                }
                in_word = true;
                i = j + 1;
            }
            // A substitution is a command of its own, and adds no text to the word.
            '$' if chars.get(i + 1) == Some(&'(') => {
                let close = after_paren(&chars, i + 2);
                nested.push(text_of(&chars, i + 2, close.saturating_sub(1)));
                i = close;
            }
            '`' => {
                let close = find_char(&chars, i + 1, '`');
                nested.push(text_of(&chars, i + 1, close));
                i = close + 1;
            }
            c if ends_command(c) => {
                end_word(&mut word, &mut in_word, &mut words);
                end_command(&mut words, &mut out);
                i += 1;
            }
            c if c.is_whitespace() => {
                end_word(&mut word, &mut in_word, &mut words);
                i += 1;
            }
            other => {
                word.push(other);
                in_word = true;
                i += 1;
            }
        }
    }
    end_word(&mut word, &mut in_word, &mut words);
    end_command(&mut words, &mut out);
    if depth < MAX_DEPTH {
        for line in nested {
            out.extend(commands_at(&line, depth + 1));
        }
    }
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

/// Programs that run the words after their options as a command: the options that take a value,
/// and the number of operands before the command (the time of `timeout`).
const WRAPPERS: &[(&str, &[&str], usize)] = &[
    ("env", &["-u", "--unset", "-C", "--chdir"], 0),
    (
        "sudo",
        &["-u", "-g", "-h", "-p", "-C", "-D", "-r", "-t", "-U", "-T"],
        0,
    ),
    ("doas", &["-u", "-C"], 0),
    ("pkexec", &["--user"], 0),
    ("runuser", &["-u", "-g", "-G"], 0),
    ("nice", &["-n", "--adjustment"], 0),
    (
        "ionice",
        &["-c", "-n", "-p", "-P", "-u", "--class", "--classdata"],
        0,
    ),
    ("nohup", &[], 0),
    (
        "stdbuf",
        &["-i", "-o", "-e", "--input", "--output", "--error"],
        0,
    ),
    ("unbuffer", &["-p"], 0),
    (
        "xargs",
        &[
            "-n",
            "-I",
            "-P",
            "-L",
            "-d",
            "-s",
            "-E",
            "-a",
            "-J",
            "-R",
            "--max-args",
            "--max-procs",
            "--delimiter",
            "--arg-file",
        ],
        0,
    ),
    ("time", &[], 0),
    ("caffeinate", &["-w", "-t"], 0),
    ("exec", &["-a"], 0),
    ("command", &[], 0),
    ("builtin", &[], 0),
    ("setsid", &[], 0),
    ("arch", &["-arch"], 0),
    ("sandbox-exec", &["-f", "-p", "-n", "-D"], 0),
    ("timeout", &["-s", "-k", "--signal", "--kill-after"], 1),
    ("gtimeout", &["-s", "-k", "--signal", "--kill-after"], 1),
    ("chroot", &["--userspec", "--groups"], 1),
    ("taskset", &[], 1),
    (
        "flock",
        &["-w", "--timeout", "-E", "--conflict-exit-code"],
        1,
    ),
    ("script", &["-F", "-t", "-T", "-I", "-O", "-B"], 1),
];

/// Programs that join the words after their options and give them to a shell: `watch 'rm x'`,
/// `ssh host 'rm x'`. The same table as [`WRAPPERS`].
const JOINERS: &[(&str, &[&str], usize)] = &[
    ("eval", &[], 0),
    ("watch", &["-n", "--interval"], 0),
    (
        "ssh",
        &[
            "-b", "-B", "-c", "-D", "-E", "-e", "-F", "-I", "-i", "-J", "-L", "-l", "-m", "-O",
            "-o", "-p", "-Q", "-R", "-S", "-W", "-w",
        ],
        1,
    ),
    (
        "parallel",
        &[
            "-j",
            "--jobs",
            "-S",
            "--sshlogin",
            "-a",
            "--arg-file",
            "--delay",
            "--timeout",
            "-n",
            "-N",
            "--colsep",
            "-d",
        ],
        0,
    ),
];

/// Programs with an option that names a command for a shell: `sh -c 'rm x'`, `su -c`.
const SHELLS: &[&str] = &[
    "sh", "bash", "zsh", "dash", "ksh", "fish", "ash", "su", "runuser", "script", "flock",
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

/// The position of the command after the options of a wrapper and its operands.
fn after_options(words: &[String], with_value: &[&str], operands: usize) -> usize {
    let mut at = 0;
    let mut left = operands;
    while let Some(word) = words.get(at) {
        if word == "--" {
            return at + 1;
        }
        if word.starts_with('-') && word.len() > 1 {
            at += if with_value.contains(&word.as_str()) {
                2
            } else {
                1
            };
        } else if left > 0 {
            left -= 1;
            at += 1;
        } else {
            break;
        }
    }
    at
}

/// A command that a program runs, in its words.
enum Inner {
    /// The words from this position are a command.
    Words(usize),
    /// This text is a command line for a shell.
    Line(String),
}

/// The commands that the program `name` runs, from the words after it.
fn inner_commands(name: &str, rest: &[String]) -> Vec<Inner> {
    let mut out = Vec::new();
    // `-c 'script'`, also in a cluster such as `-lc`.
    if SHELLS.contains(&name)
        && let Some(at) = rest.iter().position(|w| {
            w == "--command" || (w.starts_with('-') && !w.starts_with("--") && w.contains('c'))
        })
        && let Some(script) = rest.get(at + 1)
    {
        out.push(Inner::Line(script.clone()));
        return out;
    }
    match name {
        "find" => {
            for (at, word) in rest.iter().enumerate() {
                if matches!(word.as_str(), "-exec" | "-execdir" | "-ok" | "-okdir") {
                    out.push(Inner::Words(at + 1));
                }
            }
        }
        "fd" | "fdfind" => {
            for (at, word) in rest.iter().enumerate() {
                if matches!(word.as_str(), "-x" | "-X" | "--exec" | "--exec-batch") {
                    out.push(Inner::Words(at + 1));
                }
            }
        }
        _ => {
            if let Some((_, with_value, operands)) = WRAPPERS.iter().find(|(n, _, _)| *n == name) {
                out.push(Inner::Words(after_options(rest, with_value, *operands)));
            } else if let Some((_, with_value, operands)) =
                JOINERS.iter().find(|(n, _, _)| *n == name)
            {
                let from = after_options(rest, with_value, *operands);
                out.push(Inner::Line(rest.get(from..).unwrap_or(&[]).join(" ")));
            }
        }
    }
    out
}

/// Adds the forms of one command that a deny compares: as it is written and with the program by
/// its name, and then the commands that its program runs.
fn add_candidates(words: &[String], out: &mut Vec<String>, depth: usize) {
    let start = words
        .iter()
        .position(|w| !BEFORE_PROGRAM.contains(&w.as_str()) && !is_assignment(w))
        .unwrap_or(words.len());
    let words = &words[start..];
    let Some((first, rest)) = words.split_first() else {
        return;
    };
    let name = basename(first);
    for program in [first.as_str(), name] {
        let text = std::iter::once(program)
            .chain(rest.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ");
        if !out.contains(&text) {
            out.push(text);
        }
    }
    if depth >= MAX_DEPTH {
        return;
    }
    for inner in inner_commands(name, rest) {
        match inner {
            Inner::Words(from) => add_candidates(rest.get(from..).unwrap_or(&[]), out, depth + 1),
            Inner::Line(line) => {
                for command in commands_at(&line, depth + 1) {
                    add_candidates(&command, out, depth + 1);
                }
            }
        }
    }
}

/// The command lines that a deny compares with its pattern: each command of the line
/// ([`commands`]) without its quotes, the reserved words and the variables in front, with the
/// program as written and by its name. A wrapper at the place of the program (`env`, `sudo`,
/// `timeout`, `sh -c`, `find -exec`) adds the command that it runs, read in the same way. A word
/// in an argument, such as the `time` of a commit message, is not a program.
pub fn deny_candidates(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    for command in commands(line) {
        add_candidates(&command, &mut out, 0);
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
