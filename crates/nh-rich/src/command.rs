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
        display: display(source, &lexed.tokens),
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
                    resolved.display()
                ),
            );
        }
        if self::paths::is_credential(&resolved) {
            verdict.add(
                Risk::High,
                format!(
                    "The change is to a file that can hold secrets, {}.",
                    resolved.display()
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

/// The prefix that a rule for this command would match, such as `pnpm test:*`: the program, its
/// subcommand, and `:*`. `None` for a program that no rule should allow.
pub fn rule_prefix(argv: &[String]) -> Option<String> {
    let program = rules::basename(argv.first()?);
    if program.is_empty()
        || tables::HIGH_PROGRAMS.contains(&program)
        || tables::NO_RULE.contains(&program)
        || program.starts_with("mkfs.")
    {
        return None;
    }
    let second = argv.get(1).map(String::as_str).filter(|w| {
        !w.starts_with('-') && !w.contains('/') && !w.contains('=') && !w.starts_with('.')
    });
    let prefix = match (program, second) {
        ("npm" | "pnpm" | "yarn" | "bun", Some("run" | "run-script")) => {
            let script = argv
                .get(2)
                .map(String::as_str)
                .filter(|w| !w.starts_with('-'));
            match script {
                Some(script) => format!(
                    "{program} run {}",
                    script.split(':').next().unwrap_or(script)
                ),
                None => format!("{program} run"),
            }
        }
        ("pnpm" | "yarn" | "bun", Some(script)) => {
            format!("{program} {}", script.split(':').next().unwrap_or(script))
        }
        (_, Some(sub)) => format!("{program} {sub}"),
        (_, None) => program.to_owned(),
    };
    Some(format!("{prefix}:*"))
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
