//! `git.*`: the Git view of a window. It shows one git worktree: its repository, its branch, its
//! changes and its history, the other worktrees of the repository, and the agents that work in
//! each worktree. `git_repos` lists the repositories of this machine for the Repository menu.
//!
//! The window names a folder with `git.follow`. The core fills `git_head:<window>`,
//! `git_worktrees:<window>`, `git_branches:<window>` and `git_changes:<window>` only while the
//! window follows one of them, and `git_repos` only while a client follows it. It watches the
//! worktree and the git directory of its repository, and looks again after a burst of changes. A
//! window that follows nothing costs no git command and no watch.
//!
//! The same reads serve the CLI without a window: `git.status`, `git.worktrees`, `git.branches`,
//! `git.log`, `git.show` and `git.diff` take a `path` in a worktree, and give the rows that the
//! projections have. The core runs the `git` program as the Changes tab does
//! ([`crate::changes::git::Git`]), so the configuration of the person applies.

mod agents;
mod read;
#[cfg(test)]
mod tests;

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, OnceLock, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use nh_proto::api::{
    calls::git::{
        GIT_LOG_LIMIT, GitBranchList, GitLogResult, GitRepoList, GitShown, GitStatus,
        GitWorktreeList,
    },
    common::Empty,
    events::{EventPayload, GitRepoAdded},
    ids::WindowId,
    methods::{Call, Method},
    model::{
        ChangeRow, ChangeTotals, GitAgent, GitAgentState, GitBranchValue, GitCommit, GitHead,
        GitRepoSource, GitRepoValue, GitWorktreeValue, WorkspaceValue,
    },
    projections::{Projection, SINGLETON_KEY},
    rich::{self, DiffStats},
    time::Timestamp,
};
use serde::Serialize;
use serde_json::{Value, to_value};
use tokio::runtime::Handle;

use self::{
    agents::{Folders, Placed},
    read::{Branch, Kind, Place, Worktree},
};
use crate::{
    ErrorCode,
    actor::{ActorHandle, Origin},
    changes::{
        self, DIFF_INPUT, MAX_ROWS,
        git::{ChangeKind, ChangedFile, Git, GitError, Record},
    },
    error::RpcError,
    persist::now_ms,
    projection::ProjectionHub,
    runtime::Inner,
    watch,
    window_calls::check_owner,
};

/// How often the core looks whether a window still follows its Git view, and looks at the agents
/// of the worktrees, while one does.
const FOLLOW_CHECK: Duration = Duration::from_secs(1);

/// How long the core waits before it looks at a worktree again, so that a burst of changes is one
/// look.
const LOOK_DEBOUNCE: Duration = Duration::from_millis(500);

/// How often the texts of the times ("3 days ago") of the branches are made again.
const TIME_TEXTS: Duration = Duration::from_secs(30);

/// The most commits of `HEAD` that the core checks for a push.
const UNPUSHED_LIMIT: u32 = 2_000;

/// Whether this module runs the method.
pub(crate) fn handles(method: &str) -> bool {
    matches!(
        Method::from_name(method),
        Some(
            Method::GitFollow
                | Method::GitRepoAdd
                | Method::GitRepos
                | Method::GitStatus
                | Method::GitWorktrees
                | Method::GitBranches
                | Method::GitLog
                | Method::GitShow
                | Method::GitDiff
        )
    )
}

fn json_of<T: Serialize>(value: &T) -> Result<Value, RpcError> {
    to_value(value).map_err(|e| RpcError::internal(e.to_string()))
}

fn git_failed(error: &GitError) -> RpcError {
    RpcError::new(ErrorCode::GitFailed, error.to_string())
}

fn absolute(path: &str) -> Result<PathBuf, RpcError> {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(RpcError::invalid_params("the path must be absolute"))
    }
}

/// Runs a call that [`handles`] accepted.
pub(crate) async fn run(inner: &Arc<Inner>, call: Call, origin: Origin) -> Result<Value, RpcError> {
    let service = &inner.git_view;
    match call {
        Call::GitFollow(p) => {
            inner.render.require_window(p.window)?;
            check_owner(inner, &origin, p.window)?;
            service.follow_folder(p.window, absolute(&p.path)?);
            json_of(&Empty {})
        }
        Call::GitRepoAdd(p) => {
            service.repo_add(absolute(&p.path)?).await?;
            json_of(&Empty {})
        }
        Call::GitRepos(_) => json_of(&GitRepoList {
            repos: service.repo_rows().await,
        }),
        Call::GitStatus(p) => {
            let (folder, place) = service.at(inner, &origin, p.window, p.path).await?;
            json_of(&service.status(&folder, &place).await)
        }
        Call::GitWorktrees(p) => {
            let (_, place) = service.at(inner, &origin, p.window, p.path).await?;
            let look = service.look(&place).await.map_err(|e| git_failed(&e))?;
            let placed = service.placed();
            json_of(&GitWorktreeList {
                worktrees: worktree_rows(&place.root, &look, &placed),
            })
        }
        Call::GitBranches(p) => {
            let (_, place) = service.at(inner, &origin, p.window, p.path).await?;
            let look = service.look(&place).await.map_err(|e| git_failed(&e))?;
            json_of(&GitBranchList {
                branches: branch_rows(&place.root, &look, now_ms()),
            })
        }
        Call::GitLog(p) => {
            let (_, place) = service.at(inner, &origin, p.window, p.path).await?;
            json_of(&service.log(&place.root, p.skip, p.limit).await?)
        }
        Call::GitShow(p) => {
            let (_, place) = service.at(inner, &origin, p.window, p.path).await?;
            json_of(&service.show(&place.root, &p.commit).await?)
        }
        Call::GitDiff(p) => {
            let window = p.window;
            let (_, place) = service.at(inner, &origin, p.window, p.path).await?;
            let diff = match p.commit {
                Some(commit) => {
                    service
                        .commit_diff(inner, &place.root, &commit, p.file)
                        .await?
                }
                None => {
                    service
                        .file_diff(inner, window, &place.root, p.file)
                        .await?
                }
            };
            json_of(&diff)
        }
        other => Err(RpcError::internal(format!("not a git call: {other:?}"))),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the core read of a worktree and of its repository in one look.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Look {
    /// The short hash of `HEAD`, or `None` before the first commit.
    commit: Option<String>,
    branches: Vec<Branch>,
    /// The branches that `HEAD` went to last, the newest first.
    recent: Vec<String>,
    remotes: Vec<String>,
    worktrees: Vec<Worktree>,
    /// The files that are not committed in each other worktree.
    dirty: HashMap<PathBuf, u32>,
    /// Unix milliseconds.
    fetched_at: Option<i64>,
}

impl Look {
    /// The branch that the worktree has out.
    fn current(&self) -> Option<&Branch> {
        self.branches
            .iter()
            .find(|branch| branch.kind == Kind::Local && branch.head)
    }

    /// The roots of the worktrees that have files.
    fn roots(&self) -> Vec<PathBuf> {
        self.worktrees
            .iter()
            .filter(|worktree| !worktree.bare && !worktree.prunable)
            .map(|worktree| worktree.path.clone())
            .collect()
    }
}

/// The changes of the worktree in the last count.
#[derive(Clone, Debug, Default)]
struct Count {
    totals: Option<ChangeTotals>,
    /// The full name of `HEAD` in the count.
    head: Option<String>,
    rows: Vec<(String, ChangedFile)>,
    stats: Option<DiffStats>,
}

/// The Git view of one window that follows now.
struct Follow {
    folder: PathBuf,
    /// The core knows the place of `folder`.
    resolved: bool,
    /// `None`: the folder is not in a git worktree, or the core does not know yet.
    place: Option<Place>,
    watches: Vec<watch::DirWatch>,
    look: Option<Look>,
    /// Why the last look failed.
    error: Option<String>,
    count: Count,
    /// The numbers of the look and of the count that made the values. A look that started before
    /// and ends after changes nothing.
    look_seq: u64,
    count_seq: u64,
    worktree_keys: Vec<String>,
    branch_keys: Vec<String>,
    /// When the texts of the times were made, in unix milliseconds.
    timed_at: i64,
}

impl Follow {
    fn new(folder: PathBuf) -> Self {
        Self {
            folder,
            resolved: false,
            place: None,
            watches: Vec::new(),
            look: None,
            error: None,
            count: Count::default(),
            look_seq: 0,
            count_seq: 0,
            worktree_keys: Vec::new(),
            branch_keys: Vec::new(),
            timed_at: 0,
        }
    }
}

/// What a look of a window does: the whole look, or only the count of its changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Depth {
    Changes,
    Full,
}

/// Where a repository of `git_repos` was found.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Found {
    common_dir: PathBuf,
    root: PathBuf,
}

/// The state of `git_repos`.
#[derive(Default)]
struct Repos {
    /// The folders that the core looked at, with the repository that holds each one.
    found: HashMap<PathBuf, Option<Found>>,
    /// The paths that the person added, or `None` before the store answered.
    added: Option<Vec<String>>,
    /// A refresh of the rows runs.
    refreshing: bool,
    keys: Vec<String>,
}

#[derive(Default)]
struct State {
    /// The folder that each window named last.
    folders: HashMap<WindowId, PathBuf>,
    /// The windows that follow now.
    follows: HashMap<WindowId, Follow>,
    /// The windows whose look waits for the end of a burst, with the depth of the look.
    pending: HashMap<WindowId, Depth>,
    /// The check of the followers runs.
    checking: bool,
    repos: Repos,
    folders_seen: Folders,
}

/// The Git views of the windows, `git_repos`, and the git work behind them.
pub(crate) struct GitViewService {
    handle: Handle,
    git: Git,
    seq: AtomicU64,
    state: Mutex<State>,
    hub: OnceLock<Arc<ProjectionHub>>,
    actor: OnceLock<ActorHandle>,
}

/// Whether a window follows one of the projections of its Git view.
fn followed(hub: &ProjectionHub, window: WindowId) -> bool {
    [
        Projection::GitHead(window),
        Projection::GitWorktrees(window),
        Projection::GitBranches(window),
        Projection::GitChanges(window),
    ]
    .into_iter()
    .any(|projection| hub.subscriber_count(projection) > 0)
}

/// Sets the rows of a keyed projection, and removes the keys of `before` that are gone. A row that
/// did not change makes no patch.
fn set_keyed(
    hub: &ProjectionHub,
    projection: Projection,
    before: &mut Vec<String>,
    rows: Vec<(String, Value)>,
) {
    let keys: Vec<String> = rows.iter().map(|(key, _)| key.clone()).collect();
    let now: HashSet<&String> = keys.iter().collect();
    for key in before.iter().filter(|key| !now.contains(key)) {
        hub.remove(projection, key);
    }
    for (key, value) in rows {
        hub.set(projection, &key, value);
    }
    hub.set_order(projection, &keys);
    *before = keys;
}

fn keyed<T: Serialize>(rows: Vec<T>, key: impl Fn(&T) -> String) -> Vec<(String, Value)> {
    rows.into_iter()
        .filter_map(|row| Some((key(&row), serde_json::to_value(&row).ok()?)))
        .collect()
}

/// The head of a Git view.
fn head_value(
    folder: &Path,
    place: Option<&Place>,
    look: Option<&Look>,
    count: &Count,
    error: Option<&str>,
    agents: Vec<GitAgent>,
    now: i64,
) -> GitHead {
    let text = |path: &Path| path.to_string_lossy().into_owned();
    let current = look.and_then(Look::current);
    let remotes = look.map(|look| look.remotes.as_slice()).unwrap_or_default();
    let remote = read::fetch_remote(current.and_then(|b| b.remote.as_deref()), remotes);
    let fetched_at = look.and_then(|look| look.fetched_at);
    let fetched_text = remote.as_ref().map(|_| match fetched_at {
        Some(at) => format!("Last fetched {}", read::ago(now, at)),
        None => "Never fetched".to_owned(),
    });
    GitHead {
        folder: text(folder),
        worktree: place.map(|place| text(&place.root)),
        worktree_name: place.map(|place| read::last_part(&place.root)),
        repo: place.map(|place| text(&place.common_dir)),
        repo_name: place.map(|place| read::repo_name(&place.common_dir)),
        branch: current.map(|branch| branch.name.clone()),
        commit: look.and_then(|look| look.commit.clone()),
        upstream: current.and_then(|branch| branch.upstream.clone()),
        ahead: current.map_or(0, |branch| branch.ahead),
        behind: current.map_or(0, |branch| branch.behind),
        upstream_gone: current.is_some_and(|branch| branch.gone),
        remote,
        fetched_at: fetched_at.map(Timestamp),
        fetched_text,
        agents,
        changes: count.totals.clone(),
        diff_stats: count.stats,
        error: error.map(str::to_owned),
    }
}

/// The rows of `git_worktrees`: the main worktree first, then the others in the order of git. A
/// bare repository has no row for itself.
fn worktree_rows(root: &Path, look: &Look, placed: &[Placed]) -> Vec<GitWorktreeValue> {
    let roots = look.roots();
    look.worktrees
        .iter()
        .enumerate()
        .filter(|(_, worktree)| !worktree.bare)
        .map(|(index, worktree)| {
            let current = worktree.path == root;
            GitWorktreeValue {
                path: worktree.path.to_string_lossy().into_owned(),
                name: read::last_part(&worktree.path),
                branch: worktree
                    .branch
                    .as_deref()
                    .map(|reference| reference.strip_prefix("refs/heads/").unwrap_or(reference))
                    .map(str::to_owned),
                commit: worktree
                    .commit
                    .as_deref()
                    .map(|hash| hash.chars().take(7).collect()),
                main: index == 0,
                current,
                dirty: look.dirty.get(&worktree.path).copied(),
                locked: worktree.locked,
                prunable: worktree.prunable,
                agents: if worktree.prunable {
                    Vec::new()
                } else {
                    agents::agents_in(&worktree.path, &roots, placed)
                },
            }
        })
        .collect()
}

/// The rows of `git_branches`: the branch of `HEAD`, the recent ones, the other local ones, then
/// the remote ones. `for-each-ref` gives them the newest commit first.
fn branch_rows(root: &Path, look: &Look, now: i64) -> Vec<GitBranchValue> {
    let recent: Vec<&str> = look
        .recent
        .iter()
        .map(String::as_str)
        .filter(|name| {
            look.branches
                .iter()
                .any(|b| b.kind == Kind::Local && b.name == *name && !b.head)
        })
        .take(read::RECENT)
        .collect();
    let row = |branch: &Branch| GitBranchValue {
        name: branch.name.clone(),
        reference: branch.reference.clone(),
        kind: branch.kind.wire(),
        current: branch.kind == Kind::Local && branch.head,
        recent: branch.kind == Kind::Local && recent.contains(&branch.name.as_str()),
        upstream: branch.upstream.clone(),
        ahead: branch.ahead,
        behind: branch.behind,
        time: (branch.time > 0).then(|| Timestamp(branch.time.saturating_mul(1000))),
        time_text: (branch.time > 0).then(|| read::ago(now, branch.time.saturating_mul(1000))),
        subject: (!branch.subject.is_empty()).then(|| branch.subject.clone()),
        worktree: branch
            .worktree
            .as_ref()
            .filter(|path| path.as_path() != root)
            .map(|path| path.to_string_lossy().into_owned()),
    };
    let mut rows: Vec<GitBranchValue> = Vec::new();
    rows.extend(look.current().map(row));
    for name in &recent {
        rows.extend(
            look.branches
                .iter()
                .filter(|b| b.kind == Kind::Local && b.name == *name)
                .map(row),
        );
    }
    rows.extend(
        look.branches
            .iter()
            .filter(|b| b.kind == Kind::Local && !b.head && !recent.contains(&b.name.as_str()))
            .map(row),
    );
    rows.extend(
        look.branches
            .iter()
            .filter(|b| b.kind == Kind::Remote)
            .map(row),
    );
    rows
}

/// A commit of the History.
fn commit_value(commit: &read::Commit, unpushed: &HashSet<String>, now: i64) -> GitCommit {
    let at = commit.time.saturating_mul(1000);
    GitCommit {
        hash: commit.hash.clone(),
        short: commit.short.clone(),
        subject: commit.subject.clone(),
        author: commit.author.clone(),
        time: Timestamp(at),
        time_text: read::ago(now, at),
        branches: commit.branches.clone(),
        tags: commit.tags.clone(),
        head: commit.head,
        unpushed: unpushed.contains(&commit.hash),
    }
}

/// A row of a file of a commit. A commit has no worktree file, so `stamp` is 0.
fn commit_file_row(record: &Record) -> ChangeRow {
    let kind = match record.status {
        b'A' | b'C' => ChangeKind::Added,
        b'D' => ChangeKind::Deleted,
        b'R' => ChangeKind::Renamed,
        _ => ChangeKind::Modified,
    };
    let (add, del) = record.lines.unwrap_or((0, 0));
    let file = ChangedFile {
        path: record.path.clone(),
        old_path: record.old_path.clone(),
        kind,
        add,
        del,
        binary: record.lines.is_none(),
        stamp: 0,
    };
    changes::row_value(&changes::lossy(&record.path), &file)
}

fn file_status(record: &Record) -> rich::FileStatus {
    match record.status {
        b'A' | b'C' => rich::FileStatus::Added,
        b'D' => rich::FileStatus::Deleted,
        b'R' => rich::FileStatus::Renamed,
        _ => rich::FileStatus::Modified,
    }
}

/// The repositories of `git_repos`, from the folders where the core found them. A repository
/// takes the first source of the list: workspaces, then panes, then the ones that the person
/// added. The rows with agents come first, then by name.
fn repo_rows_of(
    candidates: &[(PathBuf, GitRepoSource)],
    found: &HashMap<PathBuf, Option<Found>>,
    placed: &[Placed],
) -> Vec<GitRepoValue> {
    let mut rows: Vec<GitRepoValue> = Vec::new();
    for (folder, source) in candidates {
        let Some(Some(repo)) = found.get(folder) else {
            continue;
        };
        let key = repo.common_dir.to_string_lossy().into_owned();
        if rows.iter().any(|row| row.common_dir == key) {
            continue;
        }
        // The main worktree, when the repository has one with files.
        let main = repo
            .common_dir
            .parent()
            .filter(|_| repo.common_dir.file_name() == Some(std::ffi::OsStr::new(".git")))
            .map(Path::to_path_buf)
            .unwrap_or_else(|| repo.root.clone());
        rows.push(GitRepoValue {
            name: read::repo_name(&repo.common_dir),
            common_dir: key,
            root: main.to_string_lossy().into_owned(),
            source: *source,
            agents: 0,
        });
    }
    for placed in placed
        .iter()
        .filter(|placed| placed.agent.state != GitAgentState::Exited)
    {
        let Some(Some(repo)) = found.get(&placed.folder) else {
            continue;
        };
        let key = repo.common_dir.to_string_lossy();
        if let Some(row) = rows.iter_mut().find(|row| row.common_dir == key) {
            row.agents = row.agents.saturating_add(1);
        }
    }
    rows.sort_by(|a, b| {
        b.agents
            .cmp(&a.agents)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.common_dir.cmp(&b.common_dir))
    });
    rows
}

impl GitViewService {
    /// `base_env` is the environment of the git commands. It must have a `PATH`.
    pub(crate) fn new(handle: Handle, base_env: &[(String, String)]) -> Arc<Self> {
        Arc::new(Self {
            handle,
            git: Git::new(base_env.to_vec()),
            seq: AtomicU64::new(0),
            state: Mutex::new(State::default()),
            hub: OnceLock::new(),
            actor: OnceLock::new(),
        })
    }

    /// The projections. A window that follows one of its Git projections starts its Git view,
    /// and a client that follows `git_repos` starts the list of the repositories.
    pub(crate) fn bind_projections(self: &Arc<Self>, hub: &Arc<ProjectionHub>) {
        let _ = self.hub.set(Arc::clone(hub));
        let service = Arc::downgrade(self);
        hub.on_subscribe(Box::new(move |projection| {
            let Some(service) = service.upgrade() else {
                return;
            };
            match projection {
                Projection::GitHead(window)
                | Projection::GitWorktrees(window)
                | Projection::GitBranches(window)
                | Projection::GitChanges(window) => service.window_followed(window),
                Projection::GitRepos => service.repos_followed(),
                _ => {}
            }
        }));
    }

    /// The state actor, for the store and the events.
    pub(crate) fn bind_actor(&self, actor: ActorHandle) {
        let _ = self.actor.set(actor);
    }

    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// The agents with the folders of their panes.
    fn placed(&self) -> Vec<Placed> {
        let Some(hub) = self.hub.get() else {
            return Vec::new();
        };
        let mut state = lock(&self.state);
        agents::placed(hub, &mut state.folders_seen)
    }

    // --- follows -----------------------------------------------------------------------------

    /// `git.follow`: the window shows the worktree that holds `folder`. The core starts when the
    /// window follows, now or later.
    pub(crate) fn follow_folder(self: &Arc<Self>, window: WindowId, folder: PathBuf) {
        let start = {
            let mut state = lock(&self.state);
            state.folders.insert(window, folder);
            state.follows.contains_key(&window)
                || self.hub.get().is_some_and(|hub| followed(hub, window))
        };
        if start {
            self.start_window(window);
        }
    }

    /// A window started to follow one of its Git projections. A window that named no folder yet
    /// waits for `git.follow`.
    fn window_followed(self: &Arc<Self>, window: WindowId) {
        if lock(&self.state).folders.contains_key(&window) {
            self.start_window(window);
        }
    }

    fn start_window(self: &Arc<Self>, window: WindowId) {
        let this = Arc::clone(self);
        self.handle
            .spawn(async move { this.resolve_window(window).await });
    }

    /// Starts the check of the followers, when it does not run.
    fn check(self: &Arc<Self>, state: &mut State) {
        if !state.checking {
            state.checking = true;
            self.handle.spawn(Arc::clone(self).check_followers());
        }
    }

    /// Once a second: ends the follows of the windows that no longer follow, and publishes the
    /// agents of the worktrees again. It stops when no window follows and no client follows
    /// `git_repos`.
    async fn check_followers(self: Arc<Self>) {
        let Some(hub) = self.hub.get().cloned() else {
            lock(&self.state).checking = false;
            return;
        };
        loop {
            tokio::time::sleep(FOLLOW_CHECK).await;
            let repos = hub.subscriber_count(Projection::GitRepos) > 0;
            let placed = self.placed();
            let now = now_ms();
            {
                let mut state = lock(&self.state);
                state.follows.retain(|window, follow| {
                    let kept = followed(&hub, *window);
                    if !kept {
                        forget(&hub, *window, follow);
                    }
                    kept
                });
                for (window, follow) in &mut state.follows {
                    let timed = now - follow.timed_at >= TIME_TEXTS.as_millis() as i64;
                    publish(&hub, *window, follow, &placed, now, timed);
                }
                if !repos && !state.repos.keys.is_empty() {
                    let mut keys = std::mem::take(&mut state.repos.keys);
                    set_keyed(&hub, Projection::GitRepos, &mut keys, Vec::new());
                }
                if state.follows.is_empty() && !repos {
                    state.checking = false;
                    return;
                }
            }
            if repos {
                self.refresh_repos();
            }
        }
    }

    /// Finds the worktree of the folder that the window named last. A new worktree clears the
    /// rows, starts its watches and looks.
    async fn resolve_window(self: Arc<Self>, window: WindowId) {
        let folder = {
            let mut state = lock(&self.state);
            let Some(folder) = state.folders.get(&window).cloned() else {
                return;
            };
            self.check(&mut state);
            let follow = state
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
        let place = read::place(&self.git, &folder).await.ok();
        let Some(hub) = self.hub.get() else {
            return;
        };
        let placed = self.placed();
        {
            let mut state = lock(&self.state);
            let Some(follow) = state.follows.get_mut(&window) else {
                return;
            };
            // The window named another folder in the meantime.
            if follow.folder != folder {
                return;
            }
            follow.resolved = true;
            if follow.place == place {
                publish(hub, window, follow, &placed, now_ms(), true);
                return;
            }
            let rows = changes::keys(&follow.count.rows);
            follow.place.clone_from(&place);
            follow.look = None;
            follow.error = None;
            follow.count = Count::default();
            follow.watches = Vec::new();
            changes::set_rows(hub, Projection::GitChanges(window), &rows, &[]);
            publish(hub, window, follow, &placed, now_ms(), true);
        }
        let Some(place) = place else {
            return;
        };
        let weak = Arc::downgrade(&self);
        let dirs = [place.git_dir.clone(), place.common_dir.clone()];
        let watches = self.watch_place(&place, move |change: watch::Change| {
            let Some(this) = weak.upgrade() else {
                return;
            };
            let depth = if change.paths.is_empty()
                || change
                    .paths
                    .iter()
                    .any(|path| dirs.iter().any(|dir| path.starts_with(dir)))
            {
                Depth::Full
            } else {
                Depth::Changes
            };
            this.schedule_window(window, depth);
        });
        {
            let mut state = lock(&self.state);
            match state.follows.get_mut(&window) {
                Some(follow) if follow.place.as_ref() == Some(&place) => {
                    follow.watches = watches;
                }
                _ => return,
            }
        }
        self.look_window(window, Depth::Full).await;
    }

    /// Looks at the worktree of a window now, and publishes its rows. A look of a worktree that
    /// the window left changes nothing.
    async fn look_window(&self, window: WindowId, depth: Depth) {
        let place = lock(&self.state)
            .follows
            .get(&window)
            .and_then(|follow| follow.place.clone());
        let Some(place) = place else {
            return;
        };
        let seq = self.next_seq();
        let (look, count) = match depth {
            Depth::Full => {
                let (look, count) = tokio::join!(self.look(&place), self.count(&place.root));
                (Some(look), count)
            }
            Depth::Changes => (None, self.count(&place.root).await),
        };
        let Some(hub) = self.hub.get() else {
            return;
        };
        let placed = self.placed();
        let mut state = lock(&self.state);
        let Some(follow) = state.follows.get_mut(&window) else {
            return;
        };
        if follow.place.as_ref() != Some(&place) {
            return;
        }
        if let Some(look) = look
            && seq > follow.look_seq
        {
            follow.look_seq = seq;
            match look {
                Ok(look) => {
                    follow.look = Some(look);
                    follow.error = None;
                }
                Err(error) => {
                    tracing::debug!(%window, %error, "the git view of a window is not read");
                    follow.error = Some(error.to_string());
                }
            }
        }
        if seq > follow.count_seq {
            follow.count_seq = seq;
            match count {
                Ok(count) => {
                    let before = changes::keys(&std::mem::replace(&mut follow.count, count).rows);
                    changes::set_rows(
                        hub,
                        Projection::GitChanges(window),
                        &before,
                        &follow.count.rows,
                    );
                }
                Err(error) => {
                    tracing::debug!(%window, %error, "the changes of a git view are not counted");
                    follow
                        .count
                        .totals
                        .get_or_insert(ChangeTotals {
                            files: 0,
                            truncated: false,
                            error: None,
                        })
                        .error = Some(error);
                }
            }
        }
        publish(hub, window, follow, &placed, now_ms(), true);
    }

    /// Looks at a window in a moment. A burst of causes is one look, as deep as the deepest
    /// cause.
    fn schedule_window(self: &Arc<Self>, window: WindowId, depth: Depth) {
        {
            let mut state = lock(&self.state);
            match state.pending.get_mut(&window) {
                Some(pending) => {
                    if depth == Depth::Full {
                        *pending = Depth::Full;
                    }
                    return;
                }
                None => {
                    state.pending.insert(window, depth);
                }
            }
        }
        let this = Arc::clone(self);
        self.handle.spawn(async move {
            tokio::time::sleep(LOOK_DEBOUNCE).await;
            let depth = lock(&this.state)
                .pending
                .remove(&window)
                .unwrap_or(Depth::Full);
            this.look_window(window, depth).await;
        });
    }

    /// The watches of a worktree and of the git directory of its repository, with `cause` for
    /// each change. The app watches both trees with all their folders. `nh daemon` has no watcher
    /// of the system, and a poll of a whole tree costs too much, so it polls only the entries of
    /// the git directories.
    fn watch_place(
        &self,
        place: &Place,
        cause: impl Fn(watch::Change) + Clone + Send + Sync + 'static,
    ) -> Vec<watch::DirWatch> {
        let mut out = Vec::new();
        let mut dirs: Vec<&Path> = vec![place.root.as_path()];
        if !place.common_dir.starts_with(&place.root) {
            dirs.push(&place.common_dir);
        }
        match watch::Kind::of_build() {
            watch::Kind::System => {
                for dir in dirs {
                    match watch::watch_tree(dir, cause.clone()) {
                        Ok(Some(watch)) => out.push(watch),
                        Ok(None) => {}
                        Err(error) => tracing::debug!(%error, "no watch of the git view"),
                    }
                }
            }
            watch::Kind::Poll => {
                let mut polled = vec![place.git_dir.as_path()];
                if place.common_dir != place.git_dir {
                    polled.push(&place.common_dir);
                }
                for dir in polled {
                    match watch::watch_dir(watch::Kind::Poll, dir, cause.clone()) {
                        Ok(watch) => out.push(watch),
                        Err(error) => tracing::debug!(%error, "no watch of the git view"),
                    }
                }
            }
        }
        out
    }

    // --- looks -------------------------------------------------------------------------------

    /// One look at a worktree and its repository: the commands run at the same time.
    async fn look(&self, place: &Place) -> Result<Look, GitError> {
        let root = &place.root;
        let (commit, branches, recent, remotes, worktrees) = tokio::join!(
            self.git.run(root, &["rev-parse", "--short", "HEAD"]),
            read::branches(&self.git, root),
            read::recent(&self.git, root),
            read::remotes(&self.git, root),
            read::worktrees(&self.git, root),
        );
        let worktrees = worktrees?;
        let others: Vec<&Worktree> = worktrees
            .iter()
            .filter(|w| !w.bare && !w.prunable)
            .collect();
        let counts =
            futures_util::future::join_all(others.iter().map(|w| read::dirty(&self.git, &w.path)))
                .await;
        let dirty = others
            .iter()
            .zip(counts)
            .filter_map(|(worktree, count)| Some((worktree.path.clone(), count.ok()?)))
            .collect();
        Ok(Look {
            commit: commit.ok().map(|out| out.trim().to_owned()),
            branches: branches?,
            recent: recent?,
            remotes: remotes?,
            worktrees,
            dirty,
            fetched_at: read::fetched_at(place),
        })
    }

    /// Counts the changes of a worktree, from `HEAD`.
    async fn count(&self, root: &Path) -> Result<Count, String> {
        let (found, _) = self
            .git
            .uncommitted(root)
            .await
            .map_err(|error| error.to_string())?;
        let stats = found.stats();
        let (totals, head, rows) = changes::counted(found);
        Ok(Count {
            totals: Some(totals),
            head: Some(head),
            rows,
            stats: Some(stats),
        })
    }

    // --- calls -------------------------------------------------------------------------------

    /// The worktree of a call: the one of the Git view of `window`, or the one that holds `path`.
    async fn at(
        &self,
        inner: &Inner,
        origin: &Origin,
        window: Option<WindowId>,
        path: Option<String>,
    ) -> Result<(PathBuf, Place), RpcError> {
        let folder = match (window, path) {
            (Some(_), Some(_)) => {
                return Err(RpcError::invalid_params(
                    "give a window or a path, not both",
                ));
            }
            (None, None) => return Err(RpcError::invalid_params("give a window or a path")),
            (None, Some(path)) => absolute(&path)?,
            (Some(window), None) => {
                inner.render.require_window(window)?;
                check_owner(inner, origin, window)?;
                let state = lock(&self.state);
                if let Some(Some(place)) = state.follows.get(&window).map(|f| f.place.clone()) {
                    let folder = state.follows[&window].folder.clone();
                    return Ok((folder, place));
                }
                state.folders.get(&window).cloned().ok_or_else(|| {
                    RpcError::new(ErrorCode::NotFound, "the window follows no git worktree")
                })?
            }
        };
        let place = read::place(&self.git, &folder).await.map_err(|_| {
            RpcError::new(ErrorCode::NotFound, "the folder is not in a git worktree")
        })?;
        Ok((folder, place))
    }

    /// `git.status`: the head and the changes, as the projections have them.
    async fn status(&self, folder: &Path, place: &Place) -> GitStatus {
        let (look, count) = tokio::join!(self.look(place), self.count(&place.root));
        let error = look.as_ref().err().map(ToString::to_string);
        let look = look.ok();
        let mut count = count.unwrap_or_else(|error| Count {
            totals: Some(ChangeTotals {
                files: 0,
                truncated: false,
                error: Some(error),
            }),
            ..Count::default()
        });
        let roots = look.as_ref().map(Look::roots).unwrap_or_default();
        let agents = agents::agents_in(&place.root, &roots, &self.placed());
        let rows = std::mem::take(&mut count.rows);
        GitStatus {
            head: head_value(
                folder,
                Some(place),
                look.as_ref(),
                &count,
                error.as_deref(),
                agents,
                now_ms(),
            ),
            changes: rows
                .iter()
                .map(|(key, file)| changes::row_value(key, file))
                .collect(),
        }
    }

    /// `git.log`: a page of the commits of `HEAD`.
    async fn log(&self, root: &Path, skip: u32, limit: u32) -> Result<GitLogResult, RpcError> {
        let limit = limit.clamp(1, GIT_LOG_LIMIT);
        let (commits, head, remotes) = tokio::join!(
            read::log(&self.git, root, skip, limit + 1),
            self.git.run(root, &["rev-parse", "HEAD"]),
            read::remotes(&self.git, root),
        );
        let mut commits = commits.map_err(|e| git_failed(&e))?;
        let more = commits.len() > limit as usize;
        commits.truncate(limit as usize);
        let unpushed = match remotes {
            Ok(remotes) if !remotes.is_empty() && !commits.is_empty() => {
                read::unpushed(&self.git, root, UNPUSHED_LIMIT)
                    .await
                    .unwrap_or_default()
            }
            _ => HashSet::new(),
        };
        let now = now_ms();
        Ok(GitLogResult {
            head: head.ok().map(|out| out.trim().to_owned()),
            commits: commits
                .iter()
                .map(|commit| commit_value(commit, &unpushed, now))
                .collect(),
            more,
        })
    }

    /// The full hash of a commit that a call names, or `not_found`.
    async fn commit_of(&self, root: &Path, commit: &str) -> Result<String, RpcError> {
        read::resolve_commit(&self.git, root, commit)
            .await
            .map_err(|e| git_failed(&e))?
            .ok_or_else(|| RpcError::new(ErrorCode::NotFound, format!("no commit {commit}")))
    }

    /// `git.show`: one commit with its message and its files.
    async fn show(&self, root: &Path, commit: &str) -> Result<GitShown, RpcError> {
        let hash = self.commit_of(root, commit).await?;
        let shown = read::show(&self.git, root, &hash)
            .await
            .map_err(|e| git_failed(&e))?;
        let parent = shown.parents.first().map(String::as_str);
        let (records, unpushed, remotes) = tokio::join!(
            read::commit_files(&self.git, root, &hash, parent),
            read::unpushed(&self.git, root, UNPUSHED_LIMIT),
            read::remotes(&self.git, root),
        );
        let records = records.map_err(|e| git_failed(&e))?;
        let unpushed = match remotes {
            Ok(remotes) if !remotes.is_empty() => unpushed.unwrap_or_default(),
            _ => HashSet::new(),
        };
        let stats = records.iter().fold(DiffStats { add: 0, del: 0 }, |sum, r| {
            let (add, del) = r.lines.unwrap_or((0, 0));
            DiffStats {
                add: sum.add.saturating_add(add),
                del: sum.del.saturating_add(del),
            }
        });
        let short = shown.short.len();
        Ok(GitShown {
            commit: commit_value(&shown, &unpushed, now_ms()),
            email: shown.email.clone(),
            body: shown.body.clone(),
            parents: shown
                .parents
                .iter()
                .map(|parent| parent.chars().take(short).collect())
                .collect(),
            truncated: records.len() > MAX_ROWS,
            files: records.iter().take(MAX_ROWS).map(commit_file_row).collect(),
            stats,
        })
    }

    /// `git.diff` with a commit: the patch of one file of the commit, from its first parent.
    async fn commit_diff(
        &self,
        inner: &Inner,
        root: &Path,
        commit: &str,
        file: String,
    ) -> Result<rich::FileDiff, RpcError> {
        let hash = self.commit_of(root, commit).await?;
        let shown = read::show(&self.git, root, &hash)
            .await
            .map_err(|e| git_failed(&e))?;
        let parent = shown.parents.first().map(String::as_str);
        let records = read::commit_files(&self.git, root, &hash, parent)
            .await
            .map_err(|e| git_failed(&e))?;
        let record = records
            .iter()
            .find(|record| changes::lossy(&record.path) == file)
            .ok_or_else(|| {
                RpcError::new(ErrorCode::NotFound, "the path is not a file of the commit")
            })?;
        let patch = read::commit_patch(&self.git, root, &hash, parent, record)
            .await
            .map_err(|e| git_failed(&e))?;
        let name = file.clone();
        let cut = patch.len() > DIFF_INPUT;
        let mut diff = inner
            .changes
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
        diff.path = file;
        diff.status = file_status(record);
        if record.lines.is_none() {
            diff.hunks.clear();
        }
        Ok(diff)
    }

    /// `git.diff` without a commit: the diff of one changed file of the worktree, from `HEAD`. A
    /// window that follows the worktree has the rows of its last count. A path that is not a row
    /// is `not_found`, so the call reads no file that is not a change of the worktree.
    async fn file_diff(
        &self,
        inner: &Inner,
        window: Option<WindowId>,
        root: &Path,
        file: String,
    ) -> Result<rich::FileDiff, RpcError> {
        let kept = window.and_then(|window| {
            let state = lock(&self.state);
            let follow = state.follows.get(&window)?;
            (follow.place.as_ref()?.root == root).then(|| {
                let row = follow.count.rows.iter().find(|(key, _)| *key == file);
                row.map(|(_, changed)| (changed.clone(), follow.count.head.clone()))
            })?
        });
        let (changed, head) = match kept {
            Some(found) => found,
            None => {
                let count = self
                    .count(root)
                    .await
                    .map_err(|error| RpcError::new(ErrorCode::GitFailed, error))?;
                count
                    .rows
                    .into_iter()
                    .find(|(key, _)| *key == file)
                    .map(|(_, changed)| (changed, count.head))
                    .ok_or_else(|| {
                        RpcError::new(
                            ErrorCode::NotFound,
                            "the path is not a change of the worktree",
                        )
                    })?
            }
        };
        inner.changes.diff_of(root, file, &changed, head).await
    }

    // --- repositories ------------------------------------------------------------------------

    /// A client started to follow `git_repos`.
    fn repos_followed(self: &Arc<Self>) {
        let mut state = lock(&self.state);
        self.check(&mut state);
        drop(state);
        self.refresh_repos();
    }

    /// Makes the rows of `git_repos` again, when no refresh runs.
    fn refresh_repos(self: &Arc<Self>) {
        {
            let mut state = lock(&self.state);
            if state.repos.refreshing {
                return;
            }
            state.repos.refreshing = true;
        }
        let this = Arc::clone(self);
        self.handle.spawn(async move {
            let rows = this.repo_rows().await;
            let mut state = lock(&this.state);
            state.repos.refreshing = false;
            if let Some(hub) = this.hub.get()
                && hub.subscriber_count(Projection::GitRepos) > 0
            {
                let mut keys = std::mem::take(&mut state.repos.keys);
                let rows = keyed(rows, |row| row.common_dir.clone());
                set_keyed(hub, Projection::GitRepos, &mut keys, rows);
                state.repos.keys = keys;
            }
        });
    }

    /// The folders where repositories can be: the roots of the workspaces, the folders of the
    /// panes, and the paths that the person added, each as the system names it.
    async fn candidates(&self) -> Vec<(PathBuf, GitRepoSource)> {
        let added = lock(&self.state).repos.added.clone();
        let added = match added {
            Some(added) => added,
            None => {
                let read = match self.actor.get() {
                    Some(actor) => actor.store(|store| store.git_repos()).await.ok(),
                    None => None,
                };
                let paths: Vec<String> = read
                    .unwrap_or_default()
                    .into_iter()
                    .map(|row| row.path)
                    .collect();
                lock(&self.state).repos.added = Some(paths.clone());
                paths
            }
        };
        let Some(hub) = self.hub.get() else {
            return Vec::new();
        };
        let roots: Vec<String> = hub
            .snapshot(Projection::Tree)
            .items
            .values()
            .filter_map(|value| serde_json::from_value::<WorkspaceValue>(value.clone()).ok())
            .filter_map(|workspace| workspace.root)
            .collect();
        let panes = agents::pane_folders(hub);
        let mut state = lock(&self.state);
        let mut out: Vec<(PathBuf, GitRepoSource)> = Vec::new();
        let sources = [
            (roots, GitRepoSource::Workspace),
            (panes.into_values().collect(), GitRepoSource::Pane),
            (added, GitRepoSource::Added),
        ];
        for (folders, source) in sources {
            for folder in folders {
                if let Some(path) = state.folders_seen.canonical(&folder)
                    && !out.iter().any(|(seen, _)| *seen == path)
                {
                    out.push((path, source));
                }
            }
        }
        out
    }

    /// The rows of `git_repos`. A folder that the core did not look at yet gets its look.
    async fn repo_rows(&self) -> Vec<GitRepoValue> {
        let candidates = self.candidates().await;
        let missing: Vec<PathBuf> = {
            let state = lock(&self.state);
            candidates
                .iter()
                .map(|(folder, _)| folder)
                .filter(|folder| !state.repos.found.contains_key(*folder))
                .cloned()
                .collect()
        };
        let looks =
            futures_util::future::join_all(missing.iter().map(|f| read::place(&self.git, f))).await;
        let placed = self.placed();
        let mut state = lock(&self.state);
        for (folder, look) in missing.into_iter().zip(looks) {
            let found = look.ok().map(|place| Found {
                common_dir: place.common_dir,
                root: place.root,
            });
            state.repos.found.insert(folder, found);
        }
        // A folder that is no longer a candidate is looked at again when it comes back.
        let wanted: HashSet<&PathBuf> = candidates.iter().map(|(folder, _)| folder).collect();
        state
            .repos
            .found
            .retain(|folder, _| wanted.contains(folder));
        repo_rows_of(&candidates, &state.repos.found, &placed)
    }

    /// `git.repo_add`: `git_repos` keeps the repository that holds `path`.
    async fn repo_add(self: &Arc<Self>, path: PathBuf) -> Result<(), RpcError> {
        let place = read::place(&self.git, &path)
            .await
            .map_err(|_| RpcError::new(ErrorCode::NotFound, "the path is not in a git worktree"))?;
        let actor = self
            .actor
            .get()
            .ok_or_else(|| RpcError::internal("the git view has no state actor"))?;
        let text = path.to_string_lossy().into_owned();
        let event = EventPayload::GitRepoAdded(GitRepoAdded {
            path: text.clone(),
            common_dir: place.common_dir.to_string_lossy().into_owned(),
        });
        let kept = text.clone();
        actor
            .update(move |mut parts| {
                parts.commit(|tx| {
                    if tx.add_git_repo(&kept, now_ms())? {
                        tx.append(Some(&kept), &event)?;
                    }
                    Ok(())
                })
            })
            .await
            .map_err(|error| RpcError::internal(error.to_string()))?
            .map_err(|error| RpcError::new(ErrorCode::StoreError, error.to_string()))?;
        {
            let mut state = lock(&self.state);
            let added = state.repos.added.get_or_insert_with(Vec::new);
            if !added.contains(&text) {
                added.push(text);
            }
        }
        if self
            .hub
            .get()
            .is_some_and(|hub| hub.subscriber_count(Projection::GitRepos) > 0)
        {
            self.refresh_repos();
        }
        Ok(())
    }
}

/// Removes the rows of a window that no longer follows, so that a window that follows again starts
/// from nothing.
fn forget(hub: &ProjectionHub, window: WindowId, follow: &mut Follow) {
    changes::set_rows(
        hub,
        Projection::GitChanges(window),
        &changes::keys(&follow.count.rows),
        &[],
    );
    set_keyed(
        hub,
        Projection::GitWorktrees(window),
        &mut follow.worktree_keys,
        Vec::new(),
    );
    set_keyed(
        hub,
        Projection::GitBranches(window),
        &mut follow.branch_keys,
        Vec::new(),
    );
    hub.remove(Projection::GitHead(window), SINGLETON_KEY);
}

/// Publishes the head and the worktrees of a window, and its branches when `timed`: the texts of
/// their times go stale slowly, and a look makes them again.
fn publish(
    hub: &ProjectionHub,
    window: WindowId,
    follow: &mut Follow,
    placed: &[Placed],
    now: i64,
    timed: bool,
) {
    let roots = follow.look.as_ref().map(Look::roots).unwrap_or_default();
    let agents = follow
        .place
        .as_ref()
        .map(|place| agents::agents_in(&place.root, &roots, placed))
        .unwrap_or_default();
    let head = head_value(
        &follow.folder,
        follow.place.as_ref(),
        follow.look.as_ref(),
        &follow.count,
        follow.error.as_deref(),
        agents,
        now,
    );
    if let Ok(json) = serde_json::to_value(head) {
        hub.set_singleton(Projection::GitHead(window), json);
    }
    let (Some(place), Some(look)) = (&follow.place, &follow.look) else {
        set_keyed(
            hub,
            Projection::GitWorktrees(window),
            &mut follow.worktree_keys,
            Vec::new(),
        );
        set_keyed(
            hub,
            Projection::GitBranches(window),
            &mut follow.branch_keys,
            Vec::new(),
        );
        return;
    };
    let rows = keyed(worktree_rows(&place.root, look, placed), |row| {
        row.path.clone()
    });
    set_keyed(
        hub,
        Projection::GitWorktrees(window),
        &mut follow.worktree_keys,
        rows,
    );
    if timed {
        follow.timed_at = now;
        let rows = keyed(branch_rows(&place.root, look, now), |row| {
            row.reference.clone()
        });
        set_keyed(
            hub,
            Projection::GitBranches(window),
            &mut follow.branch_keys,
            rows,
        );
    }
}
