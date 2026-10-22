//! The Changes tab of a window: the changes of the git worktree that holds the folder of
//! the window. The window names the folder with `changes.follow`. It is the root of its Files tab:
//! the folder of the pane with the focus, else of the tab.
//!
//! A task whose worktree is that worktree gives its base, and the changes go from the merge base,
//! as for `changes:<task>`. The changes of any other worktree go from `HEAD`: what is not
//! committed. The core fills `changes_head:<window>` and `changes_for:<window>` only while the
//! window follows one of them. It watches the worktree as it watches the worktree of a task, and
//! counts again after a burst of changes. A window that follows again, for example after a
//! reconnect, counts at once with the folder that it named last.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering},
};

use nh_proto::api::{
    ids::{TaskId, WindowId},
    model::{ChangeTotals, ChangesHead},
    projections::Projection,
    rich::{self, DiffStats},
};
use nh_tasks::ChangedFile;

use super::{
    changes::{Counted, counted, lock, not_a_change, set_rows},
    service::{DIFF_DEBOUNCE, TaskService},
};
use crate::{ErrorCode, error::RpcError, projection::ProjectionHub, watch};

/// The worktree whose changes a window shows.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Subject {
    /// The root of the worktree, as the system names it.
    root: PathBuf,
    /// The task whose worktree is the root.
    task: Option<OfTask>,
}

/// The task of a worktree: the changes go from the merge base of its base and `HEAD`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct OfTask {
    id: TaskId,
    branch: String,
    base: String,
}

/// The Changes tab of one window that follows now.
struct Follow {
    /// The folder of the head.
    folder: PathBuf,
    /// The core knows the worktree of `folder`.
    resolved: bool,
    /// `None`: the folder is not in a git worktree, or the core does not know yet.
    subject: Option<Subject>,
    watches: Vec<watch::DirWatch>,
    counted: Counted,
    stats: Option<DiffStats>,
    branch: Option<String>,
}

impl Follow {
    fn new(folder: PathBuf) -> Self {
        Self {
            folder,
            resolved: false,
            subject: None,
            watches: Vec::new(),
            counted: Counted::default(),
            stats: None,
            branch: None,
        }
    }
}

/// The Changes tabs of the windows.
#[derive(Default)]
pub(crate) struct Windows {
    /// The folder that each window named last.
    folders: HashMap<WindowId, PathBuf>,
    /// The windows that follow now, with their watches and their last count.
    follows: HashMap<WindowId, Follow>,
    /// The windows whose count waits for the end of a burst.
    pending: HashSet<WindowId>,
}

impl Windows {
    pub(super) fn is_empty(&self) -> bool {
        self.follows.is_empty()
    }

    /// Ends the follows of the windows that have no subscriber. Their rows and their head go, so
    /// a window that follows again starts from nothing.
    pub(super) fn drop_unfollowed(&mut self, hub: &ProjectionHub) {
        self.follows.retain(|window, follow| {
            let kept = followed(hub, *window);
            if !kept {
                set_rows(
                    hub,
                    Projection::ChangesFor(*window),
                    &keys(&follow.counted.rows),
                    &[],
                );
                hub.remove(
                    Projection::ChangesHead(*window),
                    nh_proto::api::projections::SINGLETON_KEY,
                );
            }
            kept
        });
    }
}

/// Whether a window follows `changes_for:<window>` or `changes_head:<window>`.
fn followed(hub: &ProjectionHub, window: WindowId) -> bool {
    hub.subscriber_count(Projection::ChangesFor(window)) > 0
        || hub.subscriber_count(Projection::ChangesHead(window)) > 0
}

fn keys(rows: &[(String, ChangedFile)]) -> Vec<String> {
    rows.iter().map(|(key, _)| key.clone()).collect()
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Sets `changes_head:<window>` from a follow.
fn publish_head(hub: &ProjectionHub, window: WindowId, follow: &Follow) {
    let subject = follow.subject.as_ref();
    let task = subject.and_then(|subject| subject.task.as_ref());
    let head = ChangesHead {
        folder: text(&follow.folder),
        root: subject.map(|subject| text(&subject.root)),
        branch: follow.branch.clone(),
        task: task.map(|task| task.id),
        base: task.map(|task| task.base.clone()),
        changes: follow.counted.totals.clone(),
        diff_stats: follow.stats,
    };
    if let Ok(json) = serde_json::to_value(head) {
        hub.set_singleton(Projection::ChangesHead(window), json);
    }
}

impl TaskService {
    /// `changes.follow`: the window shows the changes of the worktree that holds `folder`. The
    /// core starts when a window follows, now or later.
    pub(crate) fn follow_folder(self: &Arc<Self>, window: WindowId, folder: PathBuf) {
        let start = {
            let mut follows = lock(&self.follows);
            follows.windows.folders.insert(window, folder);
            follows.windows.follows.contains_key(&window)
                || self.hub.get().is_some_and(|hub| followed(hub, window))
        };
        if start {
            self.start_window(window);
        }
    }

    /// A window started to follow `changes_for:<window>` or `changes_head:<window>`. A window
    /// that named no folder yet waits for `changes.follow`.
    pub(crate) fn window_followed(self: &Arc<Self>, window: WindowId) {
        if lock(&self.follows).windows.folders.contains_key(&window) {
            self.start_window(window);
        }
    }

    fn start_window(self: &Arc<Self>, window: WindowId) {
        let this = Arc::clone(self);
        self.handle
            .spawn(async move { this.resolve_window(window).await });
    }

    /// Finds the worktree of the folder that the window named last. A new worktree clears the
    /// rows, starts its watches and counts.
    async fn resolve_window(self: Arc<Self>, window: WindowId) {
        let folder = {
            let mut follows = lock(&self.follows);
            let Some(folder) = follows.windows.folders.get(&window).cloned() else {
                return;
            };
            follows.check(&self);
            let follow = follows
                .windows
                .follows
                .entry(window)
                .or_insert_with(|| Follow::new(folder.clone()));
            if follow.folder == folder && follow.resolved {
                return;
            }
            if follow.folder != folder {
                follow.folder = folder.clone();
                follow.resolved = false;
            }
            folder
        };
        let subject = self.subject_of(&folder).await;
        let Some(hub) = self.hub.get() else {
            return;
        };
        {
            let mut follows = lock(&self.follows);
            let Some(follow) = follows.windows.follows.get_mut(&window) else {
                return;
            };
            // The window named another folder in the meantime.
            if follow.folder != folder {
                return;
            }
            follow.resolved = true;
            if follow.subject == subject {
                publish_head(hub, window, follow);
                return;
            }
            let before = keys(&follow.counted.rows);
            follow.branch = subject
                .as_ref()
                .and_then(|subject| subject.task.as_ref())
                .map(|task| task.branch.clone());
            follow.subject.clone_from(&subject);
            follow.counted = Counted::default();
            follow.stats = None;
            follow.watches = Vec::new();
            set_rows(hub, Projection::ChangesFor(window), &before, &[]);
            publish_head(hub, window, follow);
        }
        let Some(subject) = subject else {
            return;
        };
        let weak = Arc::downgrade(&self);
        let watches = self
            .watch_worktree(&subject.root, move |_: watch::Change| {
                if let Some(this) = weak.upgrade() {
                    this.schedule_window(window);
                }
            })
            .await;
        {
            let mut follows = lock(&self.follows);
            match follows.windows.follows.get_mut(&window) {
                Some(follow) if follow.subject.as_ref() == Some(&subject) => {
                    follow.watches = watches;
                }
                _ => return,
            }
        }
        self.count_window(window).await;
    }

    /// The worktree that holds a folder, with the task whose worktree it is. `None` for a folder
    /// that is not in a git worktree, or that is gone.
    async fn subject_of(&self, folder: &Path) -> Option<Subject> {
        let root = self.git.repo_root(folder).await.ok()?;
        let root = std::fs::canonicalize(&root).unwrap_or(root);
        let tasks: Vec<(TaskId, String, OfTask)> = self
            .actor()
            .ok()?
            .update(|parts| {
                parts
                    .state
                    .tasks
                    .by_id
                    .iter()
                    .filter(|(_, task)| {
                        !task.row.worktree_path.is_empty()
                            && !matches!(task.row.status.as_str(), "draft" | "closed")
                    })
                    .map(|(id, task)| {
                        let of = OfTask {
                            id: *id,
                            branch: task.row.branch.clone(),
                            base: task.row.base.clone(),
                        };
                        (*id, task.row.worktree_path.clone(), of)
                    })
                    .collect()
            })
            .await
            .ok()?;
        let task = tasks
            .into_iter()
            .find(|(_, worktree, _)| std::fs::canonicalize(worktree).is_ok_and(|w| w == root))
            .map(|(_, _, task)| task);
        Some(Subject { root, task })
    }

    /// Counts the changes of the worktree of a window now, and sets its rows and its head. A
    /// count of a worktree that the window left changes nothing.
    pub(super) async fn count_window(&self, window: WindowId) {
        let subject = lock(&self.follows)
            .windows
            .follows
            .get(&window)
            .and_then(|follow| follow.subject.clone());
        let Some(subject) = subject else {
            return;
        };
        let seq = self.count_seq.fetch_add(1, Ordering::Relaxed) + 1;
        let result = match &subject.task {
            Some(task) => self
                .git
                .changes(&subject.root, &task.base)
                .await
                .map(|changes| (changes, Some(task.branch.clone()))),
            None => self.git.uncommitted(&subject.root).await,
        }
        .map_err(|error| error.to_string());
        let Some(hub) = self.hub.get() else {
            return;
        };
        let mut follows = lock(&self.follows);
        let Some(follow) = follows.windows.follows.get_mut(&window) else {
            return;
        };
        if follow.subject.as_ref() != Some(&subject) || seq < follow.counted.seq {
            return;
        }
        follow.counted.seq = seq;
        match result {
            Ok((changes, branch)) => {
                follow.stats = Some(changes.stats());
                follow.branch = branch;
                let (totals, merge_base, rows) = counted(changes);
                follow.counted.totals = Some(totals);
                follow.counted.merge_base = Some(merge_base);
                let before = keys(&std::mem::replace(&mut follow.counted.rows, rows));
                set_rows(
                    hub,
                    Projection::ChangesFor(window),
                    &before,
                    &follow.counted.rows,
                );
            }
            Err(error) => {
                tracing::debug!(%window, %error, "the changes of a window are not counted");
                follow
                    .counted
                    .totals
                    .get_or_insert(ChangeTotals {
                        files: 0,
                        commits: 0,
                        merge_base: None,
                        truncated: false,
                        error: None,
                    })
                    .error = Some(error);
            }
        }
        publish_head(hub, window, follow);
    }

    /// Counts the changes of a window in a moment. A burst of causes is one count.
    fn schedule_window(self: &Arc<Self>, window: WindowId) {
        if !lock(&self.follows).windows.pending.insert(window) {
            return;
        }
        let this = Arc::clone(self);
        self.handle.spawn(async move {
            tokio::time::sleep(DIFF_DEBOUNCE).await;
            lock(&this.follows).windows.pending.remove(&window);
            this.count_window(window).await;
        });
    }

    /// The causes of a task count the windows that show its worktree too. `nh daemon` watches
    /// only the git directory, and the hooks of the agents tell it of a changed file.
    pub(super) fn schedule_windows_of(self: &Arc<Self>, task: TaskId) {
        let windows: Vec<WindowId> = lock(&self.follows)
            .windows
            .follows
            .iter()
            .filter(|(_, follow)| {
                follow
                    .subject
                    .as_ref()
                    .and_then(|subject| subject.task.as_ref())
                    .is_some_and(|of| of.id == task)
            })
            .map(|(window, _)| *window)
            .collect();
        for window in windows {
            self.schedule_window(window);
        }
    }

    /// `changes.diff`: the diff of one path of the rows of a window. A path that the last count
    /// does not have causes a count, and a path that is still not a row is `not_found`.
    pub(crate) async fn window_diff(
        &self,
        window: WindowId,
        path: String,
    ) -> Result<rich::FileDiff, RpcError> {
        let found = match self.window_row(window, &path)? {
            Some(found) => Some(found),
            None => {
                self.count_window(window).await;
                self.window_row(window, &path)?
            }
        };
        let (root, file, merge_base) = found.ok_or_else(not_a_change)?;
        self.diff_of(&root, path, &file, merge_base).await
    }

    /// The row of `path` in the last count of a window, with the root and the merge base.
    fn window_row(
        &self,
        window: WindowId,
        path: &str,
    ) -> Result<Option<(PathBuf, ChangedFile, Option<String>)>, RpcError> {
        let follows = lock(&self.follows);
        let follow =
            follows.windows.follows.get(&window).ok_or_else(|| {
                RpcError::new(ErrorCode::NotFound, "the window follows no changes")
            })?;
        let Some(subject) = &follow.subject else {
            return Ok(None);
        };
        Ok(follow
            .counted
            .rows
            .iter()
            .find(|(key, _)| key == path)
            .map(|(_, file)| {
                (
                    subject.root.clone(),
                    file.clone(),
                    follow.counted.merge_base.clone(),
                )
            }))
    }
}
