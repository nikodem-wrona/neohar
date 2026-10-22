//! The task workspaces in the core.
//!
//! A task is a unit of work with its own worktree, branch, ports and tab. It starts as a draft:
//! the repository, the title, the agent and its prompt, and the files. `task.start` makes
//! the rest. The rows are in the tables `tasks` and `port_leases`, the table in memory mirrors
//! them, and the projection `tasks` shows them. Every change is one transaction: the row, the
//! event, and then the projection. [`service`] runs the jobs (`task.start`, `task.close`),
//! [`files`] keeps the files, [`favorites`] keeps the favorite folders of the form, [`dirs`] lists
//! folders for its folder browser, and [`calls`] serves the methods.

pub(crate) mod calls;
pub(crate) mod changes;
pub(crate) mod dirs;
pub(crate) mod favorites;
pub(crate) mod files;
pub(crate) mod folder_changes;
pub(crate) mod service;

use std::collections::HashMap;

use nh_proto::api::{
    common::Harness,
    events::{EventPayload, TaskEvent, TaskStatusChanged},
    ids::{Ref, RefValue, RunId, Target, TaskId},
    model::{AttachmentValue, Execution, TaskStatus, TaskValue},
    projections::Projection,
    rich::DiffStats,
    time::Timestamp,
};
use nh_store::{FavoriteRow, Store, StoreError, TaskRow};

use crate::{actor::Parts, persist::now_ms};

/// One task: its row, the ports it leases, the last diff statistics, and the last count of its
/// changes.
#[derive(Clone, Debug)]
pub(crate) struct Task {
    pub row: TaskRow,
    pub ports: Vec<u16>,
    pub diff: Option<DiffStats>,
    pub counted: changes::Counted,
}

impl Task {
    pub fn id(&self) -> TaskId {
        TaskId::new(self.row.id)
    }
}

/// The name of a status as the table writes it.
pub(crate) fn status_name(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Draft => "draft",
        TaskStatus::Creating => "creating",
        TaskStatus::Setup => "setup",
        TaskStatus::Running => "running",
        TaskStatus::Idle => "idle",
        TaskStatus::SetupFailed => "setup_failed",
        TaskStatus::Closed => "closed",
    }
}

pub(crate) fn status_of(name: &str) -> TaskStatus {
    match name {
        "draft" => TaskStatus::Draft,
        "creating" => TaskStatus::Creating,
        "setup" => TaskStatus::Setup,
        "running" => TaskStatus::Running,
        "idle" => TaskStatus::Idle,
        "closed" => TaskStatus::Closed,
        _ => TaskStatus::SetupFailed,
    }
}

/// The name of a harness as the table writes it.
pub(crate) fn harness_name(harness: Harness) -> &'static str {
    match harness {
        Harness::Claude => "claude",
        Harness::Codex => "codex",
    }
}

pub(crate) fn harness_of(name: &str) -> Option<Harness> {
    match name {
        "claude" => Some(Harness::Claude),
        "codex" => Some(Harness::Codex),
        _ => None,
    }
}

/// A column that a draft leaves empty.
fn filled(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_owned())
}

/// All the tasks, and the favorite folders of the form of a new task.
#[derive(Clone, Debug, Default)]
pub(crate) struct Tasks {
    by_id: HashMap<TaskId, Task>,
    /// The one used last first, as the store gives them.
    favorites: Vec<FavoriteRow>,
}

impl Tasks {
    /// The tasks of the last core. A task that was in the middle of its start cannot go on. When
    /// its worktree is not there, nothing came of the start, and the task is a draft again.
    /// Else it ends in `setup_failed`. The store learns it in the same transaction.
    pub fn load(store: &mut Store, now: i64) -> Result<Self, StoreError> {
        let rows = store.tasks()?;
        let leases = store.port_leases()?;
        let mut by_id = HashMap::new();
        let mut stale = Vec::new();
        for mut row in rows {
            let leased = leases.iter().any(|(_, task)| *task == row.id);
            let made =
                !row.worktree_path.is_empty() && std::path::Path::new(&row.worktree_path).exists();
            if row.status == "creating" && !made && !leased {
                row.status = status_name(TaskStatus::Draft).to_owned();
                row.slug.clear();
                row.worktree_path.clear();
                row.updated_at = now;
                stale.push(row.clone());
            } else if matches!(row.status.as_str(), "creating" | "setup") {
                row.status = "setup_failed".to_owned();
                row.updated_at = now;
                stale.push(row.clone());
            }
            let mut ports: Vec<u16> = leases
                .iter()
                .filter(|(_, task)| *task == row.id)
                .map(|(port, _)| *port)
                .collect();
            ports.sort_unstable();
            let task = Task {
                row,
                ports,
                diff: None,
                counted: changes::Counted::default(),
            };
            by_id.insert(task.id(), task);
        }
        if !stale.is_empty() {
            store.tx(now, |tx| stale.iter().try_for_each(|row| tx.put_task(row)))?;
        }
        let favorites = store.favorites()?;
        Ok(Self { by_id, favorites })
    }

    pub fn get(&self, id: TaskId) -> Option<&Task> {
        self.by_id.get(&id)
    }

    pub fn put(&mut self, task: Task) {
        self.by_id.insert(task.id(), task);
    }

    pub fn remove(&mut self, id: TaskId) {
        self.by_id.remove(&id);
    }

    /// The task that a reference names: its id, or its handle (`k7`).
    pub fn resolve(&self, reference: &Ref<TaskId>) -> Option<TaskId> {
        match &reference.0 {
            RefValue::Id(id) => self.by_id.contains_key(id).then_some(*id),
            RefValue::Handle(handle) => {
                if handle.machine.is_some() {
                    return None;
                }
                let Target::Task(number) = handle.target else {
                    return None;
                };
                let wanted = format!("k{number}");
                self.by_id
                    .values()
                    .find(|task| task.row.handle == wanted)
                    .map(Task::id)
            }
            RefValue::Name(_) => None,
        }
    }

    /// The tasks, the oldest first.
    pub fn all(&self) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self.by_id.values().collect();
        tasks.sort_by_key(|task| (task.row.created_at, task.row.id));
        tasks
    }

    /// The favorite folders, the one used last first.
    pub fn favorites(&self) -> &[FavoriteRow] {
        &self.favorites
    }

    pub fn favorite(&self, path: &str) -> Option<&FavoriteRow> {
        self.favorites.iter().find(|row| row.path == path)
    }

    pub fn put_favorite(&mut self, row: FavoriteRow) {
        if self.favorite(&row.path).is_none() {
            self.favorites.push(row);
            self.order_favorites();
        }
    }

    pub fn remove_favorite(&mut self, path: &str) {
        self.favorites.retain(|row| row.path != path);
    }

    /// A task used the repository at `path`, which is a favorite.
    fn use_favorite(&mut self, path: &str, at: i64) {
        if let Some(row) = self.favorites.iter_mut().find(|row| row.path == path) {
            row.used_at = at;
            self.order_favorites();
        }
    }

    /// The order of the store: the one used last first.
    fn order_favorites(&mut self) {
        self.favorites
            .sort_by(|a, b| b.used_at.cmp(&a.used_at).then_with(|| a.path.cmp(&b.path)));
    }

    /// The ports that tasks lease now.
    pub fn leased_ports(&self) -> std::collections::HashSet<u16> {
        self.by_id
            .values()
            .flat_map(|task| task.ports.iter().copied())
            .collect()
    }
}

/// The row of `tasks` for a task, with the live agents.
pub(crate) fn value(parts: &Parts<'_>, task: &Task) -> TaskValue {
    let mut agents: Vec<(Timestamp, RunId)> = parts
        .state
        .runs
        .by_id
        .values()
        .filter(|run| run.live() && run.task == Some(task.id()))
        .map(|run| (run.started_at, run.id))
        .collect();
    agents.sort_unstable();
    TaskValue {
        id: task.id(),
        handle: task.row.handle.clone(),
        title: task.row.title.clone(),
        workspace: task.row.workspace.map(nh_proto::api::ids::WorkspaceId::new),
        tab: task.row.tab.map(nh_proto::api::ids::TabId::new),
        repo: task.row.repo_root.clone(),
        worktree: filled(&task.row.worktree_path),
        branch: filled(&task.row.branch),
        base: filled(&task.row.base),
        status: status_of(&task.row.status),
        ports: task.ports.clone(),
        agents: agents.into_iter().map(|(_, id)| id).collect(),
        diff_stats: task.diff,
        changes: task.counted.totals.clone(),
        harness: task.row.harness.as_deref().and_then(harness_of),
        model: task.row.model.clone(),
        effort: task.row.effort.clone(),
        prompt: task.row.prompt.clone(),
        attachments: task
            .row
            .attachments
            .iter()
            .map(|file| AttachmentValue {
                id: file.id.clone(),
                name: file.name.clone(),
                mime: file.mime.clone(),
                bytes: file.bytes,
                path: file.path.clone(),
            })
            .collect(),
        created_at: Timestamp(task.row.created_at),
    }
}

/// Publishes one task in the projection, and puts the rows in their order.
pub(crate) fn publish(parts: &Parts<'_>, id: TaskId) {
    let Some(task) = parts.state.tasks.get(id) else {
        return;
    };
    if let Ok(json) = serde_json::to_value(value(parts, task)) {
        parts
            .projections
            .set(Projection::Tasks, &id.to_string(), json);
    }
    order(parts);
}

/// The order of the rows of `tasks`: the drafts, the oldest first, as a queue; then the
/// tasks that started, the oldest first; then the closed ones, the newest first. The Tasks tab
/// shows the three groups in this order, because the webview does not sort.
fn order(parts: &Parts<'_>) {
    let all = parts.state.tasks.all();
    let of = |status: TaskStatus| {
        all.iter()
            .filter(move |task| status_of(&task.row.status) == status)
            .map(|task| task.id().to_string())
    };
    let started = all
        .iter()
        .filter(|task| !matches!(task.row.status.as_str(), "draft" | "closed"))
        .map(|task| task.id().to_string());
    let keys: Vec<String> = of(TaskStatus::Draft)
        .chain(started)
        .chain(of(TaskStatus::Closed).rev())
        .collect();
    parts.projections.set_order(Projection::Tasks, &keys);
}

/// Publishes the favorite folders in the order of the store, the one used last first.
/// `gone` is a favorite that the person removed. The name of a favorite depends on the others, so
/// every row is set again; a row that is the same makes no patch.
pub(crate) fn publish_favorites(parts: &Parts<'_>, gone: Option<&str>) {
    if let Some(path) = gone {
        parts.projections.remove(Projection::Favorites, path);
    }
    let values = favorites::values(parts.state.tasks.favorites());
    let mut keys = Vec::with_capacity(values.len());
    for value in values {
        if let Ok(json) = serde_json::to_value(&value) {
            parts
                .projections
                .set(Projection::Favorites, &value.path, json);
        }
        keys.push(value.path);
    }
    parts.projections.set_order(Projection::Favorites, &keys);
}

/// Publishes every task and the favorite folders, after the start of the core.
pub(crate) fn publish_all(parts: &mut Parts<'_>) {
    let ids: Vec<TaskId> = parts
        .state
        .tasks
        .all()
        .iter()
        .map(|task| task.id())
        .collect();
    for id in ids {
        publish(parts, id);
    }
    publish_favorites(parts, None);
}

/// Changes the status of a task: the row and the event in one transaction, and then the
/// projection. A change to the same status does nothing.
pub(crate) fn set_status(parts: &mut Parts<'_>, id: TaskId, to: TaskStatus) {
    let Some(task) = parts.state.tasks.get(id) else {
        return;
    };
    let from = status_of(&task.row.status);
    if from == to {
        return;
    }
    let mut row = task.row.clone();
    row.status = status_name(to).to_owned();
    row.updated_at = now_ms();
    let subject = id.to_string();
    let stored = parts.commit(|tx| {
        tx.put_task(&row)?;
        tx.append(
            Some(&subject),
            &EventPayload::TaskStatusChanged(TaskStatusChanged { task: id, from, to }),
        )?;
        Ok(())
    });
    if let Err(error) = stored {
        tracing::warn!(task = %id, %error, "a task status is not stored");
    }
    if let Some(task) = parts.state.tasks.by_id.get_mut(&id) {
        task.row = row;
    }
    publish(parts, id);
}

fn event_of(row: &TaskRow) -> TaskEvent {
    TaskEvent {
        task: TaskId::new(row.id),
        handle: row.handle.clone(),
        title: row.title.clone(),
    }
}

/// Adds a task that `task.create` made. One transaction: the row, the references of its files and
/// the event.
pub(crate) fn add(parts: &mut Parts<'_>, task: Task) -> Result<(), StoreError> {
    let (row, ports) = (task.row.clone(), task.ports.clone());
    let id = task.id();
    let event = EventPayload::TaskCreated(event_of(&row));
    // A task with a favorite folder makes it the one used last.
    let used = parts.commit(|tx| {
        tx.put_task(&row)?;
        tx.lease_ports(row.id, &ports, row.created_at)?;
        for file in &row.attachments {
            tx.add_blob_ref(&file.blob)?;
        }
        tx.append(Some(&id.to_string()), &event)?;
        tx.use_favorite(&row.repo_root, row.created_at)
    })?;
    if used {
        parts
            .state
            .tasks
            .use_favorite(&row.repo_root, row.created_at);
        publish_favorites(parts, None);
    }
    parts.state.tasks.put(task);
    publish(parts, id);
    Ok(())
}

/// Stores a change of a task that `task.update` made: the row, the references of the files that
/// came and went, and the event, in one transaction. A new repository that is a favorite becomes
/// the one used last.
pub(crate) fn change(parts: &mut Parts<'_>, row: TaskRow) -> Result<(), StoreError> {
    let id = TaskId::new(row.id);
    let Some(task) = parts.state.tasks.get(id) else {
        return Ok(());
    };
    // Two files can have the same bytes, so the references are counted, not compared.
    let mut refs: HashMap<String, i64> = HashMap::new();
    for file in &row.attachments {
        *refs.entry(file.blob.clone()).or_default() += 1;
    }
    for file in &task.row.attachments {
        *refs.entry(file.blob.clone()).or_default() -= 1;
    }
    let moved = row.repo_root != task.row.repo_root;
    let mut row = row;
    row.updated_at = now_ms();
    let event = EventPayload::TaskChanged(event_of(&row));
    let used = parts.commit(|tx| {
        tx.put_task(&row)?;
        for (blob, delta) in &refs {
            for _ in 0..*delta {
                tx.add_blob_ref(blob)?;
            }
            for _ in *delta..0 {
                tx.release_blob_ref(blob)?;
            }
        }
        tx.append(Some(&id.to_string()), &event)?;
        if moved {
            tx.use_favorite(&row.repo_root, row.updated_at)
        } else {
            Ok(false)
        }
    })?;
    if used {
        parts
            .state
            .tasks
            .use_favorite(&row.repo_root, row.updated_at);
        publish_favorites(parts, None);
    }
    if let Some(task) = parts.state.tasks.by_id.get_mut(&id) {
        task.row = row;
    }
    publish(parts, id);
    Ok(())
}

/// Removes a draft or a closed task: the row, its leases and the references of its files go, and
/// the event says so.
pub(crate) fn delete(parts: &mut Parts<'_>, id: TaskId) -> Result<(), StoreError> {
    let Some(task) = parts.state.tasks.get(id) else {
        return Ok(());
    };
    let row = task.row.clone();
    let event = EventPayload::TaskDeleted(event_of(&row));
    parts.commit(|tx| {
        tx.delete_task(row.id)?;
        for file in &row.attachments {
            tx.release_blob_ref(&file.blob)?;
        }
        tx.append(Some(&id.to_string()), &event)?;
        Ok(())
    })?;
    parts.state.tasks.remove(id);
    parts.projections.remove(Projection::Tasks, &id.to_string());
    parts.projections.forget(Projection::Changes(id));
    Ok(())
}

/// A start that failed before the task had its worktree: the task is the draft again, with the
/// row that it had before the start, and its leases go back.
pub(crate) fn back_to_draft(parts: &mut Parts<'_>, draft: TaskRow) {
    let id = TaskId::new(draft.id);
    let Some(task) = parts.state.tasks.get(id) else {
        return;
    };
    let from = status_of(&task.row.status);
    let mut row = draft;
    row.status = status_name(TaskStatus::Draft).to_owned();
    row.updated_at = now_ms();
    let subject = id.to_string();
    let stored = parts.commit(|tx| {
        tx.put_task(&row)?;
        tx.release_ports(row.id)?;
        tx.append(
            Some(&subject),
            &EventPayload::TaskStatusChanged(TaskStatusChanged {
                task: id,
                from,
                to: TaskStatus::Draft,
            }),
        )?;
        Ok(())
    });
    if let Err(error) = stored {
        tracing::warn!(task = %id, %error, "the draft of a task is not stored again");
    }
    if let Some(task) = parts.state.tasks.by_id.get_mut(&id) {
        task.row = row;
        task.ports.clear();
        task.diff = None;
        task.counted = changes::Counted::default();
    }
    parts.projections.forget(Projection::Changes(id));
    publish(parts, id);
}

/// The start of a draft took its name: the slug, the worktree, the branch and the base. The
/// status is `creating`.
pub(crate) fn named(
    parts: &mut Parts<'_>,
    id: TaskId,
    slug: String,
    worktree: String,
    branch: String,
    base: String,
) {
    change_row(parts, id, |row| {
        row.slug = slug;
        row.worktree_path = worktree;
        row.branch = branch;
        row.base = base;
    });
}

/// The start copied the files into the worktree: each one has its path now.
pub(crate) fn placed(parts: &mut Parts<'_>, id: TaskId, paths: Vec<(String, String)>) {
    change_row(parts, id, |row| {
        for file in &mut row.attachments {
            file.path = paths
                .iter()
                .find(|(of, _)| *of == file.id)
                .map(|(_, path)| path.clone());
        }
    });
}

/// A run of a task changed: its agents and its status follow. The status is `running` while an
/// agent starts or works, and `idle` when none does.
pub(crate) fn run_changed(parts: &mut Parts<'_>, task: TaskId) {
    let Some(current) = parts.state.tasks.get(task) else {
        return;
    };
    let status = status_of(&current.row.status);
    if matches!(
        status,
        TaskStatus::Draft
            | TaskStatus::Creating
            | TaskStatus::Setup
            | TaskStatus::SetupFailed
            | TaskStatus::Closed
    ) {
        publish(parts, task);
        return;
    }
    let working = parts.state.runs.by_id.values().any(|run| {
        run.live()
            && run.task == Some(task)
            && matches!(
                run.arbiter.execution().value,
                Execution::Starting | Execution::Working
            )
    });
    let wanted = if working {
        TaskStatus::Running
    } else {
        TaskStatus::Idle
    };
    if wanted != status {
        set_status(parts, task, wanted);
    } else {
        publish(parts, task);
    }
    // The agent stopped: the diff changed, and now is the time to look (never on a timer).
    if wanted == TaskStatus::Idle
        && status == TaskStatus::Running
        && let Some(service) = parts.state.task_service.clone()
    {
        service.schedule_diff(task);
    }
}

/// Leases ports to a task (one transaction), and shows them.
pub(crate) fn lease(parts: &mut Parts<'_>, id: TaskId, ports: &[u16]) -> Result<(), StoreError> {
    let at = now_ms();
    parts.commit(|tx| tx.lease_ports(id.ulid(), ports, at))?;
    if let Some(task) = parts.state.tasks.by_id.get_mut(&id) {
        task.ports = ports.to_vec();
    }
    publish(parts, id);
    Ok(())
}

/// Notes the environment of the panes of a task and the blob with the output of its setup.
pub(crate) fn record_env(
    parts: &mut Parts<'_>,
    id: TaskId,
    env: serde_json::Value,
    blob: Option<String>,
) {
    change_row(parts, id, |row| {
        row.env = env;
        if blob.is_some() {
            row.setup_blob = blob;
        }
    });
}

/// Notes the workspace and the tab of a task.
pub(crate) fn attach_tab(
    parts: &mut Parts<'_>,
    id: TaskId,
    workspace: nh_proto::api::ids::WorkspaceId,
    tab: nh_proto::api::ids::TabId,
) {
    change_row(parts, id, |row| {
        row.workspace = Some(workspace.ulid());
        row.tab = Some(tab.ulid());
    });
}

fn change_row(parts: &mut Parts<'_>, id: TaskId, change: impl FnOnce(&mut TaskRow)) {
    let Some(task) = parts.state.tasks.get(id) else {
        return;
    };
    let mut row = task.row.clone();
    change(&mut row);
    row.updated_at = now_ms();
    if let Err(error) = parts.commit(|tx| tx.put_task(&row)) {
        tracing::warn!(task = %id, %error, "a task is not stored");
    }
    if let Some(task) = parts.state.tasks.by_id.get_mut(&id) {
        task.row = row;
    }
    publish(parts, id);
}

/// The task is closed: its ports go back, and the status says so. The event goes with it.
pub(crate) fn close(parts: &mut Parts<'_>, id: TaskId) {
    let Some(task) = parts.state.tasks.get(id) else {
        return;
    };
    let from = status_of(&task.row.status);
    let mut row = task.row.clone();
    row.status = status_name(TaskStatus::Closed).to_owned();
    row.updated_at = now_ms();
    let (handle, title) = (row.handle.clone(), row.title.clone());
    let subject = id.to_string();
    let stored = parts.commit(|tx| {
        tx.put_task(&row)?;
        tx.release_ports(id.ulid())?;
        tx.append(
            Some(&subject),
            &EventPayload::TaskStatusChanged(TaskStatusChanged {
                task: id,
                from,
                to: TaskStatus::Closed,
            }),
        )?;
        tx.append(
            Some(&subject),
            &EventPayload::TaskClosed(TaskEvent {
                task: id,
                handle,
                title,
            }),
        )?;
        Ok(())
    });
    if let Err(error) = stored {
        tracing::warn!(task = %id, %error, "the end of a task is not stored");
    }
    if let Some(task) = parts.state.tasks.by_id.get_mut(&id) {
        task.row = row;
        task.ports.clear();
    }
    changes::forget(parts, id);
    publish(parts, id);
}

#[cfg(test)]
mod tests {
    use nh_store::{Store, TaskRow};
    use ulid::Ulid;

    use super::Tasks;

    fn row(n: u128, status: &str, worktree: &str) -> TaskRow {
        TaskRow {
            id: Ulid::from(n),
            handle: format!("k{n}"),
            title: "Fix it".to_owned(),
            slug: "fix-it".to_owned(),
            workspace: None,
            tab: None,
            repo_root: "/r".to_owned(),
            worktree_path: worktree.to_owned(),
            branch: "neohar/fix-it".to_owned(),
            base: "main".to_owned(),
            status: status.to_owned(),
            env: serde_json::json!({}),
            setup_blob: None,
            harness: None,
            model: None,
            effort: None,
            prompt: None,
            ports: None,
            attachments: Vec::new(),
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn a_start_that_the_end_of_the_core_cut_short_is_a_draft_without_a_worktree() {
        let mut store = Store::open_in_memory().expect("a store");
        let here = std::env::temp_dir().display().to_string();
        store
            .tx(1, |tx| {
                tx.put_task(&row(1, "creating", ""))?;
                tx.put_task(&row(2, "creating", "/no/such/worktree"))?;
                tx.put_task(&row(3, "creating", &here))?;
                tx.put_task(&row(4, "setup", "/no/such/worktree"))?;
                tx.put_task(&row(5, "draft", ""))
            })
            .expect("tasks");
        let tasks = Tasks::load(&mut store, 9).expect("the tasks");
        let status = |n: u128| {
            tasks
                .get(nh_proto::api::ids::TaskId::new(Ulid::from(n)))
                .expect("a task")
                .row
                .status
                .clone()
        };
        assert_eq!(status(1), "draft");
        assert_eq!(status(2), "draft");
        assert_eq!(status(3), "setup_failed", "its worktree is there");
        assert_eq!(status(4), "setup_failed", "the setup ran in its worktree");
        assert_eq!(status(5), "draft");
        let again = store.tasks().expect("rows");
        assert_eq!(again[1].status, "draft");
        assert_eq!(
            (again[1].slug.as_str(), again[1].worktree_path.as_str()),
            ("", "")
        );
    }
}
