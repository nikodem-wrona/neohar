//! The workflow files of the person: `<config>/workflows/*.toml`. The core is the only writer. It
//! keeps the versions before each save in memory for undo and redo, and fills the `workflows`
//! projection. It watches the folder, so an edit by hand shows at once.

use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use nh_proto::api::{
    calls::workflow::{WorkflowGraph, WorkflowShown},
    envelope::ErrorCode,
    model::WorkflowValue,
    projections::Projection,
};

use super::{file, graph, presets};
use crate::{error::RpcError, projection::ProjectionHub, watch};

/// The folder of the workflow files, in the config folder.
pub(crate) const DIR_NAME: &str = "workflows";
/// The versions that undo keeps for each workflow.
const HISTORY: usize = 50;
/// The quiet time after a change of the folder before the core reads it again.
const DEBOUNCE: Duration = Duration::from_millis(200);

#[derive(Default)]
struct History {
    /// The texts before each save, the oldest first.
    past: Vec<String>,
    /// The texts that undo took back, the newest last.
    future: Vec<String>,
}

/// The workflow files.
pub(crate) struct Library {
    dir: PathBuf,
    hub: Arc<ProjectionHub>,
    history: Mutex<HashMap<String, History>>,
    /// The rows of `workflows` that are out, to remove the ones that went.
    published: Mutex<Vec<String>>,
}

fn not_found(workflow: &str) -> RpcError {
    RpcError::new(ErrorCode::NotFound, format!("no workflow {workflow}"))
}

fn io_error(path: &Path, error: &std::io::Error) -> RpcError {
    RpcError::internal(format!("{}: {error}", path.display()))
}

impl Library {
    pub(crate) fn new(dir: PathBuf, hub: Arc<ProjectionHub>) -> Arc<Self> {
        Arc::new(Self {
            dir,
            hub,
            history: Mutex::new(HashMap::new()),
            published: Mutex::new(Vec::new()),
        })
    }

    fn history(&self) -> MutexGuard<'_, HashMap<String, History>> {
        self.history.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn path(&self, workflow: &str) -> PathBuf {
        self.dir.join(format!("{workflow}.{}", file::EXTENSION))
    }

    fn checked_path(&self, workflow: &str) -> Result<PathBuf, RpcError> {
        if !file::is_stem(workflow) {
            return Err(RpcError::invalid_params(format!(
                "\"{workflow}\" is not the name of a workflow file"
            )));
        }
        Ok(self.path(workflow))
    }

    /// The text of a workflow file.
    pub(crate) fn text(&self, workflow: &str) -> Result<String, RpcError> {
        let path = self.checked_path(workflow)?;
        fs::read_to_string(&path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => not_found(workflow),
            _ => io_error(&path, &error),
        })
    }

    /// The graph of a workflow file. A file that is not valid TOML is an error that tells the line.
    pub(crate) fn graph(&self, workflow: &str) -> Result<(String, WorkflowGraph), RpcError> {
        let text = self.text(workflow)?;
        let graph = file::parse(&text).map_err(|error| {
            RpcError::new(
                ErrorCode::InvalidParams,
                format!(
                    "The file {} has an error. Correct it by hand. {error}",
                    self.path(workflow).display()
                ),
            )
        })?;
        Ok((text, graph))
    }

    fn shown(&self, workflow: &str, text: &str, graph: WorkflowGraph) -> WorkflowShown {
        let history = self.history();
        let entry = history.get(workflow);
        WorkflowShown {
            workflow: workflow.to_owned(),
            revision: file::revision(text),
            problems: graph::problems(&graph),
            graph,
            can_undo: entry.is_some_and(|h| !h.past.is_empty()),
            can_redo: entry.is_some_and(|h| !h.future.is_empty()),
            path: self.path(workflow).to_string_lossy().into_owned(),
        }
    }

    /// `workflow.show`.
    pub(crate) fn show(&self, workflow: &str) -> Result<WorkflowShown, RpcError> {
        let (text, graph) = self.graph(workflow)?;
        Ok(self.shown(workflow, &text, graph))
    }

    /// Writes a file through a file beside it, so a reader never sees half of it.
    fn write(&self, workflow: &str, text: &str) -> Result<(), RpcError> {
        fs::create_dir_all(&self.dir).map_err(|e| io_error(&self.dir, &e))?;
        let path = self.path(workflow);
        let temporary = self
            .dir
            .join(format!(".{workflow}.{}.tmp", std::process::id()));
        let written = fs::File::create(&temporary)
            .and_then(|mut out| out.write_all(text.as_bytes()).and_then(|()| out.sync_all()))
            .and_then(|()| fs::rename(&temporary, &path));
        if let Err(error) = written {
            let _ = fs::remove_file(&temporary);
            return Err(io_error(&path, &error));
        }
        self.publish();
        Ok(())
    }

    /// A file name that no workflow has, from a name.
    fn free_stem(&self, name: &str) -> String {
        let base = file::slug(name);
        let mut stem = base.clone();
        let mut n = 2;
        while self.path(&stem).exists() {
            stem = format!("{base}-{n}");
            n += 1;
        }
        stem
    }

    /// `workflow.create`.
    pub(crate) fn create(
        &self,
        name: &str,
        preset: Option<&str>,
    ) -> Result<WorkflowShown, RpcError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(RpcError::invalid_params("the name is empty"));
        }
        let graph = presets::graph(name, preset).ok_or_else(|| {
            RpcError::new(
                ErrorCode::NotFound,
                format!("no preset workflow {}", preset.unwrap_or_default()),
            )
        })?;
        let stem = self.free_stem(name);
        let text = file::render(&graph);
        self.write(&stem, &text)?;
        Ok(self.shown(&stem, &text, graph))
    }

    /// `workflow.save`: writes the graph if the file is still at `revision`.
    pub(crate) fn save(
        &self,
        workflow: &str,
        revision: &str,
        graph: WorkflowGraph,
    ) -> Result<WorkflowShown, RpcError> {
        let before = self.text(workflow)?;
        if file::revision(&before) != revision {
            return Err(RpcError::new(
                ErrorCode::Conflict,
                "the workflow changed since you loaded it",
            ));
        }
        if graph.nodes.len() > graph::NODES_MAX {
            return Err(RpcError::invalid_params(format!(
                "a workflow has at most {} nodes",
                graph::NODES_MAX
            )));
        }
        let text = file::render(&graph);
        if text != before {
            self.write(workflow, &text)?;
            let mut history = self.history();
            let entry = history.entry(workflow.to_owned()).or_default();
            entry.past.push(before);
            if entry.past.len() > HISTORY {
                entry.past.remove(0);
            }
            entry.future.clear();
        }
        Ok(self.shown(workflow, &text, graph))
    }

    /// `workflow.undo` and `workflow.redo`.
    pub(crate) fn step(&self, workflow: &str, back: bool) -> Result<WorkflowShown, RpcError> {
        let now = self.text(workflow)?;
        let target = {
            let mut history = self.history();
            let entry = history.entry(workflow.to_owned()).or_default();
            let (from, to) = if back {
                (&mut entry.past, &mut entry.future)
            } else {
                (&mut entry.future, &mut entry.past)
            };
            let Some(target) = from.pop() else {
                return Err(RpcError::new(
                    ErrorCode::NotFound,
                    if back {
                        "there is nothing to undo"
                    } else {
                        "there is nothing to redo"
                    },
                ));
            };
            to.push(now);
            target
        };
        self.write(workflow, &target)?;
        let graph = file::parse(&target).map_err(RpcError::internal)?;
        Ok(self.shown(workflow, &target, graph))
    }

    /// `workflow.layout`: "Tidy up".
    pub(crate) fn layout(&self, workflow: &str, revision: &str) -> Result<WorkflowShown, RpcError> {
        let (_, graph) = self.graph(workflow)?;
        self.save(workflow, revision, graph::tidy(&graph))
    }

    /// `workflow.rename`: a new name, and a file name that comes from it when that one is free.
    pub(crate) fn rename(&self, workflow: &str, name: &str) -> Result<WorkflowShown, RpcError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(RpcError::invalid_params("the name is empty"));
        }
        let (text, mut graph) = self.graph(workflow)?;
        graph.name = name.to_owned();
        let saved = self.save(workflow, &file::revision(&text), graph)?;
        let wanted = file::slug(name);
        if wanted == workflow || self.path(&wanted).exists() {
            return Ok(saved);
        }
        fs::rename(self.path(workflow), self.path(&wanted))
            .map_err(|e| io_error(&self.path(workflow), &e))?;
        {
            let mut history = self.history();
            if let Some(entry) = history.remove(workflow) {
                history.insert(wanted.clone(), entry);
            }
        }
        self.publish();
        self.show(&wanted)
    }

    /// `workflow.duplicate`.
    pub(crate) fn duplicate(&self, workflow: &str) -> Result<WorkflowShown, RpcError> {
        let (_, mut graph) = self.graph(workflow)?;
        graph.name = format!("{} copy", graph.name);
        let stem = self.free_stem(&graph.name);
        let text = file::render(&graph);
        self.write(&stem, &text)?;
        Ok(self.shown(&stem, &text, graph))
    }

    /// `workflow.delete`.
    pub(crate) fn delete(&self, workflow: &str) -> Result<(), RpcError> {
        let path = self.checked_path(workflow)?;
        fs::remove_file(&path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => not_found(workflow),
            _ => io_error(&path, &error),
        })?;
        self.history().remove(workflow);
        self.publish();
        Ok(())
    }

    /// The rows of `workflows`, by the name of the file.
    pub(crate) fn values(&self) -> Vec<WorkflowValue> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut stems: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                (path.extension().and_then(|e| e.to_str()) == Some(file::EXTENSION))
                    .then(|| path.file_stem()?.to_str().map(str::to_owned))
                    .flatten()
            })
            .filter(|stem| file::is_stem(stem))
            .collect();
        stems.sort();
        stems
            .into_iter()
            .filter_map(|stem| {
                let text = fs::read_to_string(self.path(&stem)).ok()?;
                let revision = file::revision(&text);
                Some(match file::parse(&text) {
                    Ok(graph) => WorkflowValue {
                        workflow: stem,
                        name: graph.name.clone(),
                        nodes: u32::try_from(graph.nodes.len()).unwrap_or(u32::MAX),
                        problems: u32::try_from(graph::problems(&graph).len()).unwrap_or(u32::MAX),
                        revision,
                    },
                    Err(_) => WorkflowValue {
                        name: stem.clone(),
                        workflow: stem,
                        nodes: 0,
                        problems: 1,
                        revision,
                    },
                })
            })
            .collect()
    }

    /// Fills `workflows` from the folder.
    pub(crate) fn publish(&self) {
        let values = self.values();
        let keys: Vec<String> = values.iter().map(|v| v.workflow.clone()).collect();
        let mut published = self
            .published
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for gone in published.iter().filter(|key| !keys.contains(key)) {
            self.hub.remove(Projection::Workflows, gone);
        }
        for value in values {
            if let Ok(json) = serde_json::to_value(&value) {
                self.hub.set(Projection::Workflows, &value.workflow, json);
            }
        }
        self.hub.set_order(Projection::Workflows, &keys);
        *published = keys;
    }

    /// Watches the folder and fills `workflows` again after each burst of changes. The watch stops
    /// when the result drops.
    pub(crate) fn watch(self: &Arc<Self>) -> std::io::Result<watch::DirWatch> {
        fs::create_dir_all(&self.dir)?;
        let (sender, receiver) = std::sync::mpsc::channel::<()>();
        let watched = watch::watch_dir(watch::Kind::of_build(), &self.dir, move |_| {
            let _ = sender.send(());
        })?;
        let this = Arc::downgrade(self);
        std::thread::Builder::new()
            .name("nh-workflow-watch".to_owned())
            .spawn(move || {
                while receiver.recv().is_ok() {
                    while receiver.recv_timeout(DEBOUNCE).is_ok() {}
                    match this.upgrade() {
                        Some(library) => library.publish(),
                        None => return,
                    }
                }
            })?;
        Ok(watched)
    }
}
