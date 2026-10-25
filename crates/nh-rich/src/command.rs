//! Shell commands for the approval cards.
//!
//! [`explain`] reads a command line and gives what the card shows: the words of the program, the
//! line with its parts marked, the risk with the reasons for it, and a hint for the policy rule that
//! would allow the command. The rules are a function of the command and of a [`Context`], so the
//! same command has the same risk everywhere.
//!
//! The reader is not a shell, and the rules are a heuristic. They are made to over-warn: a command
//! that they cannot read is `unknown`, and a command with a part that is dangerous is `high`, also
//! when the rest of it looks harmless.

mod lex;
mod network;
mod paths;
mod rules;
mod tables;

use std::path::Path;

use serde::Serialize;

use self::lex::{Op, Token, Word};
use crate::{
    highlight::{Span, Spans, class},
    project::Project,
    text::Utf16Map,
};

/// How much a command can hurt. The order is the order of the danger: `unknown` is above `low`,
/// because a command that nobody read cannot be called safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Low,
    Unknown,
    Medium,
    High,
}

/// What the rules need to know about the place where a command runs.
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    /// The root of the workspace. A write below it is in place.
    pub workspace: &'a Path,
    /// The directory where the command runs.
    pub cwd: &'a Path,
    /// The home of the user, for `~`.
    pub home: Option<&'a Path>,
    pub project: &'a Project,
    /// Hosts that a command may connect to. `*.example.com` has the subdomains. This machine is
    /// always allowed.
    pub network_allow: &'a [String],
}

/// The risk of a command or a change, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Assessment {
    pub risk: Risk,
    pub reasons: Vec<String>,
}

/// A shell command, ready for a card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Command {
    /// The words of the first command of the line, without the quotes. A line with many commands
    /// has the rest in `display`.
    pub argv: Vec<String>,
    /// The whole line, with the program, the flags, the strings and the operators marked.
    pub display: Spans,
    pub cwd: String,
    pub risk: Risk,
    pub reasons: Vec<String>,
    /// The rule that would allow this command, in words. `None` for a command that no rule should
    /// allow: a high risk, a line with many commands, or a line that is not read.
    pub policy_hint: Option<String>,
}

/// The risk of a command line.
pub fn assess_command(source: &str, ctx: &Context<'_>) -> Assessment {
    let verdict = rules::assess(source, ctx);
    Assessment {
        risk: verdict.risk.unwrap_or(Risk::Unknown),
        reasons: verdict.reasons(),
    }
}

/// Everything a card shows about a command line.
pub fn explain(source: &str, ctx: &Context<'_>) -> Command {
    let assessment = assess_command(source, ctx);
    let lexed = lex::lex(source);
    let argv = first_argv(&lexed.tokens);
    let compound = is_compound(&lexed, source);
    let policy_hint = if assessment.risk == Risk::High || compound || lexed.open || argv.is_empty()
    {
        None
    } else {
        policy_hint(&argv)
    };
    Command {
        argv,
        // An invisible character shows as an escape, so the person sees each character that runs.
        display: crate::text::visible_spans(display(source, &lexed.tokens)),
        cwd: ctx.cwd.display().to_string(),
        risk: assessment.risk,
        reasons: assessment.reasons,
        policy_hint,
    }
}

/// The risk of changing files with a tool such as `Edit` or `Write`: a path outside the
/// workspace or a file with secrets is `high`, more than 5 files is `medium`, and the rest is
/// `unknown`, because only reading is `low`.
pub fn assess_paths(paths: &[&str], ctx: &Context<'_>) -> Assessment {
    let workspace = self::paths::normalize(ctx.workspace);
    let mut verdict = rules::Verdict::default();
    let mut files = std::collections::BTreeSet::new();
    for path in paths {
        let Some(resolved) = self::paths::resolve(path, false, ctx.cwd, ctx.home) else {
            verdict.add(
                Risk::Unknown,
                format!("neohar could not resolve the path {path}."),
            );
            continue;
        };
        if !self::paths::is_inside(&resolved, &workspace) {
            verdict.add(
                Risk::High,
                format!(
                    "The change writes outside the workspace, to {}.",
                    self::paths::shown(&resolved)
                ),
            );
        }
        if self::paths::is_credential_or_leads_to_one(&resolved) {
            verdict.add(
                Risk::High,
                format!(
                    "The change is to a file that can hold secrets, {}.",
                    self::paths::shown(&resolved)
                ),
            );
        }
        if let Some(ch) = path.chars().find(|ch| crate::text::is_invisible(*ch)) {
            verdict.add(
                Risk::High,
                format!(
                    "The path has an invisible character, U+{:04X}. It can show another name than the name of the file.",
                    u32::from(ch)
                ),
            );
        }
        files.insert(resolved);
    }
    if files.len() > 5 {
        verdict.add(
            Risk::Medium,
            format!("The change is to {} files.", files.len()),
        );
    }
    if verdict.risk.is_none() {
        let reason = match files.len() {
            0 => "The change has no file.".to_owned(),
            1 => "The change is to one file in the workspace.".to_owned(),
            n => format!("The change is to {n} files in the workspace."),
        };
        verdict.add(Risk::Unknown, reason);
    }
    Assessment {
        risk: verdict.risk.unwrap_or(Risk::Unknown),
        reasons: verdict.reasons(),
    }
}

/// The words of the first command, after variables that are set in front of it.
fn first_argv(tokens: &[Token]) -> Vec<String> {
    let mut argv = Vec::new();
    let mut redirect_target = false;
    for token in tokens {
        match token {
            Token::Word(word) => {
                if redirect_target {
                    redirect_target = false;
                } else if argv.is_empty() && is_assignment_text(word) {
                    // Variables in front of the program are not part of its argv.
                } else {
                    argv.push(word.text.clone());
                }
            }
            Token::Redirect { .. } => redirect_target = true,
            Token::Op(Op::Open | Op::Close, _) => {}
            Token::Op(_, _) if !argv.is_empty() => break,
            Token::Op(_, _) | Token::Comment(_) => {}
        }
    }
    argv
}

fn is_assignment_text(word: &Word) -> bool {
    word.text.split_once('=').is_some_and(|(name, _)| {
        name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    })
}

/// More than one command, a redirection, a substitution or a here-document: the line does more
/// than its first words say.
fn is_compound(lexed: &lex::Lexed, source: &str) -> bool {
    !lexed.nested.is_empty()
        || source.contains("<<")
        || lexed
            .tokens
            .iter()
            .any(|t| matches!(t, Token::Op(_, _) | Token::Redirect { .. }))
}

/// Whether a path holds secrets: environment files, keys and credentials. The path
/// can be relative or absolute.
pub fn is_credential_path(path: &str) -> bool {
    paths::is_credential(Path::new(path))
}

/// Where an absolute path leads, after the symbolic links of the part that exists. A link whose
/// target does not exist yet leads to that target.
pub fn real_path(path: &Path) -> std::path::PathBuf {
    paths::real(path)
}

/// The pattern of the rule that would allow this command and calls like it: the program, its
/// subcommand and `:*`, such as `pnpm test:*`. A command of one word gets only itself, such as
/// `ls`, because `ls:*` would allow every call of the program.
///
/// `None` when no rule may cover the command: a program that destroys data or acts as another
/// user, a program that runs code that its arguments name (a shell, an interpreter, a wrapper such
/// as `env` or `timeout`, a runner of packages such as `npx`), a program by its path, and a second
/// word that is an option (`git -C x status`), a path, or a word with other characters than
/// letters, digits and `_-+@:.`.
pub fn rule_prefix(argv: &[String]) -> Option<String> {
    let program = argv.first()?.as_str();
    if !is_rule_word(program) || !may_have_rule(program) {
        return None;
    }
    let Some(second) = argv.get(1).map(String::as_str) else {
        return Some(program.to_owned());
    };
    if !is_rule_word(second) || tables::RUNS_ANY_SUB.contains(&(program, second)) {
        return None;
    }
    let first_part = |word: &str| word.split(':').next().unwrap_or(word).to_owned();
    let prefix = match (program, second) {
        ("npm" | "pnpm" | "yarn" | "bun", "run" | "run-script") => {
            match argv.get(2).map(String::as_str) {
                // `npm run` lists the scripts. The rule is only that.
                None => return Some(format!("{program} {second}")),
                Some(script) if is_rule_word(script) => {
                    format!("{program} run {}", first_part(script))
                }
                Some(_) => return None,
            }
        }
        ("pnpm" | "yarn" | "bun", script) => format!("{program} {}", first_part(script)),
        (_, sub) => format!("{program} {sub}"),
    };
    Some(format!("{prefix}:*"))
}

/// A word that a rule may name: letters, digits and `_-+@:.`, and not an option or a hidden name.
fn is_rule_word(word: &str) -> bool {
    !word.is_empty()
        && !word.starts_with(['-', '.', ':'])
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '@' | ':' | '.'))
}

/// Whether a rule may name the program at all.
fn may_have_rule(program: &str) -> bool {
    // `python3.12` and `node20` are `python` and `node`.
    let lower = program.to_ascii_lowercase();
    let unversioned = lower.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    !(tables::HIGH_PROGRAMS.contains(&program)
        || tables::NO_RULE.contains(&program)
        || tables::RUNS_ANY.contains(&lower.as_str())
        || tables::RUNS_ANY.contains(&unversioned)
        || program.starts_with("mkfs."))
}

/// A rule in words, such as "A rule for `pnpm test:*` can allow this command."
fn policy_hint(argv: &[String]) -> Option<String> {
    let prefix = rule_prefix(argv)?;
    Some(format!("A rule for `{prefix}` can allow this command."))
}

/// The line with its parts marked: the program, the flags, strings, operators and comments.
fn display(source: &str, tokens: &[Token]) -> Spans {
    let map = Utf16Map::new(source);
    let mut spans: Vec<Span> = Vec::new();
    let mut push = |range: &std::ops::Range<usize>, class: &'static str| {
        if range.start < range.end {
            spans.push(Span {
                start: map.at(range.start),
                end: map.at(range.end),
                class,
            });
        }
    };
    // The next word is a program: at the start, and after an operator.
    let mut command_position = true;
    let mut wrapper = false;
    let mut redirect_target = false;
    for token in tokens {
        match token {
            Token::Word(word) => {
                if redirect_target {
                    redirect_target = false;
                    if word.quoted {
                        push(&word.range, class::STRING);
                    }
                } else if command_position && is_assignment_text(word) {
                    let name_end = word.range.start + word.text.find('=').map_or(0, |n| n + 1);
                    push(
                        &(word.range.start..name_end.min(word.range.end)),
                        class::VARIABLE,
                    );
                } else if command_position
                    && tables::RESERVED.contains(&word.text.as_str())
                    && !word.quoted
                {
                    push(&word.range, class::KEYWORD);
                } else if command_position || wrapper {
                    let name = rules::basename(&word.text);
                    let flag = word.text.starts_with('-');
                    if flag {
                        // A flag of `sudo` or `env`: the program comes after it.
                        push(&word.range, class::OPTION);
                        continue;
                    } else if is_assignment_text(word) {
                        // `env FOO=1 program`
                    } else {
                        push(&word.range, class::COMMAND);
                        command_position = false;
                        wrapper = matches!(
                            name,
                            "sudo"
                                | "env"
                                | "time"
                                | "nohup"
                                | "exec"
                                | "command"
                                | "xargs"
                                | "nice"
                                | "timeout"
                        );
                        continue;
                    }
                } else if word.text.starts_with('-') && word.text.len() > 1 {
                    push(&word.range, class::OPTION);
                } else if word.quoted {
                    push(&word.range, class::STRING);
                }
                if !command_position {
                    wrapper = false;
                }
                // A variable or a keyword in front keeps the next word a program.
            }
            Token::Op(_, range) | Token::Redirect { range, .. } => {
                push(range, class::OPERATOR);
                if matches!(token, Token::Op(..)) {
                    command_position = true;
                    wrapper = false;
                } else {
                    redirect_target = true;
                }
            }
            Token::Comment(range) => push(range, class::COMMENT),
        }
    }
    Spans {
        text: source.to_owned(),
        spans,
    }
}

#[cfg(test)]
mod tests;
