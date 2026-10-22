//! `rich.diff`, `rich.markdown` and `rich.highlight`.
//!
//! The UI asks for content that no interaction carries: a diff of two texts, a plan in markdown, a
//! piece of code. `nh-rich` makes it. The work is CPU work on text of any size, so it runs on the
//! blocking pool and never on the state actor or on a task of the runtime. The cards of the
//! interactions get their content from the same functions when the interaction opens.
//!
//! `nh-rich` has no workspace dependency, so this module maps its types to the types of the API.

use std::sync::Arc;

use nh_proto::api::{
    common::Risk,
    methods::{Call, Method},
    rich,
};
use serde_json::{Value, to_value};

use crate::{error::RpcError, runtime::Inner};

/// Whether this module runs the method.
pub(crate) fn handles(method: &str) -> bool {
    matches!(
        Method::from_name(method),
        Some(Method::RichDiff | Method::RichMarkdown | Method::RichHighlight)
    )
}

/// Runs a call that [`handles`] accepted.
pub(crate) async fn run(inner: &Arc<Inner>, call: Call) -> Result<Value, RpcError> {
    let work = inner
        .runtime
        .spawn_blocking(move || -> Result<Value, RpcError> {
            match call {
                Call::RichDiff(p) => to_value(diff(&nh_rich::diff::between(
                    &p.a,
                    &p.b,
                    p.path.as_deref(),
                    p.lang.as_deref(),
                ))),
                Call::RichMarkdown(p) => to_value(markdown(&nh_rich::markdown(&p.text))),
                Call::RichHighlight(p) => {
                    to_value(spans(&nh_rich::highlight::highlight(&p.text, &p.lang)))
                }
                _ => return Err(RpcError::internal("not a rich call")),
            }
            .map_err(|e| RpcError::internal(e.to_string()))
        });
    work.await
        .map_err(|e| RpcError::internal(format!("the rich task failed: {e}")))?
}

pub(crate) fn spans(spans: &nh_rich::Spans) -> rich::Spans {
    rich::Spans {
        text: spans.text.clone(),
        spans: spans
            .spans
            .iter()
            .map(|span| (span.start, span.end, span.class.to_owned()))
            .collect(),
    }
}

pub(crate) fn markdown(markdown: &nh_rich::Markdown) -> rich::Markdown {
    rich::Markdown {
        html: markdown.html.clone(),
        truncated: markdown.truncated,
    }
}

pub(crate) fn diff(diff: &nh_rich::diff::Diff) -> rich::Diff {
    use nh_rich::diff::{FileStatus, LineKind};
    rich::Diff {
        files: diff
            .files
            .iter()
            .map(|file| rich::FileDiff {
                path: file.path.clone(),
                status: match file.status {
                    FileStatus::Modified => rich::FileStatus::Modified,
                    FileStatus::Added => rich::FileStatus::Added,
                    FileStatus::Deleted => rich::FileStatus::Deleted,
                    FileStatus::Renamed => rich::FileStatus::Renamed,
                },
                hunks: file
                    .hunks
                    .iter()
                    .map(|hunk| rich::Hunk {
                        header: hunk.header.clone(),
                        lines: hunk
                            .lines
                            .iter()
                            .map(|line| rich::DiffLine {
                                kind: match line.kind {
                                    LineKind::Ctx => rich::LineKind::Ctx,
                                    LineKind::Add => rich::LineKind::Add,
                                    LineKind::Del => rich::LineKind::Del,
                                },
                                old: line.old,
                                new: line.new,
                                text: spans(&line.text),
                                intra: line.intra.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
                stats: rich::DiffStats {
                    add: file.stats.add,
                    del: file.stats.del,
                },
                truncated: file.truncated,
            })
            .collect(),
    }
}

/// The risk of the rules, as the API names it.
pub(crate) fn risk(risk: nh_rich::command::Risk) -> Risk {
    use nh_rich::command::Risk as Rules;
    match risk {
        Rules::Low => Risk::Low,
        Rules::Medium => Risk::Medium,
        Rules::High => Risk::High,
        Rules::Unknown => Risk::Unknown,
    }
}

pub(crate) fn command(command: &nh_rich::command::Command) -> rich::Command {
    rich::Command {
        argv: command.argv.clone(),
        display: spans(&command.display),
        cwd: command.cwd.clone(),
        risk: risk(command.risk),
        reasons: command.reasons.clone(),
        policy_hint: command.policy_hint.clone(),
    }
}

#[cfg(test)]
mod tests;
