//! The payload of an interaction, rendered once when it opens.
//!
//! The adapter gives the payload as it read it from the harness: a tool name, a command, some
//! paths, the input of the tool. This module turns it into what the card shows. A shell command
//! gets its words, its risk and the reasons for the risk. A change of files gets a diff. A plan
//! gets sanitized HTML. The core does this once, so every client shows the same, and the UI needs
//! no formatting code.

use std::path::{Path, PathBuf};

use nh_proto::api::{
    common::Risk,
    model::{
        ApprovalPayload, InteractionKind, InteractionPayload, NoticePayload, PlanPayload, Question,
        QuestionOption, QuestionPayload,
    },
    time::Timestamp,
};
use nh_rich::{
    cap_output,
    command::{self, Context},
    diff::{self, Change, Edit},
    project::Project,
};
use serde_json::Value;

use crate::rich_calls;

/// What the rendering needs to know about the place of the run.
pub(crate) struct RenderCtx<'a> {
    pub workspace: &'a Path,
    pub cwd: &'a Path,
    pub home: Option<&'a Path>,
    pub project: &'a Project,
    pub network_allow: &'a [String],
    /// The last lines of the pane, for the strip of context.
    pub peek: Vec<String>,
    /// Reads the content of a file for a diff. `None` when it cannot be read.
    pub read_file: &'a dyn Fn(&Path) -> Option<String>,
}

/// What the policy needs to know about an approval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Subject {
    pub tool: String,
    pub command: Option<String>,
    /// Relative to the workspace root. A path outside the root is absolute.
    pub paths: Vec<String>,
    /// The first command of the line, as the card shows it.
    pub argv: Vec<String>,
    pub workspace: String,
}

/// The result of the rendering.
pub(crate) struct Rendered {
    pub title: String,
    pub risk: Risk,
    pub payload: InteractionPayload,
    /// What the policy sees. Only an approval has it.
    pub subject: Option<Subject>,
}

/// The tools of a harness that run a shell command.
const SHELL_TOOLS: &[&str] = &["Bash", "exec_command", "shell", "local_shell", "BashOutput"];

/// The tools that change files with a diff of text.
const EDIT_TOOLS: &[&str] = &["Edit", "MultiEdit", "Write"];

/// A longer input than this is cut: a card is not a place for a megabyte.
const INPUT_CAP: usize = 16 * 1024;

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

/// A title of at most `max` characters, on one line.
fn one_line(text: &str, max: usize) -> String {
    let first = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    let mut out: String = first.trim().chars().take(max).collect();
    if first.trim().chars().count() > max || text.trim().lines().count() > 1 {
        out.push('…');
    }
    out
}

pub(crate) fn render(kind: InteractionKind, raw: &Value, ctx: &RenderCtx<'_>) -> Rendered {
    match kind {
        InteractionKind::Approval => approval(raw, ctx),
        InteractionKind::Question => question(raw),
        InteractionKind::PlanReview => plan(raw),
        InteractionKind::Notice => notice(raw),
    }
}

fn rules_context<'a>(ctx: &'a RenderCtx<'a>) -> Context<'a> {
    Context {
        workspace: ctx.workspace,
        cwd: ctx.cwd,
        home: ctx.home,
        project: ctx.project,
        network_allow: ctx.network_allow,
    }
}

fn approval(raw: &Value, ctx: &RenderCtx<'_>) -> Rendered {
    let tool = text(raw, "tool").unwrap_or("tool").to_owned();
    let input = raw.get("input").cloned().unwrap_or(Value::Null);
    let paths: Vec<String> = raw
        .get("paths")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let command_text = command_of(raw);
    if let Some(line) = command_text.as_deref()
        && (SHELL_TOOLS.contains(&tool.as_str()) || paths.is_empty())
    {
        let rules = rules_context(ctx);
        let explained = command::explain(line, &rules);
        let risk = rich_calls::risk(explained.risk);
        let subject = Subject {
            tool: tool.clone(),
            command: Some(line.to_owned()),
            paths: relative_paths(&paths, ctx),
            argv: explained.argv.clone(),
            workspace: ctx.workspace.display().to_string(),
        };
        return Rendered {
            title: format!("Run `{}`", one_line(line, 80)),
            risk,
            payload: InteractionPayload::Approval(Box::new(ApprovalPayload {
                tool,
                command: Some(rich_calls::command(&explained)),
                cwd: Some(ctx.cwd.display().to_string()),
                paths,
                diff: None,
                input: None,
                peek: ctx.peek.clone(),
                policy_note: None,
            })),
            subject: Some(subject),
        };
    }
    let (title, diff_value) = if EDIT_TOOLS.contains(&tool.as_str()) {
        (
            format!(
                "{} `{}`",
                verb(&tool),
                paths.first().map_or("file", String::as_str)
            ),
            file_diff(&tool, &input, &paths, ctx),
        )
    } else if paths.is_empty() {
        (tool.clone(), None)
    } else {
        (format!("{tool} `{}`", paths.join("`, `")), None)
    };
    let risk = if paths.is_empty() {
        Risk::Unknown
    } else {
        let list: Vec<&str> = paths.iter().map(String::as_str).collect();
        rich_calls::risk(command::assess_paths(&list, &rules_context(ctx)).risk)
    };
    let subject = Subject {
        tool: tool.clone(),
        command: None,
        paths: relative_paths(&paths, ctx),
        argv: Vec::new(),
        workspace: ctx.workspace.display().to_string(),
    };
    Rendered {
        title,
        risk,
        subject: Some(subject),
        payload: InteractionPayload::Approval(Box::new(ApprovalPayload {
            tool,
            command: None,
            cwd: Some(ctx.cwd.display().to_string()),
            paths,
            input: if diff_value.is_none() {
                capped(&input)
            } else {
                None
            },
            diff: diff_value,
            peek: ctx.peek.clone(),
            policy_note: None,
        })),
    }
}

/// The paths relative to the root of the workspace. A path outside the root stays absolute, so
/// that no glob of a rule matches it.
fn relative_paths(paths: &[String], ctx: &RenderCtx<'_>) -> Vec<String> {
    let root = lexical(ctx.workspace);
    paths
        .iter()
        .map(|path| {
            let full = lexical(&absolute(ctx, path));
            match full.strip_prefix(&root) {
                Ok(rest) if !rest.as_os_str().is_empty() => rest.to_string_lossy().into_owned(),
                _ => full.to_string_lossy().into_owned(),
            }
        })
        .collect()
}

/// The path without `.` and `..`, without looking at the disk.
fn lexical(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn verb(tool: &str) -> &'static str {
    if tool == "Write" { "Write" } else { "Edit" }
}

/// The shell command of an approval: a string, or the words of a command.
fn command_of(raw: &Value) -> Option<String> {
    match raw.get("command")? {
        Value::String(line) if !line.trim().is_empty() => Some(line.clone()),
        Value::Array(words) => {
            let words: Vec<&str> = words.iter().filter_map(Value::as_str).collect();
            (!words.is_empty()).then(|| words.join(" "))
        }
        _ => None,
    }
}

/// The input of a tool without a card of its own, cut at a cap.
fn capped(input: &Value) -> Option<Value> {
    if input.is_null() {
        return None;
    }
    let text = input.to_string();
    if text.len() <= INPUT_CAP {
        return Some(input.clone());
    }
    let cut = nh_rich::cap(&text, INPUT_CAP);
    Some(Value::String(format!("{}…", cut.text)))
}

fn absolute(ctx: &RenderCtx<'_>, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        ctx.cwd.join(path)
    }
}

/// The diff of a tool that changes a file with text.
fn file_diff(
    tool: &str,
    input: &Value,
    paths: &[String],
    ctx: &RenderCtx<'_>,
) -> Option<nh_proto::api::rich::Diff> {
    let path = paths.first()?;
    let file = (ctx.read_file)(&absolute(ctx, path));
    let result = match tool {
        "Edit" => {
            let edit = Edit {
                old: text(input, "old_string")?,
                new: text(input, "new_string")?,
                replace_all: input
                    .get("replace_all")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            };
            diff::diff(&[Change::Edits {
                path,
                edits: &[edit],
                file: file.as_deref(),
            }])
        }
        "MultiEdit" => {
            let edits: Vec<Edit<'_>> = input
                .get("edits")?
                .as_array()?
                .iter()
                .filter_map(|edit| {
                    Some(Edit {
                        old: text(edit, "old_string")?,
                        new: text(edit, "new_string")?,
                        replace_all: edit
                            .get("replace_all")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                })
                .collect();
            diff::diff(&[Change::Edits {
                path,
                edits: &edits,
                file: file.as_deref(),
            }])
        }
        "Write" => diff::diff(&[Change::Write {
            path,
            before: file.as_deref(),
            after: text(input, "content")?,
        }]),
        _ => return None,
    };
    Some(rich_calls::diff(&result))
}

fn question(raw: &Value) -> Rendered {
    // An elicitation of an MCP server has a message and a schema. It is shown as one question.
    if text(raw, "source") == Some("elicitation") {
        let message = text(raw, "message")
            .unwrap_or("A tool asks for input")
            .to_owned();
        let server = text(raw, "server").unwrap_or("an MCP server");
        return Rendered {
            subject: None,
            title: one_line(&message, 80),
            risk: Risk::Unknown,
            payload: InteractionPayload::Question(QuestionPayload {
                questions: vec![Question {
                    question: message,
                    header: Some(server.to_owned()),
                    options: Vec::new(),
                    multi_select: false,
                }],
                context: None,
            }),
        };
    }
    let questions: Vec<Question> = raw
        .get("questions")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|item| {
                    Some(Question {
                        question: text(item, "question")?.to_owned(),
                        header: text(item, "header").map(str::to_owned),
                        options: item
                            .get("options")
                            .and_then(Value::as_array)
                            .map(|options| {
                                options
                                    .iter()
                                    .filter_map(|option| {
                                        Some(QuestionOption {
                                            label: text(option, "label")?.to_owned(),
                                            description: text(option, "description")
                                                .map(str::to_owned),
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                        multi_select: item
                            .get("multiSelect")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let title = questions
        .first()
        .map_or_else(|| "A question".to_owned(), |q| one_line(&q.question, 80));
    Rendered {
        subject: None,
        title,
        risk: Risk::Low,
        payload: InteractionPayload::Question(QuestionPayload {
            questions,
            context: None,
        }),
    }
}

fn plan(raw: &Value) -> Rendered {
    let markdown = nh_rich::markdown(text(raw, "plan_md").unwrap_or(""));
    Rendered {
        subject: None,
        title: "Review the plan".to_owned(),
        risk: Risk::Low,
        payload: InteractionPayload::PlanReview(PlanPayload {
            plan: rich_calls::markdown(&markdown),
            plan_path: text(raw, "plan_file").map(str::to_owned),
        }),
    }
}

fn notice(raw: &Value) -> Rendered {
    let body = text(raw, "text")
        .or_else(|| text(raw, "message"))
        .unwrap_or("")
        .to_owned();
    let capped = cap_output(&body).text.to_owned();
    let title = one_line(&capped, 80);
    Rendered {
        subject: None,
        title: if title.is_empty() {
            "Notice".to_owned()
        } else {
            title
        },
        risk: Risk::Low,
        payload: InteractionPayload::Notice(NoticePayload {
            text: capped,
            resets_at: raw.get("resets_at").and_then(Value::as_i64).map(Timestamp),
        }),
    }
}

#[cfg(test)]
mod tests;
