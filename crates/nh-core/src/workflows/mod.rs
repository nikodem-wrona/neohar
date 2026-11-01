//! `workflow.*`: the workflows of the person and their executions.
//!
//! A workflow is a graph in a TOML file of `<config>/workflows/` ([`file`]): a manual trigger,
//! agent nodes and Report nodes, with the connections as the `inputs` of each node ([`graph`]). The
//! [`library`] reads and writes the files and fills `workflows`. The [`exec`] executor runs the
//! executions in a folder: it starts the agent of each node in a hidden tab when the inputs of the
//! node are done, takes the last message of its turn as its report, and writes `report.md` for a
//! Report node. It fills `workflow_executions`.
//!
//! The calls run off the actor. The executor writes its rows through the actor, in one transaction
//! with their events.

mod exec;
mod facts;
mod file;
mod graph;
mod library;
mod presets;
#[cfg(test)]
mod tests;
mod text;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use nh_proto::api::{
    calls::workflow::{
        WorkflowCopied, WorkflowExecutionList, WorkflowExecutionShown, WorkflowFacts, WorkflowList,
        WorkflowNotes, WorkflowReport,
    },
    common::{Empty, Harness},
    envelope::ErrorCode,
    ids::RunId,
    methods::{Call, Method},
};
use serde::Serialize;
use serde_json::Value;

pub(crate) use self::exec::Executor;
use self::exec::Request;
pub(crate) use self::{facts::FactsReader, library::Library};
use crate::{actor::Origin, error::RpcError, runtime::Inner};

pub(crate) use library::DIR_NAME;

/// What the runs of workflow nodes tell the executor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Signal {
    /// A turn of the run ended. The message is the last message of the agent, from the `Stop`
    /// hook.
    TurnEnded { run: RunId, message: Option<String> },
    /// The state of the run changed: an interaction, an error or the end of the process.
    Changed { run: RunId },
}

/// The largest report that `workflow.report` renders.
const REPORT_CAP: u64 = 256 * 1024;
/// The longest notes that `workflow.notes_from` gives.
const NOTES_CAP: usize = 4000;

/// Whether a method is a call of this module.
pub(crate) fn handles(method: &str) -> bool {
    matches!(
        Method::from_name(method),
        Some(
            Method::WorkflowListGet
                | Method::WorkflowShow
                | Method::WorkflowCreate
                | Method::WorkflowSave
                | Method::WorkflowRename
                | Method::WorkflowDuplicate
                | Method::WorkflowDelete
                | Method::WorkflowUndo
                | Method::WorkflowRedo
                | Method::WorkflowLayout
                | Method::WorkflowPresetsGet
                | Method::WorkflowFactsGet
                | Method::WorkflowExecute
                | Method::WorkflowNotesFrom
                | Method::WorkflowExecutions
                | Method::WorkflowExecution
                | Method::WorkflowReportGet
                | Method::WorkflowRetry
                | Method::WorkflowSkip
                | Method::WorkflowCancel
                | Method::WorkflowContinue
                | Method::WorkflowReveal
                | Method::WorkflowCopyToFolder
        )
    )
}

fn json_of<T: Serialize>(value: &T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|e| RpcError::internal(e.to_string()))
}

fn absolute(path: &str) -> Result<PathBuf, RpcError> {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(RpcError::invalid_params(format!(
            "{} is not an absolute path",
            path.display()
        )))
    }
}

/// Runs a call that [`handles`] accepted.
pub(crate) async fn run(
    inner: &Arc<Inner>,
    call: Call,
    _origin: Origin,
) -> Result<Value, RpcError> {
    let library = &inner.workflows;
    let executor = &inner.executor;
    executor.wait_loaded().await;
    match call {
        Call::WorkflowListGet(_) => json_of(&WorkflowList {
            workflows: library.values(),
        }),
        Call::WorkflowShow(p) => json_of(&library.show(&p.workflow)?),
        Call::WorkflowCreate(p) => json_of(&library.create(&p.name, p.preset.as_deref())?),
        Call::WorkflowSave(p) => json_of(&library.save(&p.workflow, &p.revision, p.graph)?),
        Call::WorkflowRename(p) => json_of(&library.rename(&p.workflow, &p.name)?),
        Call::WorkflowDuplicate(p) => json_of(&library.duplicate(&p.workflow)?),
        Call::WorkflowDelete(p) => {
            library.delete(&p.workflow)?;
            json_of(&Empty {})
        }
        Call::WorkflowUndo(p) => json_of(&library.step(&p.workflow, true)?),
        Call::WorkflowRedo(p) => json_of(&library.step(&p.workflow, false)?),
        Call::WorkflowLayout(p) => json_of(&library.layout(&p.workflow, &p.revision)?),
        Call::WorkflowPresetsGet(_) => json_of(&presets::presets()),
        Call::WorkflowFactsGet(p) => {
            let folder = absolute(&p.folder)?;
            let facts = executor.facts.facts(&folder).await;
            json_of(&WorkflowFacts {
                folder: folder.to_string_lossy().into_owned(),
                repo: facts.repo,
                branch: facts.branch,
                base: facts.base,
            })
        }
        Call::WorkflowExecute(p) => {
            let folder = absolute(&p.folder)?;
            let facts = executor.facts.facts(&folder).await;
            executor.request(Request::Execute(p, facts)).await
        }
        Call::WorkflowNotesFrom(p) => json_of(&WorkflowNotes {
            text: notes_from(inner, p.agent).await?,
        }),
        Call::WorkflowExecutions(p) => json_of(&WorkflowExecutionList {
            executions: executor.values(p.folder.as_deref().map(|f| f.trim_end_matches('/'))),
        }),
        Call::WorkflowExecution(p) => {
            let (execution, graph) = executor
                .execution(p.execution)
                .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no such execution"))?;
            json_of(&WorkflowExecutionShown { execution, graph })
        }
        Call::WorkflowReportGet(p) => {
            let path = executor.report_path(p.execution, p.node.as_deref())?;
            json_of(&report(&path).await?)
        }
        Call::WorkflowRetry(p) => executor.request(Request::Retry(p.execution, p.node)).await,
        Call::WorkflowSkip(p) => executor.request(Request::Skip(p.execution, p.node)).await,
        Call::WorkflowCancel(p) => executor.request(Request::Cancel(p.execution)).await,
        Call::WorkflowContinue(p) => {
            json_of(&executor.resume(p.execution, &p.node, p.window).await?)
        }
        Call::WorkflowReveal(p) => {
            executor.reveal(p.execution, &p.node, p.window).await?;
            json_of(&Empty {})
        }
        Call::WorkflowCopyToFolder(p) => {
            let (dir, folder) = executor
                .dir(p.execution)
                .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no such execution"))?;
            json_of(&copy_to_folder(Path::new(&dir), Path::new(&folder))?)
        }
        other => Err(RpcError::internal(format!(
            "not a workflow call: {other:?}"
        ))),
    }
}

/// A report rendered as HTML.
async fn report(path: &str) -> Result<WorkflowReport, RpcError> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || {
        let size = fs::metadata(&path)
            .map_err(|e| RpcError::new(ErrorCode::NotFound, format!("{path}: {e}")))?
            .len();
        let text = if size > REPORT_CAP {
            let bytes = fs::read(&path).map_err(|e| RpcError::internal(e.to_string()))?;
            let cut = usize::try_from(REPORT_CAP)
                .unwrap_or(usize::MAX)
                .min(bytes.len());
            String::from_utf8_lossy(&bytes[..cut]).into_owned()
        } else {
            fs::read_to_string(&path).map_err(|e| RpcError::internal(e.to_string()))?
        };
        let mut markdown = crate::rich_calls::markdown(&nh_rich::markdown(&text));
        markdown.truncated |= size > REPORT_CAP;
        Ok(WorkflowReport { path, markdown })
    })
    .await
    .map_err(|e| RpcError::internal(e.to_string()))?
}

/// `workflow.copy_to_folder`: the folder of an execution goes to `workflow-reports/` in the folder
/// of the execution.
fn copy_to_folder(dir: &Path, folder: &Path) -> Result<WorkflowCopied, RpcError> {
    let name = dir
        .file_name()
        .ok_or_else(|| RpcError::internal("the execution has no folder"))?;
    let target = folder.join("workflow-reports").join(name);
    fs::create_dir_all(&target).map_err(|e| RpcError::internal(e.to_string()))?;
    let entries = fs::read_dir(dir).map_err(|e| RpcError::internal(e.to_string()))?;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            fs::copy(&path, target.join(entry.file_name()))
                .map_err(|e| RpcError::internal(e.to_string()))?;
        }
    }
    Ok(WorkflowCopied {
        path: target.to_string_lossy().into_owned(),
    })
}

/// The prompts of an agent run, from its transcript, for the notes of an execution.
async fn notes_from(
    inner: &Arc<Inner>,
    agent: nh_proto::api::ids::Ref<RunId>,
) -> Result<String, RpcError> {
    let found = inner
        .actor
        .update(move |parts| {
            let id = parts.state.runs.resolve(&agent)?;
            let run = parts.state.runs.by_id.get(&id)?;
            Some((run.harness, run.transcript_path.clone()))
        })
        .await
        .map_err(|e| RpcError::internal(e.to_string()))?;
    let Some((harness, Some(path))) = found else {
        return Err(RpcError::new(
            ErrorCode::NotFound,
            "the agent has no transcript",
        ));
    };
    tokio::task::spawn_blocking(move || prompts(harness, Path::new(&path)))
        .await
        .map_err(|e| RpcError::internal(e.to_string()))
}

fn prompts(harness: Harness, path: &Path) -> String {
    use nh_agents::transcript::{Format, Kind, parse_line};
    let format = match harness {
        Harness::Claude => Format::Claude,
        Harness::Codex => Format::Codex,
    };
    let Ok(text) = fs::read_to_string(path) else {
        return String::new();
    };
    let mut prompts = Vec::new();
    let mut offset = 0u64;
    for line in text.lines() {
        for entry in parse_line(format, line, offset) {
            if let Kind::Prompt { text } = entry.kind
                && !entry.sidechain
                && !text.trim().is_empty()
            {
                prompts.push(text.trim().to_owned());
            }
        }
        offset += line.len() as u64 + 1;
    }
    let mut out = prompts.join("\n\n");
    if out.chars().count() > NOTES_CAP {
        // The latest prompts say the most.
        let skip = out.chars().count() - NOTES_CAP;
        out = out.chars().skip(skip).collect();
    }
    out
}
