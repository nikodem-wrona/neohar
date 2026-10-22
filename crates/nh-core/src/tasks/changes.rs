//! The changes of a task in the core: the count, the projection `changes:<task>`, the
//! diff of one file, and the watches while a window follows the projection.
//!
//! A count runs the commands of [`nh_tasks::Git::changes`]. Its totals go into the row of
//! `tasks`, and its first [`MAX_ROWS`] files into `changes:<task>`. A count that fails keeps the
//! values of the count before and sets `error`. Some events of a task cause a count
//! (`TaskService::schedule_diff`). While a window follows `changes:<task>`, the worktree and its
//! git directory are causes too.
//! The Changes tab of a window uses the same rows, diffs and watches (`folder_changes`).

use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError, atomic::Ordering},
    time::Duration,
};

use nh_proto::api::{
    calls::agent::TaskChangesResult,
    ids::{Ref, TaskId},
    model::{ChangeRow, ChangeStatus, ChangeTotals},
    projections::Projection,
    rich,
};
use nh_tasks::{ChangeKind, ChangedFile, Changes};

use super::service::{TaskService, gone};
use crate::{ErrorCode, actor::Parts, error::RpcError, projection::ProjectionHub, watch};

/// The most rows of `changes:<task>`. `truncated` tells the page that the task has more.
pub(crate) const MAX_ROWS: usize = 1_000;

/// The most bytes of a patch or of a new file that a diff reads, as `nh-rich` caps its input.
const DIFF_INPUT: usize = 2 << 20;

/// How often the core looks whether a window still follows the changes of a task, while one does.
const FOLLOW_CHECK: Duration = Duration::from_secs(1);

/// The last count of a task.
#[derive(Clone, Debug, Default)]
pub(crate) struct Counted {
    /// The totals in the row of `tasks`.
    pub totals: Option<ChangeTotals>,
    /// The full name of the merge base of the last count that worked.
    pub merge_base: Option<String>,
    /// The rows of `changes:<task>` with their keys, in their order.
    pub rows: Vec<(String, ChangedFile)>,
    /// The number of the count that made these values. A count that started before it and ends
    /// after it changes nothing.
    pub seq: u64,
}

/// The tasks that a window follows, with their watches, and the Changes tabs of the windows.
#[derive(Default)]
pub(crate) struct Follows {
    tasks: HashMap<TaskId, Vec<watch::DirWatch>>,
    /// The Changes tabs of the windows.
    pub(super) windows: super::folder_changes::Windows,
    /// The check of the followers runs.
    checking: bool,
}

impl Follows {
    /// Starts the check of the followers, when it does not run.
    pub(super) fn check(&mut self, service: &Arc<TaskService>) {
        if !self.checking {
            self.checking = true;
            service.handle.spawn(Arc::clone(service).check_followers());
        }
    }
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn status(kind: ChangeKind) -> ChangeStatus {
    match kind {
        ChangeKind::Modified => ChangeStatus::Modified,
        ChangeKind::Added => ChangeStatus::Added,
        ChangeKind::Deleted => ChangeStatus::Deleted,
        ChangeKind::Renamed => ChangeStatus::Renamed,
        ChangeKind::Untracked => ChangeStatus::Untracked,
    }
}

/// The value of a row of `changes:<task>`.
pub(crate) fn row_value(path: &str, file: &ChangedFile) -> ChangeRow {
    let (dir, name) = match path.rfind('/') {
        Some(at) => (path[..=at].to_owned(), path[at + 1..].to_owned()),
        None => (String::new(), path.to_owned()),
    };
    ChangeRow {
        path: path.to_owned(),
        dir,
        name,
        old_path: file.old_path.as_deref().map(text),
        status: status(file.kind),
        add: file.add,
        del: file.del,
        binary: file.binary,
        uncommitted: file.uncommitted,
        stamp: file.stamp,
    }
}

/// The totals and the rows of a count that worked. Two paths that are the same text after the
/// change of bytes that are not UTF-8 are one row.
pub(crate) fn counted(changes: Changes) -> (ChangeTotals, String, Vec<(String, ChangedFile)>) {
    let files = changes.files.len();
    let mut seen = HashSet::new();
    let rows: Vec<(String, ChangedFile)> = changes
        .files
        .into_iter()
        .map(|file| (text(&file.path), file))
        .filter(|(key, _)| seen.insert(key.clone()))
        .take(MAX_ROWS)
        .collect();
    let totals = ChangeTotals {
        files: u32::try_from(files).unwrap_or(u32::MAX),
        commits: changes.commits,
        // A count from `HEAD` of a repository with no commit has no merge base to name.
        merge_base: (!changes.short.is_empty()).then_some(changes.short),
        truncated: files > MAX_ROWS,
        error: None,
    };
    (totals, changes.merge_base, rows)
}

/// Stores a count of a task: its totals in the row of `tasks`, and its rows in `changes:<task>`.
/// A count that failed keeps the values before and sets `error`. A closed task keeps the values
/// and shows no rows, because the core forgot its projection.
pub(crate) fn apply(parts: &mut Parts<'_>, id: TaskId, seq: u64, result: Result<Changes, String>) {
    let Some(task) = parts.state.tasks.by_id.get_mut(&id) else {
        return;
    };
    if seq < task.counted.seq {
        return;
    }
    task.counted.seq = seq;
    let before: Vec<String> = match result {
        Ok(changes) => {
            let stats = changes.stats();
            let (totals, merge_base, rows) = counted(changes);
            task.diff = Some(stats);
            task.counted.totals = Some(totals);
            task.counted.merge_base = Some(merge_base);
            let before = std::mem::replace(&mut task.counted.rows, rows);
            before.into_iter().map(|(key, _)| key).collect()
        }
        Err(error) => {
            task.counted
                .totals
                .get_or_insert(ChangeTotals {
                    files: 0,
                    commits: 0,
                    merge_base: None,
                    truncated: false,
                    error: None,
                })
                .error = Some(error);
            Vec::new()
        }
    };
    let closed = task.row.status == "closed";
    if !closed {
        publish_rows(parts, id, &before);
    }
    super::publish(parts, id);
}

/// Sets the rows of `changes:<task>`, and removes the keys of `before` that are gone.
fn publish_rows(parts: &Parts<'_>, id: TaskId, before: &[String]) {
    if let Some(task) = parts.state.tasks.get(id) {
        set_rows(
            parts.projections,
            Projection::Changes(id),
            before,
            &task.counted.rows,
        );
    }
}

/// Sets the rows of a projection of changes, and removes the keys of `before` that are gone. A
/// row that did not change makes no patch.
pub(super) fn set_rows(
    hub: &ProjectionHub,
    projection: Projection,
    before: &[String],
    rows: &[(String, ChangedFile)],
) {
    let keys: Vec<String> = rows.iter().map(|(k, _)| k.clone()).collect();
    let now: HashSet<&String> = keys.iter().collect();
    for key in before.iter().filter(|key| !now.contains(key)) {
        hub.remove(projection, key);
    }
    for (key, file) in rows {
        if let Ok(json) = serde_json::to_value(row_value(key, file)) {
            hub.set(projection, key, json);
        }
    }
    hub.set_order(projection, &keys);
}

/// The core forgets the rows of a task that closes. Its totals stay in its row.
pub(crate) fn forget(parts: &mut Parts<'_>, id: TaskId) {
    parts.projections.forget(Projection::Changes(id));
    if let Some(task) = parts.state.tasks.by_id.get_mut(&id) {
        task.counted.rows.clear();
    }
}

pub(super) fn not_a_change() -> RpcError {
    RpcError::new(ErrorCode::NotFound, "the path is not a change of the task")
}

impl TaskService {
    /// Counts the changes of a task now and stores them. A count that `explicit` does not ask
    /// for leaves out a draft, a task that makes its worktree now, and a closed task.
    pub(crate) async fn count(&self, task: TaskId, explicit: bool) -> Result<(), RpcError> {
        let actor = self.actor()?;
        let found = actor
            .update(move |parts| {
                parts.state.tasks.get(task).map(|t| {
                    (
                        t.row.status.clone(),
                        t.row.worktree_path.clone(),
                        t.row.base.clone(),
                    )
                })
            })
            .await
            .map_err(gone)?;
        let Some((status, worktree, base)) = found else {
            return Err(RpcError::new(ErrorCode::NotFound, "no such task"));
        };
        if status == "draft" {
            return Err(RpcError::invalid_params(
                "the task is a draft and has no changes",
            ));
        }
        if !explicit && matches!(status.as_str(), "creating" | "closed") {
            return Ok(());
        }
        let seq = self.count_seq.fetch_add(1, Ordering::Relaxed) + 1;
        let result = self
            .git
            .changes(Path::new(&worktree), &base)
            .await
            .map_err(|error| error.to_string());
        if let Err(error) = &result {
            tracing::debug!(%task, %error, "the changes of a task are not counted");
        }
        actor
            .update(move |mut parts| apply(&mut parts, task, seq, result))
            .await
            .map_err(gone)
    }

    /// The worktree of a task that started, for `task.changes` and `task.diff`.
    async fn started(&self, reference: Ref<TaskId>) -> Result<(TaskId, PathBuf), RpcError> {
        let (id, row) = self.find(reference).await?;
        if row.status == "draft" {
            return Err(RpcError::invalid_params(
                "the task is a draft and has no changes",
            ));
        }
        let worktree = PathBuf::from(&row.worktree_path);
        if row.worktree_path.is_empty() || !worktree.is_dir() {
            return Err(RpcError::new(
                ErrorCode::NotFound,
                "the worktree of the task is gone",
            ));
        }
        Ok((id, worktree))
    }

    /// `task.changes`: counts now, and gives the totals and the rows.
    pub(crate) async fn changes(
        &self,
        reference: Ref<TaskId>,
    ) -> Result<TaskChangesResult, RpcError> {
        let (id, _) = self.started(reference).await?;
        self.count(id, true).await?;
        self.actor()?
            .update(move |parts| {
                let task = parts.state.tasks.get(id)?;
                Some(TaskChangesResult {
                    changes: task.counted.totals.clone()?,
                    files: task
                        .counted
                        .rows
                        .iter()
                        .map(|(key, file)| row_value(key, file))
                        .collect(),
                })
            })
            .await
            .map_err(gone)?
            .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no such task"))
    }

    /// The row of `path` in the last count, with the merge base.
    async fn row_of(
        &self,
        id: TaskId,
        path: &str,
    ) -> Result<Option<(ChangedFile, Option<String>)>, RpcError> {
        let path = path.to_owned();
        self.actor()?
            .update(move |parts| {
                let task = parts.state.tasks.get(id)?;
                let (_, file) = task.counted.rows.iter().find(|(key, _)| *key == path)?;
                Some((file.clone(), task.counted.merge_base.clone()))
            })
            .await
            .map_err(gone)
    }

    /// `task.diff`: the diff of one path of the rows, from the merge base to the worktree. A path
    /// that the last count does not have causes a count, and a path that is still not a row is
    /// `not_found`. So the call reads no file that is not a change of the task.
    pub(crate) async fn diff(
        &self,
        reference: Ref<TaskId>,
        path: String,
    ) -> Result<rich::FileDiff, RpcError> {
        let (id, worktree) = self.started(reference).await?;
        let found = match self.row_of(id, &path).await? {
            Some(found) => Some(found),
            None => {
                self.count(id, true).await?;
                self.row_of(id, &path).await?
            }
        };
        let (file, merge_base) = found.ok_or_else(not_a_change)?;
        self.diff_of(&worktree, path, &file, merge_base).await
    }

    /// The diff of one row of a count, from its merge base to the worktree.
    pub(super) async fn diff_of(
        &self,
        worktree: &Path,
        path: String,
        file: &ChangedFile,
        merge_base: Option<String>,
    ) -> Result<rich::FileDiff, RpcError> {
        let mut diff = if file.kind == ChangeKind::Untracked {
            self.new_file_diff(worktree, &path, file).await?
        } else {
            let merge_base = merge_base.ok_or_else(not_a_change)?;
            let patch = self
                .git
                .file_diff(worktree, &merge_base, file)
                .await
                .map_err(|error| RpcError::new(ErrorCode::GitFailed, error.to_string()))?;
            let name = path.clone();
            let cut = patch.len() > DIFF_INPUT;
            let mut diff = self
                .rich(move || {
                    let patch = nh_rich::cap(&patch, DIFF_INPUT);
                    nh_rich::diff::diff(&[nh_rich::diff::Change::Patch {
                        path: Some(&name),
                        status: None,
                        text: patch.text,
                    }])
                })
                .await?;
            diff.truncated |= cut;
            diff
        };
        diff.path = path;
        diff.status = match file.kind {
            ChangeKind::Added | ChangeKind::Untracked => rich::FileStatus::Added,
            ChangeKind::Deleted => rich::FileStatus::Deleted,
            ChangeKind::Renamed => rich::FileStatus::Renamed,
            ChangeKind::Modified => rich::FileStatus::Modified,
        };
        if file.binary {
            diff.hunks.clear();
        }
        Ok(diff)
    }

    /// The diff of a file that git does not track: all its lines are added.
    async fn new_file_diff(
        &self,
        worktree: &Path,
        path: &str,
        file: &ChangedFile,
    ) -> Result<rich::FileDiff, RpcError> {
        let empty = || rich::FileDiff {
            path: path.to_owned(),
            status: rich::FileStatus::Added,
            hunks: Vec::new(),
            stats: rich::DiffStats { add: 0, del: 0 },
            truncated: false,
        };
        if file.binary {
            return Ok(empty());
        }
        let full = worktree.join(OsStr::from_bytes(&file.path));
        let name = path.to_owned();
        let meta = std::fs::symlink_metadata(&full).ok();
        let cut = meta
            .as_ref()
            .is_some_and(|meta| meta.is_file() && meta.len() > DIFF_INPUT as u64);
        let mut diff = self
            .rich(move || {
                use std::io::Read;
                // A file that went away after the count has no lines. A symlink shows its target,
                // as git shows it. A FIFO, a socket or a device is never opened: a read of a FIFO
                // can wait for ever.
                let text = match meta {
                    Some(meta) if meta.file_type().is_symlink() => std::fs::read_link(&full)
                        .map(|target| target.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    Some(meta) if meta.is_file() => {
                        let mut bytes = Vec::new();
                        if let Ok(opened) = std::fs::File::open(&full) {
                            let _ = opened.take(DIFF_INPUT as u64).read_to_end(&mut bytes);
                        }
                        String::from_utf8_lossy(&bytes).into_owned()
                    }
                    _ => String::new(),
                };
                nh_rich::diff::between("", &text, Some(&name), None)
            })
            .await?;
        diff.truncated |= cut;
        Ok(diff)
    }

    /// Runs the work of `nh-rich` on the blocking pool, and gives its one file.
    async fn rich(
        &self,
        work: impl FnOnce() -> nh_rich::diff::Diff + Send + 'static,
    ) -> Result<rich::FileDiff, RpcError> {
        let diff = self
            .handle
            .spawn_blocking(work)
            .await
            .map_err(|e| RpcError::internal(format!("the diff failed: {e}")))?;
        let mut files = crate::rich_calls::diff(&diff).files;
        Ok(if files.is_empty() {
            // The file is as the merge base has it again.
            rich::FileDiff {
                path: String::new(),
                status: rich::FileStatus::Modified,
                hunks: Vec::new(),
                stats: rich::DiffStats { add: 0, del: 0 },
                truncated: false,
            }
        } else {
            files.swap_remove(0)
        })
    }

    // --- the followers of `changes:<task>` -------------------------------------------------------

    /// A window started to follow `changes:<task>`. The first follower causes a count now, and
    /// the watches of the worktree and of its git directory start.
    pub(crate) fn followed(self: &Arc<Self>, task: TaskId) {
        {
            let mut follows = lock(&self.follows);
            if follows.tasks.contains_key(&task) {
                return;
            }
            follows.tasks.insert(task, Vec::new());
            follows.check(self);
        }
        let this = Arc::clone(self);
        self.handle.spawn(async move {
            // The watches start before the count: a change during the start is in the count, or a
            // watch sees it.
            let watches = this.watches(task).await;
            if let Some(slot) = lock(&this.follows).tasks.get_mut(&task) {
                *slot = watches;
            }
            let _ = this.count(task, false).await;
        });
    }

    /// Ends the watches of the tasks and of the windows that no window follows any more. It stops
    /// when none is left.
    async fn check_followers(self: Arc<Self>) {
        let Some(hub) = self.hub.get().cloned() else {
            lock(&self.follows).checking = false;
            return;
        };
        loop {
            tokio::time::sleep(FOLLOW_CHECK).await;
            let mut follows = lock(&self.follows);
            follows
                .tasks
                .retain(|task, _| hub.subscriber_count(Projection::Changes(*task)) > 0);
            follows.windows.drop_unfollowed(&hub);
            if follows.tasks.is_empty() && follows.windows.is_empty() {
                follows.checking = false;
                return;
            }
        }
    }

    /// The watches of the worktree of a task.
    async fn watches(self: &Arc<Self>, task: TaskId) -> Vec<watch::DirWatch> {
        let Ok(actor) = self.actor() else {
            return Vec::new();
        };
        let worktree = actor
            .update(move |parts| {
                parts
                    .state
                    .tasks
                    .get(task)
                    .filter(|t| !matches!(t.row.status.as_str(), "draft" | "closed"))
                    .map(|t| PathBuf::from(&t.row.worktree_path))
            })
            .await
            .ok()
            .flatten();
        let Some(worktree) = worktree.filter(|w| w.is_dir()) else {
            return Vec::new();
        };
        let weak = Arc::downgrade(self);
        self.watch_worktree(&worktree, move |_: watch::Change| {
            if let Some(this) = weak.upgrade() {
                this.schedule_diff(task);
            }
        })
        .await
    }

    /// The watches of a worktree and of its git directory, with `cause` for each change. The app
    /// watches both trees with all their directories. `nh daemon` has no watcher of the system,
    /// and a poll of a whole tree costs too much, so it polls only the entries of the
    /// git directory.
    pub(super) async fn watch_worktree(
        &self,
        worktree: &Path,
        cause: impl Fn(watch::Change) + Clone + Send + Sync + 'static,
    ) -> Vec<watch::DirWatch> {
        let git_dir = match self.git.git_dir(worktree).await {
            Ok(dir) => dir,
            Err(error) => {
                tracing::debug!(worktree = %worktree.display(), %error, "no git directory");
                return Vec::new();
            }
        };
        let mut out = Vec::new();
        match watch::Kind::of_build() {
            watch::Kind::System => {
                for dir in [worktree, git_dir.as_path()] {
                    match watch::watch_tree(dir, cause.clone()) {
                        Ok(Some(watch)) => out.push(watch),
                        Ok(None) => {}
                        Err(error) => tracing::debug!(%error, "no watch of the changes"),
                    }
                }
            }
            watch::Kind::Poll => match watch::watch_dir(watch::Kind::Poll, &git_dir, cause) {
                Ok(watch) => out.push(watch),
                Err(error) => tracing::debug!(%error, "no watch of the changes"),
            },
        }
        out
    }
}

#[cfg(test)]
mod tests;
