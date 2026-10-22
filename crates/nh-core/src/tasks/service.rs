//! The tasks and the git work behind them.
//!
//! `task.create` keeps a draft: the repository, the title, the agent and its prompt, and the
//! files. `task.update` changes a draft, and `task.delete` removes it. `task.start` is a job with
//! steps: the name, the worktree, the ports, the files, the setup commands, a tab in the workspace
//! of the repository, and the agent. A step before the worktree that fails makes the task a draft
//! again. A later step that fails ends the job, and what it made before stays in a state that
//! `task.close` can undo.

use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, PoisonError, atomic::AtomicU64},
    time::Duration,
};

use nh_proto::api::{
    calls::agent::{AgentStart, Placement, TaskCreate, TaskCreated, TaskUpdate, Upload},
    ids::{JobId, PaneId, Ref, TaskId, WorkspaceId},
    methods::Call,
    model::{JobStatus, TaskStatus},
};
use nh_store::{AttachmentRow, TaskRow};
use nh_tasks::{
    Git, GitError, Worktree, ports,
    setup::{self, Outcome},
    slug,
};
use serde_json::{Value, json};
use tokio::runtime::Handle;

use super::{Task, changes::Follows, files, harness_name, harness_of, status_name};
use crate::{
    ErrorCode,
    actor::{ActorHandle, Origin},
    agents::Control,
    error::RpcError,
    jobs::{JobHandle, Jobs},
    persist::now_ms,
    projection::ProjectionHub,
};

/// How long the core waits before it counts the lines of a task again, so that a burst of changes
/// is one look.
pub(super) const DIFF_DEBOUNCE: Duration = Duration::from_secs(2);

pub(crate) struct TaskService {
    pub(super) handle: Handle,
    jobs: Arc<Jobs>,
    actor: OnceLock<ActorHandle>,
    control: Arc<Control>,
    pub(super) git: Git,
    shell: String,
    pub(super) env: Vec<(String, String)>,
    /// The tasks whose diff is going to be counted.
    pending: Mutex<HashSet<TaskId>>,
    /// The number of the last count of changes.
    pub(super) count_seq: AtomicU64,
    /// The tasks whose `changes:<task>` a window follows, with their watches.
    pub(super) follows: Mutex<Follows>,
    /// The projections, to know whether a window still follows `changes:<task>`.
    pub(super) hub: OnceLock<Arc<ProjectionHub>>,
}

pub(super) fn gone(error: impl std::fmt::Display) -> RpcError {
    RpcError::internal(error.to_string())
}

fn git_error(error: GitError) -> RpcError {
    RpcError::new(ErrorCode::GitFailed, error.to_string())
}

/// `~` and `~/…` as the home directory.
pub(super) fn expand(path: &str, env: &[(String, String)]) -> PathBuf {
    let home = env
        .iter()
        .find(|(name, _)| name == "HOME")
        .map(|(_, value)| PathBuf::from(value));
    match (path.strip_prefix("~/"), path, home) {
        (Some(rest), _, Some(home)) => home.join(rest),
        (None, "~", Some(home)) => home,
        _ => PathBuf::from(path),
    }
}

impl TaskService {
    pub(crate) fn new(
        handle: Handle,
        jobs: Arc<Jobs>,
        control: Arc<Control>,
        base_env: &[(String, String)],
    ) -> Arc<Self> {
        let shell = base_env
            .iter()
            .find(|(name, _)| name == "SHELL")
            .map_or_else(|| "/bin/sh".to_owned(), |(_, value)| value.clone());
        Arc::new(Self {
            handle,
            jobs,
            actor: OnceLock::new(),
            control,
            git: Git::new(base_env.to_vec()),
            shell,
            env: base_env.to_vec(),
            pending: Mutex::new(HashSet::new()),
            count_seq: AtomicU64::new(0),
            follows: Mutex::new(Follows::default()),
            hub: OnceLock::new(),
        })
    }

    pub(crate) fn bind(&self, actor: ActorHandle) {
        let _ = self.actor.set(actor);
    }

    /// The projections. A first follower of `changes:<task>` causes a count, and the watches of
    /// the task run while one follows. A window that follows `changes_for:<window>` or
    /// `changes_head:<window>` starts its Changes tab.
    pub(crate) fn bind_projections(self: &Arc<Self>, hub: &Arc<ProjectionHub>) {
        let _ = self.hub.set(Arc::clone(hub));
        let service = Arc::downgrade(self);
        hub.on_subscribe(Box::new(move |projection| {
            use nh_proto::api::projections::Projection;
            let Some(service) = service.upgrade() else {
                return;
            };
            match projection {
                Projection::Changes(task) => service.followed(task),
                Projection::ChangesFor(window) | Projection::ChangesHead(window) => {
                    service.window_followed(window);
                }
                _ => {}
            }
        }));
    }

    pub(super) fn actor(&self) -> Result<&ActorHandle, RpcError> {
        self.actor
            .get()
            .ok_or_else(|| RpcError::internal("the core is not up yet"))
    }

    // --- task.create ---------------------------------------------------------------------------

    /// The repository that the call names: a path, or the root of a workspace (`w3`).
    pub(super) async fn repo_of(&self, text: &str) -> Result<PathBuf, RpcError> {
        let wanted = text.to_owned();
        let by_workspace = self
            .actor()?
            .update(move |parts| {
                parts
                    .state
                    .layout
                    .workspaces
                    .iter()
                    .find(|w| w.handle == wanted || w.id.to_string() == wanted)
                    .map(|w| w.root.clone())
            })
            .await
            .map_err(gone)?;
        let path = match by_workspace {
            Some(Some(root)) => PathBuf::from(root),
            Some(None) => {
                return Err(RpcError::invalid_params(format!(
                    "the workspace {text} has no root; give the path of the repository"
                )));
            }
            None => expand(text, &self.env),
        };
        if !path.is_dir() {
            return Err(RpcError::invalid_params(format!(
                "{} is not a directory",
                path.display()
            )));
        }
        self.git.repo_root(&path).await.map_err(|error| {
            RpcError::invalid_params(format!(
                "{} is not a git repository: {error}",
                path.display()
            ))
        })
    }

    fn home(&self) -> Option<PathBuf> {
        self.env
            .iter()
            .find(|(name, _)| name == "HOME")
            .map(|(_, value)| PathBuf::from(value))
    }

    /// Keeps the files of a call as blobs, and gives the rows of the attachments.
    async fn keep_files(
        &self,
        paths: &[String],
        uploads: Vec<Upload>,
    ) -> Result<Vec<AttachmentRow>, RpcError> {
        let incoming = files::read(paths, uploads, self.home().as_deref()).await?;
        let mut rows = Vec::with_capacity(incoming.len());
        for files::Incoming { name, bytes } in incoming {
            let mime = files::mime_of(&name);
            let size = bytes.len() as u64;
            let blob = self
                .actor()?
                .store(move |store| store.put_blob(&bytes, mime, now_ms()))
                .await
                .map_err(gone)?;
            rows.push(AttachmentRow {
                id: crate::ids::new_ulid()?.to_string(),
                name,
                mime: mime.to_owned(),
                bytes: size,
                blob,
                path: None,
            });
        }
        Ok(rows)
    }

    /// The task that a reference names, with its row.
    pub(super) async fn find(&self, reference: Ref<TaskId>) -> Result<(TaskId, TaskRow), RpcError> {
        self.actor()?
            .update(move |parts| {
                let id = parts.state.tasks.resolve(&reference)?;
                parts.state.tasks.get(id).map(|t| (id, t.row.clone()))
            })
            .await
            .map_err(gone)?
            .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no such task"))
    }

    /// Keeps a draft. The repository and the words are checked now, so a wrong path is
    /// an error of the call. With `start` the draft starts at once.
    pub(crate) async fn create(self: &Arc<Self>, p: TaskCreate) -> Result<TaskCreated, RpcError> {
        let title = p.title.trim().to_owned();
        if title.is_empty() {
            return Err(RpcError::invalid_params("the title of a task is empty"));
        }
        if p.harness.is_none() && (p.model.is_some() || p.effort.is_some()) {
            return Err(RpcError::invalid_params(
                "a model or an effort level needs an agent: give harness",
            ));
        }
        // The other checks of the agent come when it starts, after the worktree. A wrong word must
        // not leave a task without its agent.
        crate::agents::control::check_words(p.model.as_deref(), p.effort.as_deref())?;
        let repo = self.repo_of(&p.repo).await?;
        if p.files.len() + p.uploads.len() > files::MAX_FILES {
            return Err(too_many_files());
        }
        let attachments = self.keep_files(&p.files, p.uploads).await?;
        let created = crate::ids::new_ulid()?;
        let row = TaskRow {
            id: created,
            handle: String::new(),
            title,
            slug: String::new(),
            workspace: None,
            tab: None,
            repo_root: repo.display().to_string(),
            worktree_path: String::new(),
            branch: trimmed(p.branch).unwrap_or_default(),
            base: trimmed(p.base).unwrap_or_default(),
            status: status_name(TaskStatus::Draft).to_owned(),
            env: json!({}),
            setup_blob: None,
            harness: p.harness.map(|h| harness_name(h).to_owned()),
            model: p.model,
            effort: p.effort,
            prompt: trimmed(p.prompt),
            ports: p.ports,
            attachments,
            created_at: now_ms(),
            updated_at: now_ms(),
        };
        let task = TaskId::new(created);
        let handle = self
            .actor()?
            .update(move |mut parts| {
                let number = parts.commit(|tx| tx.next_handle("task")).map_err(gone)?;
                let mut row = row;
                row.handle = format!("k{number}");
                let handle = row.handle.clone();
                super::add(
                    &mut parts,
                    Task {
                        row,
                        ports: Vec::new(),
                        diff: None,
                        counted: super::changes::Counted::default(),
                    },
                )
                .map_err(gone)?;
                Ok::<_, RpcError>(handle)
            })
            .await
            .map_err(gone)??;
        let job = if p.start {
            match self.start(Ref::<TaskId>::id(task)).await {
                Ok(job) => Some(job),
                Err(mut error) => {
                    error.message = format!(
                        "the task {handle} is a draft, and it did not start: {}",
                        error.message
                    );
                    return Err(error);
                }
            }
        } else {
            None
        };
        Ok(TaskCreated { task, handle, job })
    }

    /// Changes a task. A draft can change everything. A task that started can change
    /// only its title.
    pub(crate) async fn update(&self, p: TaskUpdate) -> Result<(), RpcError> {
        let (id, row) = self.find(p.task.clone()).await?;
        let more = p.repo.is_some()
            || p.base.is_some()
            || p.branch.is_some()
            || p.harness.is_some()
            || p.prompt.is_some()
            || p.model.is_some()
            || p.effort.is_some()
            || p.ports.is_some()
            || !p.files.is_empty()
            || !p.uploads.is_empty()
            || !p.remove.is_empty();
        if more && row.status != "draft" {
            return Err(started(&row.handle));
        }
        let mut next = row.clone();
        if let Some(title) = &p.title {
            let title = title.trim();
            if title.is_empty() {
                return Err(RpcError::invalid_params("the title of a task is empty"));
            }
            title.clone_into(&mut next.title);
        }
        if let Some(repo) = &p.repo {
            next.repo_root = self.repo_of(repo).await?.display().to_string();
        }
        if let Some(base) = p.base {
            next.base = trimmed(base).unwrap_or_default();
        }
        if let Some(branch) = p.branch {
            next.branch = trimmed(branch).unwrap_or_default();
        }
        if let Some(harness) = p.harness {
            next.harness = harness.map(|h| harness_name(h).to_owned());
        }
        if let Some(prompt) = p.prompt {
            next.prompt = trimmed(prompt);
        }
        let asks_for_choice = p.model.as_ref().is_some_and(Option::is_some)
            || p.effort.as_ref().is_some_and(Option::is_some);
        if let Some(model) = p.model {
            next.model = model;
        }
        if let Some(effort) = p.effort {
            next.effort = effort;
        }
        if next.harness.is_none() {
            if asks_for_choice {
                return Err(RpcError::invalid_params(
                    "a model or an effort level needs an agent: give harness",
                ));
            }
            // Without an agent the choices of the agent go too.
            next.model = None;
            next.effort = None;
        }
        crate::agents::control::check_words(next.model.as_deref(), next.effort.as_deref())?;
        if let Some(ports) = p.ports {
            next.ports = ports;
        }
        if let Some(unknown) = p
            .remove
            .iter()
            .find(|id| !next.attachments.iter().any(|file| file.id == **id))
        {
            return Err(RpcError::new(
                ErrorCode::NotFound,
                format!("the task {} has no file {unknown}", row.handle),
            ));
        }
        next.attachments.retain(|file| !p.remove.contains(&file.id));
        if next.attachments.len() + p.files.len() + p.uploads.len() > files::MAX_FILES {
            return Err(too_many_files());
        }
        let added = self.keep_files(&p.files, p.uploads).await?;
        next.attachments.extend(added);
        let title = p.title.is_some();
        self.actor()?
            .update(move |mut parts| {
                let current = parts
                    .state
                    .tasks
                    .get(id)
                    .map(|task| task.row.clone())
                    .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no such task"))?;
                let stored = if more {
                    // The draft must be the one that the change started from.
                    if current.status != "draft" {
                        return Err(started(&current.handle));
                    }
                    if current != row {
                        return Err(RpcError::new(
                            ErrorCode::StaleVersion,
                            "the task changed in the meantime; read it again",
                        ));
                    }
                    next
                } else if title {
                    TaskRow {
                        title: next.title,
                        ..current
                    }
                } else {
                    return Ok(());
                };
                super::change(&mut parts, stored).map_err(gone)
            })
            .await
            .map_err(gone)?
    }

    /// Removes a draft or a closed task, with its files.
    pub(crate) async fn delete(&self, reference: Ref<TaskId>) -> Result<(), RpcError> {
        self.actor()?
            .update(move |mut parts| {
                let id = parts
                    .state
                    .tasks
                    .resolve(&reference)
                    .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no such task"))?;
                let row = parts.state.tasks.get(id).map(|task| task.row.clone());
                if let Some(row) = row
                    && !matches!(row.status.as_str(), "draft" | "closed")
                {
                    return Err(RpcError::invalid_params(format!(
                        "the task {} has started; close it first",
                        row.handle
                    )));
                }
                super::delete(&mut parts, id).map_err(gone)
            })
            .await
            .map_err(gone)?
    }

    // --- task.start ----------------------------------------------------------------------------

    /// Starts a draft: the job makes its worktree, its ports, its files, its setup, its tab and
    /// its agent. The repository and the base are checked first, so a wrong one is an error of the
    /// call and the task stays a draft.
    pub(crate) async fn start(self: &Arc<Self>, reference: Ref<TaskId>) -> Result<JobId, RpcError> {
        let (id, row) = self.find(reference).await?;
        if row.status != "draft" {
            return Err(running(&row.handle));
        }
        // The harness is there and logged in before the worktree exists, so a task whose harness
        // is not there stays a draft.
        if let Some(harness) = row.harness.as_deref().and_then(harness_of) {
            self.control.ensure_harness(harness).await?;
        }
        let repo = self.repo_of(&row.repo_root).await?;
        let base = if row.base.is_empty() {
            self.git.current_branch(&repo).await.map_err(git_error)?
        } else {
            row.base.clone()
        };
        // The task leaves the draft once: a second start finds it `creating`.
        let draft = self
            .actor()?
            .update(move |mut parts| {
                let current = parts.state.tasks.get(id).map(|task| task.row.clone());
                match current {
                    Some(draft) if draft.status == "draft" => {
                        super::set_status(&mut parts, id, TaskStatus::Creating);
                        Ok(draft)
                    }
                    Some(other) => Err(running(&other.handle)),
                    None => Err(RpcError::new(ErrorCode::NotFound, "no such task")),
                }
            })
            .await
            .map_err(gone)??;
        let job = match self.jobs.start(format!("Start the task {}", draft.title)) {
            Ok(job) => job,
            Err(error) => {
                self.back_to_draft(draft).await;
                return Err(error);
            }
        };
        let job_id = job.id();
        let this = Arc::clone(self);
        self.handle.spawn(async move {
            match this.run_start(&job, draft, repo, base).await {
                Ok(done) => job.done(done),
                Err(error) => job.fail(super::calls::job_error(&error)),
            }
        });
        Ok(job_id)
    }

    /// A slug, a branch and a path that nothing uses.
    async fn free_name(
        &self,
        repo: &Path,
        repo_name: &str,
        title: &str,
        branch_override: Option<&str>,
        config: &Config,
    ) -> Result<(String, String, PathBuf), RpcError> {
        let base_slug = slug::slugify(title);
        let root = expand(&config.worktree_root, &self.env).join(repo_name);
        let taken_by_tasks = self
            .actor()?
            .update({
                let repo = repo.display().to_string();
                move |parts| {
                    parts
                        .state
                        .tasks
                        .all()
                        .iter()
                        .filter(|task| task.row.repo_root == repo)
                        .map(|task| task.row.slug.clone())
                        .collect::<HashSet<_>>()
                }
            })
            .await
            .map_err(gone)?;
        for n in 1..1000 {
            let candidate = slug::numbered(&base_slug, n);
            let branch = match branch_override {
                Some(branch) if n == 1 => branch.to_owned(),
                Some(branch) => format!("{branch}-{n}"),
                None => format!("{}{candidate}", config.branch_prefix),
            };
            let path = root.join(&candidate);
            let used = taken_by_tasks.contains(&candidate)
                || path.exists()
                || self.git.branch_exists(repo, &branch).await;
            if !used {
                return Ok((candidate, branch, path));
            }
        }
        Err(RpcError::new(
            ErrorCode::NameTaken,
            "no free name for the task",
        ))
    }

    async fn run_start(
        self: &Arc<Self>,
        job: &JobHandle,
        draft: TaskRow,
        repo: PathBuf,
        base: String,
    ) -> Result<Value, RpcError> {
        let actor = self.actor()?;
        let task_id = TaskId::new(draft.id);
        let handle_text = draft.handle.clone();
        let title = draft.title.clone();
        let config = actor
            .update(|parts| Config::read(parts.state.config.config().as_ref()))
            .await
            .map_err(gone)?;
        let repo_name = repo
            .file_name()
            .map_or_else(|| "repo".to_owned(), |n| n.to_string_lossy().into_owned());
        job.progress(0.05, "Choosing a name");
        let wanted_branch = (!draft.branch.is_empty()).then_some(draft.branch.as_str());
        let named = self
            .free_name(&repo, &repo_name, &title, wanted_branch, &config)
            .await;
        let (slug, branch, worktree) = match named {
            Ok(named) => named,
            Err(error) => {
                self.back_to_draft(draft).await;
                return Err(error);
            }
        };
        let (wt_text, branch_text, base_text) =
            (worktree.display().to_string(), branch.clone(), base.clone());
        actor
            .update(move |mut parts| {
                super::named(&mut parts, task_id, slug, wt_text, branch_text, base_text);
            })
            .await
            .map_err(gone)?;

        // The worktree.
        job.progress(0.15, "Creating the worktree");
        let made = match worktree.parent() {
            Some(parent) => std::fs::create_dir_all(parent).map_err(gone),
            None => Ok(()),
        };
        if let Err(error) = made {
            self.back_to_draft(draft).await;
            return Err(error);
        }
        if let Err(error) = self
            .git
            .worktree_add(&repo, &worktree, &branch, &base)
            .await
        {
            self.back_to_draft(draft).await;
            return Err(git_error(error));
        }

        // The ports.
        job.progress(0.3, "Leasing ports");
        let wanted = usize::from(draft.ports.unwrap_or(config.ports));
        let range = (config.port_range.0, config.port_range.1);
        let leased = actor
            .update(move |mut parts| {
                let taken = parts.state.tasks.leased_ports();
                let Some(found) = ports::pick(wanted, range, &taken) else {
                    return Err(RpcError::new(
                        ErrorCode::NoFreePorts,
                        format!(
                            "the range {}-{} has no {wanted} free ports",
                            range.0, range.1
                        ),
                    ));
                };
                super::lease(&mut parts, task_id, &found).map_err(gone)?;
                Ok(found)
            })
            .await
            .map_err(gone)?;
        let found = match leased {
            Ok(found) => found,
            Err(error) => {
                let _ = self.git.worktree_remove(&repo, &worktree, true).await;
                self.back_to_draft(draft).await;
                return Err(error);
            }
        };

        // The files, where git does not see them.
        let mut placed = Vec::new();
        if !draft.attachments.is_empty() {
            job.progress(0.35, "Copying the files");
            match self
                .copy_files(task_id, &worktree, &draft.attachments)
                .await
            {
                Ok(paths) => placed = paths,
                Err(error) => {
                    let _ = self.git.worktree_remove(&repo, &worktree, true).await;
                    self.back_to_draft(draft).await;
                    return Err(error);
                }
            }
        }

        // The environment of the panes.
        let mut env: BTreeMap<String, String> = config.env.clone();
        env.insert("NEOHAR_TASK_ID".to_owned(), handle_text.clone());
        if let Some(first) = found.first() {
            env.insert("PORT".to_owned(), first.to_string());
        }
        env.insert(
            "NEOHAR_PORTS".to_owned(),
            found
                .iter()
                .map(u16::to_string)
                .collect::<Vec<_>>()
                .join(","),
        );

        // The setup commands.
        if !config.setup.is_empty() {
            job.progress(0.4, "Running the setup commands");
            self.set_status(task_id, TaskStatus::Setup).await?;
            let mut process_env = self.env.clone();
            process_env.extend(env.iter().map(|(k, v)| (k.clone(), v.clone())));
            let outcome: Outcome =
                setup::run(&self.shell, &config.setup, &worktree, &process_env).await;
            let blob = self.keep_output(&outcome.output).await?;
            self.record_env(task_id, &env, blob.clone()).await?;
            if !outcome.success {
                self.set_status(task_id, TaskStatus::SetupFailed).await?;
                let mut error = RpcError::new(
                    ErrorCode::SetupFailed,
                    format!(
                        "the setup command `{}` failed",
                        outcome.failed.as_deref().unwrap_or("?")
                    ),
                );
                error.details = json!({"task": handle_text, "log": blob});
                return Err(error);
            }
        } else {
            self.record_env(task_id, &env, None).await?;
        }

        // The tab.
        job.progress(0.6, "Opening a tab");
        let (workspace, tab, pane) = self
            .open_tab(&repo, &repo_name, &title, &worktree, &env)
            .await?;
        self.attach_tab(task_id, workspace, tab).await?;
        self.set_status(task_id, TaskStatus::Idle).await?;

        // The agent.
        let mut agent_note = None;
        if let Some(harness) = draft.harness.as_deref().and_then(harness_of) {
            job.progress(0.75, "Starting the agent");
            let started = self
                .control
                .start(AgentStart {
                    harness,
                    placement: Placement::Pane {
                        pane: Ref::<PaneId>::id(pane),
                    },
                    cwd: None,
                    name: None,
                    prompt: files::prompt_with(draft.prompt.as_deref(), &placed),
                    model: draft.model.clone(),
                    effort: draft.effort.clone(),
                    args: Vec::new(),
                    task: Some(Ref::<TaskId>::id(task_id)),
                })
                .await;
            match started {
                Ok(agent_job) => {
                    if let Some(error) = self.wait_job(job, agent_job).await {
                        agent_note = Some(error);
                    }
                }
                Err(error) => agent_note = Some(error.message),
            }
        }
        self.refresh_diff(task_id).await;
        if let Some(note) = agent_note {
            let mut error = RpcError::new(
                ErrorCode::AgentFailed,
                format!("the task exists, but the agent did not start: {note}"),
            );
            error.details = json!({"task": handle_text});
            return Err(error);
        }
        Ok(json!({"task": task_id.to_string(), "handle": handle_text, "pane": pane.to_string()}))
    }

    /// Waits for a job of the registry to end. Gives its error text when it failed.
    async fn wait_job(&self, parent: &JobHandle, child: JobId) -> Option<String> {
        loop {
            if parent.is_cancelled() {
                return Some("the job was cancelled".to_owned());
            }
            let value = self.jobs.get(child)?;
            match value.status {
                JobStatus::Running => tokio::time::sleep(Duration::from_millis(100)).await,
                JobStatus::Done => return None,
                JobStatus::Failed | JobStatus::Cancelled => {
                    return Some(
                        value
                            .error
                            .map_or_else(|| "it ended".to_owned(), |error| error.message),
                    );
                }
            }
        }
    }

    /// The start failed before the task had a worktree: the task is the draft again.
    async fn back_to_draft(&self, draft: TaskRow) {
        if let Ok(actor) = self.actor() {
            let _ = actor
                .update(move |mut parts| super::back_to_draft(&mut parts, draft))
                .await;
        }
    }

    /// Copies the files of a task into its worktree, and notes where each one is.
    async fn copy_files(
        &self,
        task: TaskId,
        worktree: &Path,
        attachments: &[AttachmentRow],
    ) -> Result<Vec<PathBuf>, RpcError> {
        let actor = self.actor()?;
        let mut contents = Vec::with_capacity(attachments.len());
        for file in attachments {
            let hash = file.blob.clone();
            let bytes = actor
                .store(move |store| store.get_blob(&hash))
                .await
                .map_err(gone)?
                .ok_or_else(|| {
                    RpcError::internal(format!("the store has lost the file {}", file.name))
                })?;
            contents.push((file.name.clone(), bytes));
        }
        let paths = files::write_all(worktree, contents)
            .await
            .map_err(|e| RpcError::internal(format!("the files are not in the worktree: {e}")))?;
        let noted: Vec<(String, String)> = attachments
            .iter()
            .zip(&paths)
            .map(|(file, path)| (file.id.clone(), path.display().to_string()))
            .collect();
        actor
            .update(move |mut parts| super::placed(&mut parts, task, noted))
            .await
            .map_err(gone)?;
        Ok(paths)
    }

    async fn set_status(&self, task: TaskId, status: TaskStatus) -> Result<(), RpcError> {
        self.actor()?
            .update(move |mut parts| super::set_status(&mut parts, task, status))
            .await
            .map_err(gone)
    }

    async fn keep_output(&self, output: &str) -> Result<Option<String>, RpcError> {
        let bytes = nh_redact::redact(output).into_owned().into_bytes();
        let hash = self
            .actor()?
            .store(move |store| store.put_blob(&bytes, "text/plain", now_ms()))
            .await
            .map_err(gone)?;
        Ok(Some(hash))
    }

    async fn record_env(
        &self,
        task: TaskId,
        env: &BTreeMap<String, String>,
        blob: Option<String>,
    ) -> Result<(), RpcError> {
        let env = serde_json::to_value(env).map_err(gone)?;
        self.actor()?
            .update(move |mut parts| super::record_env(&mut parts, task, env, blob))
            .await
            .map_err(gone)
    }

    async fn attach_tab(
        &self,
        task: TaskId,
        workspace: WorkspaceId,
        tab: nh_proto::api::ids::TabId,
    ) -> Result<(), RpcError> {
        self.actor()?
            .update(move |mut parts| super::attach_tab(&mut parts, task, workspace, tab))
            .await
            .map_err(gone)
    }

    /// The tab of the task, in the workspace whose root is the repository. The workspace is made
    /// when it is missing.
    async fn open_tab(
        &self,
        repo: &Path,
        repo_name: &str,
        title: &str,
        worktree: &Path,
        env: &BTreeMap<String, String>,
    ) -> Result<(WorkspaceId, nh_proto::api::ids::TabId, PaneId), RpcError> {
        let actor = self.actor()?;
        let root = repo.display().to_string();
        let existing = actor
            .update({
                let root = root.clone();
                move |parts| {
                    parts
                        .state
                        .layout
                        .workspaces
                        .iter()
                        .find(|w| w.root.as_deref() == Some(root.as_str()))
                        .map(|w| w.id)
                }
            })
            .await
            .map_err(gone)?;
        let workspace = match existing {
            Some(id) => id,
            None => {
                let call =
                    Call::parse("workspace.create", json!({"name": repo_name, "root": root}))
                        .map_err(|e| RpcError::invalid_params(e.to_string()))?;
                let made = actor.call(call, Origin::ui()).await?;
                parse_id::<WorkspaceId>(&made, "workspace")?
            }
        };
        let call = Call::parse(
            "tab.create",
            json!({
                "workspace": workspace.to_string(),
                "name": title,
                "command": {
                    "argv": [],
                    "cwd": worktree.display().to_string(),
                    "env": env,
                },
            }),
        )
        .map_err(|e| RpcError::invalid_params(e.to_string()))?;
        let made = actor.call(call, Origin::ui()).await?;
        let tab = parse_id::<nh_proto::api::ids::TabId>(&made, "tab")?;
        let pane = parse_id::<PaneId>(&made, "pane")?;
        Ok((workspace, tab, pane))
    }

    // --- diff statistics -----------------------------------------------------------------------

    /// Counts the changes and the lines of a task now.
    pub(crate) async fn refresh_diff(&self, task: TaskId) {
        let _ = self.count(task, false).await;
    }

    /// Counts the lines of a task in a moment. A burst of calls is one count. The windows that
    /// show the worktree of the task count again too.
    pub(crate) fn schedule_diff(self: &Arc<Self>, task: TaskId) {
        self.schedule_windows_of(task);
        if !self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(task)
        {
            return;
        }
        let this = Arc::clone(self);
        self.handle.spawn(async move {
            tokio::time::sleep(DIFF_DEBOUNCE).await;
            this.pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&task);
            this.refresh_diff(task).await;
        });
    }

    // --- task.close and worktree.list ----------------------------------------------------------

    pub(crate) async fn close(
        &self,
        reference: Ref<TaskId>,
        remove_worktree: bool,
        force: bool,
    ) -> Result<(), RpcError> {
        let actor = self.actor()?;
        let found = actor
            .update(move |parts| {
                let id = parts.state.tasks.resolve(&reference)?;
                parts.state.tasks.get(id).map(|t| (id, t.row.clone()))
            })
            .await
            .map_err(gone)?
            .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no such task"))?;
        let (id, row) = found;
        if row.status == "closed" {
            return Err(RpcError::new(
                ErrorCode::InvalidParams,
                "the task is closed already",
            ));
        }
        if row.status == "draft" {
            return Err(RpcError::invalid_params(
                "the task is a draft and has nothing to close; delete it",
            ));
        }
        if remove_worktree {
            let path = PathBuf::from(&row.worktree_path);
            if path.exists() {
                let dirty = self.git.dirty(&path).await.map_err(git_error)?;
                if dirty && !force {
                    let mut error = RpcError::new(
                        ErrorCode::ConfirmRequired,
                        "the worktree has changes that the removal loses; call again with force",
                    );
                    error.details = json!({"worktree": row.worktree_path, "dirty": true});
                    return Err(error);
                }
                self.git
                    .worktree_remove(Path::new(&row.repo_root), &path, force)
                    .await
                    .map_err(git_error)?;
            }
        }
        actor
            .update(move |mut parts| super::close(&mut parts, id))
            .await
            .map_err(gone)
    }

    /// The worktrees of the repositories, with the task of each and whether it has changes.
    pub(crate) async fn worktrees(
        &self,
        repo: Option<String>,
    ) -> Result<Vec<(Worktree, PathBuf, bool)>, RpcError> {
        let repos: Vec<PathBuf> = match repo {
            Some(text) => vec![self.repo_of(&text).await?],
            None => {
                let mut roots: Vec<String> = self
                    .actor()?
                    .update(|parts| {
                        parts
                            .state
                            .tasks
                            .all()
                            .iter()
                            .map(|t| t.row.repo_root.clone())
                            .collect()
                    })
                    .await
                    .map_err(gone)?;
                roots.sort();
                roots.dedup();
                roots.into_iter().map(PathBuf::from).collect()
            }
        };
        let mut out = Vec::new();
        for repo in repos {
            for worktree in self.git.worktrees(&repo).await.map_err(git_error)? {
                let dirty = self.git.dirty(&worktree.path).await.unwrap_or(false);
                out.push((worktree, repo.clone(), dirty));
            }
        }
        Ok(out)
    }
}

/// A text without the space around it, or `None` when nothing is left.
fn trimmed(text: Option<String>) -> Option<String> {
    text.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
}

fn too_many_files() -> RpcError {
    RpcError::invalid_params(format!("a task has at most {} files", files::MAX_FILES))
}

/// The error of a start of a task that is not a draft.
fn running(handle: &str) -> RpcError {
    RpcError::invalid_params(format!("the task {handle} has started already"))
}

/// The error of a change that only a draft can have.
fn started(handle: &str) -> RpcError {
    RpcError::invalid_params(format!(
        "the task {handle} has started: only a draft can change this"
    ))
}

fn parse_id<T: std::str::FromStr>(value: &Value, key: &str) -> Result<T, RpcError> {
    value[key]
        .as_str()
        .and_then(|text| text.parse::<T>().ok())
        .ok_or_else(|| RpcError::internal(format!("the layout gave no {key}")))
}

/// The settings of the tasks that a create needs.
pub(crate) struct Config {
    pub worktree_root: String,
    pub branch_prefix: String,
    pub ports: u8,
    pub port_range: (u16, u16),
    pub setup: Vec<String>,
    pub env: BTreeMap<String, String>,
}

impl Config {
    pub(crate) fn read(config: &crate::config::Config) -> Self {
        let tasks = &config.tasks;
        Self {
            worktree_root: tasks.worktree_root.clone(),
            branch_prefix: tasks.branch_prefix.clone(),
            ports: tasks.ports,
            port_range: (tasks.port_range[0], tasks.port_range[1]),
            setup: tasks.setup.clone(),
            env: tasks.env.clone(),
        }
    }
}
