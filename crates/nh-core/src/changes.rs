//! `changes.follow` and `changes.diff`: the Changes tab of a window. It shows the changes of the
//! git worktree that holds the folder of the window, from `HEAD`: what is not committed. The
//! window names the folder with `changes.follow`. It is the root of its Files tab: the folder of
//! the pane with the focus, else of the tab.
//!
//! The core fills `changes_head:<window>` and `changes_for:<window>` only while the window
//! follows one of them. It watches the worktree and its git directory, and counts again after a
//! burst of changes. A window that follows again, for example after a reconnect, counts at once
//! with the folder that it named last. A count runs the commands of [`git::Git::uncommitted`],
//! and its first [`MAX_ROWS`] files go into `changes_for:<window>`. A count that fails keeps the
//! values of the count before and sets `error`.

mod git;

use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, OnceLock, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use nh_proto::api::{
    common::Empty,
    ids::WindowId,
    methods::{Call, Method},
    model::{ChangeRow, ChangeStatus, ChangeTotals, ChangesHead},
    projections::{Projection, SINGLETON_KEY},
    rich::{self, DiffStats},
};
use serde::Serialize;
use serde_json::{Value, to_value};
use tokio::runtime::Handle;

use self::git::{ChangeKind, ChangedFile, Changes, Git};
use crate::{
    ErrorCode, actor::Origin, error::RpcError, projection::ProjectionHub, runtime::Inner, watch,
    window_calls::check_owner,
};

/// The most rows of `changes_for:<window>`. `truncated` tells the page that the worktree has
/// more.
pub(crate) const MAX_ROWS: usize = 1_000;

/// The most bytes of a patch or of a new file that a diff reads, as `nh-rich` caps its input.
const DIFF_INPUT: usize = 2 << 20;

/// How often the core looks whether a window still follows its changes, while one does.
const FOLLOW_CHECK: Duration = Duration::from_secs(1);

/// How long the core waits before it counts the changes of a window again, so that a burst of
/// changes is one look.
const DIFF_DEBOUNCE: Duration = Duration::from_secs(2);

/// Whether this module runs the method.
pub(crate) fn handles(method: &str) -> bool {
    matches!(
        Method::from_name(method),
        Some(Method::ChangesFollow | Method::ChangesDiff)
    )
}

fn json_of<T: Serialize>(value: &T) -> Result<Value, RpcError> {
    to_value(value).map_err(|e| RpcError::internal(e.to_string()))
}

/// Runs a call that [`handles`] accepted.
pub(crate) async fn run(inner: &Arc<Inner>, call: Call, origin: Origin) -> Result<Value, RpcError> {
    match call {
        Call::ChangesFollow(p) => {
            inner.render.require_window(p.window)?;
            check_owner(inner, &origin, p.window)?;
            let folder = PathBuf::from(&p.path);
            if !folder.is_absolute() {
                return Err(RpcError::invalid_params("the path must be absolute"));
            }
            inner.changes.follow_folder(p.window, folder);
            json_of(&Empty {})
        }
        Call::ChangesDiff(p) => {
            inner.render.require_window(p.window)?;
            check_owner(inner, &origin, p.window)?;
            json_of(&inner.changes.window_diff(p.window, p.path).await?)
        }
        other => Err(RpcError::internal(format!("not a changes call: {other:?}"))),
    }
}

/// The last count of a window.
#[derive(Clone, Debug, Default)]
struct Counted {
    /// The totals in `changes_head:<window>`.
    totals: Option<ChangeTotals>,
    /// The full name of `HEAD` in the last count that worked.
    head: Option<String>,
    /// The rows of `changes_for:<window>` with their keys, in their order.
    rows: Vec<(String, ChangedFile)>,
    /// The number of the count that made these values. A count that started before it and ends
    /// after it changes nothing.
    seq: u64,
}

/// The Changes tab of one window that follows now.
struct Follow {
    /// The folder of the head.
    folder: PathBuf,
    /// The core knows the worktree of `folder`.
    resolved: bool,
    /// The root of the worktree, as the system names it. `None`: the folder is not in a git
    /// worktree, or the core does not know yet.
    root: Option<PathBuf>,
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
            root: None,
            watches: Vec::new(),
            counted: Counted::default(),
            stats: None,
            branch: None,
        }
    }
}

/// The Changes tabs of the windows.
#[derive(Default)]
struct Windows {
    /// The folder that each window named last.
    folders: HashMap<WindowId, PathBuf>,
    /// The windows that follow now, with their watches and their last count.
    follows: HashMap<WindowId, Follow>,
    /// The windows whose count waits for the end of a burst.
    pending: HashSet<WindowId>,
    /// The check of the followers runs.
    checking: bool,
}

impl Windows {
    /// Ends the follows of the windows that have no subscriber. Their rows and their head go, so
    /// a window that follows again starts from nothing.
    fn drop_unfollowed(&mut self, hub: &ProjectionHub) {
        self.follows.retain(|window, follow| {
            let kept = followed(hub, *window);
            if !kept {
                set_rows(
                    hub,
                    Projection::ChangesFor(*window),
                    &keys(&follow.counted.rows),
                    &[],
                );
                hub.remove(Projection::ChangesHead(*window), SINGLETON_KEY);
            }
            kept
        });
    }
}

/// The Changes tabs of the windows, and the git work behind them.
pub(crate) struct ChangesService {
    handle: Handle,
    git: Git,
    /// The number of the last count.
    count_seq: AtomicU64,
    windows: Mutex<Windows>,
    /// The projections, to know whether a window still follows its changes.
    hub: OnceLock<Arc<ProjectionHub>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
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

fn lossy(bytes: &[u8]) -> String {
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

/// The value of a row of `changes_for:<window>`.
fn row_value(path: &str, file: &ChangedFile) -> ChangeRow {
    let (dir, name) = match path.rfind('/') {
        Some(at) => (path[..=at].to_owned(), path[at + 1..].to_owned()),
        None => (String::new(), path.to_owned()),
    };
    ChangeRow {
        path: path.to_owned(),
        dir,
        name,
        old_path: file.old_path.as_deref().map(lossy),
        status: status(file.kind),
        add: file.add,
        del: file.del,
        binary: file.binary,
        stamp: file.stamp,
    }
}

/// The totals and the rows of a count that worked, with the full name of `HEAD`. Two paths that
/// are the same text after the change of bytes that are not UTF-8 are one row.
fn counted(changes: Changes) -> (ChangeTotals, String, Vec<(String, ChangedFile)>) {
    let files = changes.files.len();
    let mut seen = HashSet::new();
    let rows: Vec<(String, ChangedFile)> = changes
        .files
        .into_iter()
        .map(|file| (lossy(&file.path), file))
        .filter(|(key, _)| seen.insert(key.clone()))
        .take(MAX_ROWS)
        .collect();
    let totals = ChangeTotals {
        files: u32::try_from(files).unwrap_or(u32::MAX),
        truncated: files > MAX_ROWS,
        error: None,
    };
    (totals, changes.head, rows)
}

/// Sets the rows of a projection of changes, and removes the keys of `before` that are gone. A
/// row that did not change makes no patch.
fn set_rows(
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

fn not_a_change() -> RpcError {
    RpcError::new(
        ErrorCode::NotFound,
        "the path is not a change of the worktree",
    )
}

/// Sets `changes_head:<window>` from a follow.
fn publish_head(hub: &ProjectionHub, window: WindowId, follow: &Follow) {
    let head = ChangesHead {
        folder: text(&follow.folder),
        root: follow.root.as_deref().map(text),
        branch: follow.branch.clone(),
        changes: follow.counted.totals.clone(),
        diff_stats: follow.stats,
    };
    if let Ok(json) = serde_json::to_value(head) {
        hub.set_singleton(Projection::ChangesHead(window), json);
    }
}

impl ChangesService {
    /// `base_env` is the environment of the git commands. It must have a `PATH`.
    pub(crate) fn new(handle: Handle, base_env: &[(String, String)]) -> Arc<Self> {
        Arc::new(Self {
            handle,
            git: Git::new(base_env.to_vec()),
            count_seq: AtomicU64::new(0),
            windows: Mutex::new(Windows::default()),
            hub: OnceLock::new(),
        })
    }

    /// The projections. A window that follows `changes_for:<window>` or
    /// `changes_head:<window>` starts its Changes tab.
    pub(crate) fn bind_projections(self: &Arc<Self>, hub: &Arc<ProjectionHub>) {
        let _ = self.hub.set(Arc::clone(hub));
        let service = Arc::downgrade(self);
        hub.on_subscribe(Box::new(move |projection| {
            let Some(service) = service.upgrade() else {
                return;
            };
            if let Projection::ChangesFor(window) | Projection::ChangesHead(window) = projection {
                service.window_followed(window);
            }
        }));
    }

    /// `changes.follow`: the window shows the changes of the worktree that holds `folder`. The
    /// core starts when a window follows, now or later.
    pub(crate) fn follow_folder(self: &Arc<Self>, window: WindowId, folder: PathBuf) {
        let start = {
            let mut windows = lock(&self.windows);
            windows.folders.insert(window, folder);
            windows.follows.contains_key(&window)
                || self.hub.get().is_some_and(|hub| followed(hub, window))
        };
        if start {
            self.start_window(window);
        }
    }

    /// A window started to follow `changes_for:<window>` or `changes_head:<window>`. A window
    /// that named no folder yet waits for `changes.follow`.
    fn window_followed(self: &Arc<Self>, window: WindowId) {
        if lock(&self.windows).folders.contains_key(&window) {
            self.start_window(window);
        }
    }

    fn start_window(self: &Arc<Self>, window: WindowId) {
        let this = Arc::clone(self);
        self.handle
            .spawn(async move { this.resolve_window(window).await });
    }

    /// Starts the check of the followers, when it does not run.
    fn check(self: &Arc<Self>, windows: &mut Windows) {
        if !windows.checking {
            windows.checking = true;
            self.handle.spawn(Arc::clone(self).check_followers());
        }
    }

    /// Ends the watches of the windows that no longer follow. It stops when none is left.
    async fn check_followers(self: Arc<Self>) {
        let Some(hub) = self.hub.get().cloned() else {
            lock(&self.windows).checking = false;
            return;
        };
        loop {
            tokio::time::sleep(FOLLOW_CHECK).await;
            let mut windows = lock(&self.windows);
            windows.drop_unfollowed(&hub);
            if windows.follows.is_empty() {
                windows.checking = false;
                return;
            }
        }
    }

    /// Finds the worktree of the folder that the window named last. A new worktree clears the
    /// rows, starts its watches and counts.
    async fn resolve_window(self: Arc<Self>, window: WindowId) {
        let folder = {
            let mut windows = lock(&self.windows);
            let Some(folder) = windows.folders.get(&window).cloned() else {
                return;
            };
            self.check(&mut windows);
            let follow = windows
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
        let root = self.root_of(&folder).await;
        let Some(hub) = self.hub.get() else {
            return;
        };
        {
            let mut windows = lock(&self.windows);
            let Some(follow) = windows.follows.get_mut(&window) else {
                return;
            };
            // The window named another folder in the meantime.
            if follow.folder != folder {
                return;
            }
            follow.resolved = true;
            if follow.root == root {
                publish_head(hub, window, follow);
                return;
            }
            let before = keys(&follow.counted.rows);
            follow.branch = None;
            follow.root.clone_from(&root);
            follow.counted = Counted::default();
            follow.stats = None;
            follow.watches = Vec::new();
            set_rows(hub, Projection::ChangesFor(window), &before, &[]);
            publish_head(hub, window, follow);
        }
        let Some(root) = root else {
            return;
        };
        let weak = Arc::downgrade(&self);
        let watches = self
            .watch_worktree(&root, move |_: watch::Change| {
                if let Some(this) = weak.upgrade() {
                    this.schedule_window(window);
                }
            })
            .await;
        {
            let mut windows = lock(&self.windows);
            match windows.follows.get_mut(&window) {
                Some(follow) if follow.root.as_ref() == Some(&root) => {
                    follow.watches = watches;
                }
                _ => return,
            }
        }
        self.count_window(window).await;
    }

    /// The root of the worktree that holds a folder. `None` for a folder that is not in a git
    /// worktree, or that is gone.
    async fn root_of(&self, folder: &Path) -> Option<PathBuf> {
        let root = self.git.repo_root(folder).await.ok()?;
        Some(std::fs::canonicalize(&root).unwrap_or(root))
    }

    /// Counts the changes of the worktree of a window now, and sets its rows and its head. A
    /// count of a worktree that the window left changes nothing.
    async fn count_window(&self, window: WindowId) {
        let root = lock(&self.windows)
            .follows
            .get(&window)
            .and_then(|follow| follow.root.clone());
        let Some(root) = root else {
            return;
        };
        let seq = self.count_seq.fetch_add(1, Ordering::Relaxed) + 1;
        let result = self
            .git
            .uncommitted(&root)
            .await
            .map_err(|error| error.to_string());
        let Some(hub) = self.hub.get() else {
            return;
        };
        let mut windows = lock(&self.windows);
        let Some(follow) = windows.follows.get_mut(&window) else {
            return;
        };
        if follow.root.as_ref() != Some(&root) || seq < follow.counted.seq {
            return;
        }
        follow.counted.seq = seq;
        match result {
            Ok((changes, branch)) => {
                follow.stats = Some(changes.stats());
                follow.branch = branch;
                let (totals, head, rows) = counted(changes);
                follow.counted.totals = Some(totals);
                follow.counted.head = Some(head);
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
        if !lock(&self.windows).pending.insert(window) {
            return;
        }
        let this = Arc::clone(self);
        self.handle.spawn(async move {
            tokio::time::sleep(DIFF_DEBOUNCE).await;
            lock(&this.windows).pending.remove(&window);
            this.count_window(window).await;
        });
    }

    /// `changes.diff`: the diff of one path of the rows of a window. A path that the last count
    /// does not have causes a count, and a path that is still not a row is `not_found`. So the
    /// call reads no file that is not a change of the worktree.
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
        let (root, file, head) = found.ok_or_else(not_a_change)?;
        self.diff_of(&root, path, &file, head).await
    }

    /// The row of `path` in the last count of a window, with the root and `HEAD`.
    fn window_row(
        &self,
        window: WindowId,
        path: &str,
    ) -> Result<Option<(PathBuf, ChangedFile, Option<String>)>, RpcError> {
        let windows = lock(&self.windows);
        let follow = windows
            .follows
            .get(&window)
            .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "the window follows no changes"))?;
        let Some(root) = &follow.root else {
            return Ok(None);
        };
        Ok(follow
            .counted
            .rows
            .iter()
            .find(|(key, _)| key == path)
            .map(|(_, file)| (root.clone(), file.clone(), follow.counted.head.clone())))
    }

    /// The diff of one row of a count, from `HEAD` to the worktree.
    async fn diff_of(
        &self,
        worktree: &Path,
        path: String,
        file: &ChangedFile,
        head: Option<String>,
    ) -> Result<rich::FileDiff, RpcError> {
        let mut diff = if file.kind == ChangeKind::Untracked {
            self.new_file_diff(worktree, &path, file).await?
        } else {
            let head = head.ok_or_else(not_a_change)?;
            let patch = self
                .git
                .file_diff(worktree, &head, file)
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
            // The file is as `HEAD` has it again.
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

    /// The watches of a worktree and of its git directory, with `cause` for each change. The app
    /// watches both trees with all their directories. `nh daemon` has no watcher of the system,
    /// and a poll of a whole tree costs too much, so it polls only the entries of the
    /// git directory.
    async fn watch_worktree(
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
