//! The trust of git repositories, as the workspace trust of an editor.
//!
//! The settings of a repository can name programs that git runs: `core.fsmonitor`, a clean filter,
//! the `uploadpack` of a remote, and more. A repository from an archive, a shared folder or a plain
//! clone can bring such settings. Thus the core runs no git command in a repository until the
//! person trusts it (`git.trust`). Until then, the Changes tab and the Git view show a prompt.
//!
//! - **The walk.** [`discover`] finds the repository of a folder without git. It walks up the
//!   folders to `.git`, as git does. A bare repository that git would find in a folder stops the
//!   walk, because git refuses it (`safe.bareRepository=explicit`).
//! - **The key.** A trust is for the canonical path of the common git directory, so the worktrees
//!   of a repository share it. The store keeps it in `git_trust`.
//! - **The settings.** At the time of the trust, the core keeps the settings of the repository
//!   (local and worktree, with their includes) that name a program ([`names_a_program`]). It reads
//!   them with `git config --list`, which runs no program. Before git runs in a trusted repository,
//!   the core reads them again ([`Trusts::gate`]). When they changed, for example because an agent
//!   wrote `.git/config`, the core asks again.
//! - **Git after the trust.** `git rev-parse` gives the worktree. When git finds another common
//!   git directory than the walk, the core does not use the folder.

use std::{
    collections::{BTreeSet, HashMap},
    ffi::OsStr,
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError},
};

use nh_proto::api::{
    calls::git::GitTrustResult,
    events::{EventPayload, GitTrustEvent},
    model::{GitTrustAsk, GitTrustReason},
};
use serde_json::json;

use super::git::{Git, GitError, Place};
use crate::{ErrorCode, actor::ActorHandle, error::RpcError, persist::now_ms};

/// A repository that holds a folder, found without git.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Found {
    /// The folder that holds `.git`: the root of the worktree.
    pub root: PathBuf,
    /// The git directory of the worktree.
    pub git_dir: PathBuf,
    /// The common git directory, as the system names it: the key of the trust.
    pub common_dir: PathBuf,
}

/// The largest `HEAD`, `commondir` or `.git` file that the walk reads.
const SMALL_FILE: u64 = 4096;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The text of a small regular file. A FIFO or a device is never opened.
fn small_text(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > SMALL_FILE {
        return None;
    }
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(SMALL_FILE)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

/// A path that a file of git holds: absolute, or from `base`.
fn path_in(base: &Path, text: &str) -> PathBuf {
    let path = Path::new(text.trim_end_matches(['\n', '\r']));
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

/// Whether `HEAD` is as git wants it: a symbolic ref or an object name.
fn valid_head(head: &Path) -> bool {
    if std::fs::symlink_metadata(head).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return true;
    }
    small_text(head).is_some_and(|text| {
        let text = text.trim_end();
        text.starts_with("ref:")
            || ((40..=64).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_hexdigit()))
    })
}

/// The common git directory of `git_dir`, when it is a git directory: it has a valid `HEAD`, and
/// the common directory (`commondir`, else itself) has `objects` and `refs`.
fn common_of(git_dir: &Path) -> Option<PathBuf> {
    if !valid_head(&git_dir.join("HEAD")) {
        return None;
    }
    let common = match small_text(&git_dir.join("commondir")) {
        Some(text) => path_in(git_dir, &text),
        None => git_dir.to_path_buf(),
    };
    (common.join("objects").is_dir() && common.join("refs").is_dir()).then(|| canonical(&common))
}

/// The repository that holds `folder`, as git finds it from there with
/// `safe.bareRepository=explicit`. It runs no program, and it reads only small files.
///
/// From the folder up to the root, or to another file system: a folder with a `.git` that is a
/// git directory, or a `.git` file that names one, is the worktree. A `.git` file that names no
/// git directory, and a folder that is a bare repository, stop the walk, as git stops.
pub(crate) fn discover(folder: &Path) -> Option<Found> {
    let start = std::fs::canonicalize(folder).ok()?;
    let device = std::fs::metadata(&start).ok()?.dev();
    for dir in start.ancestors() {
        // Git does not go up into another file system by itself.
        if std::fs::metadata(dir).ok()?.dev() != device {
            return None;
        }
        let dot = dir.join(".git");
        match std::fs::metadata(&dot) {
            Ok(meta) if meta.is_dir() => {
                if let Some(common_dir) = common_of(&dot) {
                    return Some(Found {
                        root: dir.to_path_buf(),
                        git_dir: canonical(&dot),
                        common_dir,
                    });
                }
            }
            Ok(meta) if meta.is_file() => {
                let git_dir = small_text(&dot)?
                    .strip_prefix("gitdir:")
                    .map(|rest| path_in(dir, rest.trim()))?;
                let common_dir = common_of(&git_dir)?;
                return Some(Found {
                    root: dir.to_path_buf(),
                    git_dir: canonical(&git_dir),
                    common_dir,
                });
            }
            _ => {}
        }
        // A bare repository: git refuses it, and does not look further up.
        if common_of(dir).is_some() {
            return None;
        }
    }
    None
}

// --- the settings ------------------------------------------------------------------------------

/// One setting of a repository: its key as git gives it, and its value, or `None` for a key with
/// no value.
pub(crate) type Setting = (String, Option<String>);

/// Whether a setting names a program that git, or a program that git starts, runs. It is the list
/// of git-config(1), and of `git lfs`, of the settings whose value is a command or a program, and
/// of the settings that read more settings. The key has the form `section[.subsection].name`, and
/// the section and the name have any case.
pub(crate) fn names_a_program(key: &str) -> bool {
    let Some((section, rest)) = key.split_once('.') else {
        return false;
    };
    let (sub, name) = match rest.rsplit_once('.') {
        Some((sub, name)) => (Some(sub), name),
        None => (None, rest),
    };
    let section = section.to_ascii_lowercase();
    let name = name.to_ascii_lowercase();
    match (section.as_str(), sub, name.as_str()) {
        // Each setting of these sections can run a program, or read more settings.
        ("alias" | "filter" | "gpg" | "includeif" | "instaweb" | "pager" | "sendemail", _, _) => {
            true
        }
        (
            "core",
            None,
            "alternaterefscommand"
            | "askpass"
            | "editor"
            | "fsmonitor"
            | "gitproxy"
            | "hookspath"
            | "pager"
            | "sshcommand",
        )
        | ("include", None, "path")
        | ("diff", None, "external")
        | ("diff", Some(_), "command" | "textconv")
        | ("difftool" | "mergetool" | "browser" | "man", Some(_), "cmd" | "path")
        | ("merge", Some(_), "driver")
        | ("remote", Some(_), "uploadpack" | "receivepack" | "vcs")
        | ("credential", _, "helper")
        | ("sequence", None, "editor")
        | ("uploadpack", None, "packobjectshook")
        | ("interactive", None, "difffilter")
        | ("submodule", Some(_), "update")
        | ("trailer", Some(_), "command" | "cmd") => true,
        ("lfs", Some(sub), name) => {
            let sub = sub.to_ascii_lowercase();
            (sub.starts_with("customtransfer.") && name == "path")
                || (sub.starts_with("extension.") && matches!(name, "clean" | "smudge"))
        }
        _ => false,
    }
}

/// The settings of the repository that name a program, from the output of
/// `git config --list --includes --show-scope -z`: a scope, then the key, a newline and the
/// value, each with a NUL at its end. Only the scopes of the repository count: `local` and
/// `worktree`, with the files that they include. The settings of the person (`global`, `system`)
/// and of the command line do not.
pub(crate) fn parse_settings(out: &[u8]) -> Vec<Setting> {
    let mut fields = out.split(|b| *b == 0);
    let mut settings = Vec::new();
    while let (Some(scope), Some(entry)) = (fields.next(), fields.next()) {
        if !matches!(scope, b"local" | b"worktree") {
            continue;
        }
        let entry = String::from_utf8_lossy(entry);
        let (key, value) = match entry.split_once('\n') {
            Some((key, value)) => (key.to_owned(), Some(value.to_owned())),
            None => (entry.into_owned(), None),
        };
        if names_a_program(&key) {
            settings.push((key, value));
        }
    }
    settings
}

/// The keys whose values are not the same in `before` and `now`, in their order. A key can have
/// many values, and their order counts.
pub(crate) fn changed_keys(before: &[Setting], now: &[Setting]) -> Vec<String> {
    let values = |list: &[Setting], key: &str| -> Vec<Option<String>> {
        list.iter()
            .filter(|(k, _)| k == key)
            .map(|(_, value)| value.clone())
            .collect()
    };
    before
        .iter()
        .chain(now)
        .map(|(key, _)| key.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|key| values(before, key) != values(now, key))
        .map(str::to_owned)
        .collect()
}

// --- the trusts --------------------------------------------------------------------------------

/// Why git may not run in a repository now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub reason: GitTrustReason,
    /// The keys that changed after the trust.
    pub changed: Vec<String>,
}

/// What git may do in the repository of a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Access {
    /// The folder is in no git worktree that git can use.
    None,
    /// The person must trust the repository first.
    Ask(GitTrustAsk),
    /// Git may run. The place comes from git.
    Open(Place),
}

/// The prompt of a repository that git may not run in.
pub(crate) fn ask_of(found: &Found, refusal: Refusal) -> GitTrustAsk {
    ask_for(&found.root, &found.common_dir, refusal)
}

/// The prompt of the repository of `common_dir`, from its worktree at `root`.
pub(crate) fn ask_for(root: &Path, common_dir: &Path, refusal: Refusal) -> GitTrustAsk {
    GitTrustAsk {
        path: text(root),
        common_dir: text(common_dir),
        reason: refusal.reason,
        changed: refusal.changed,
    }
}

/// The error of a call on a repository that git may not run in.
pub(crate) fn untrusted(ask: &GitTrustAsk) -> RpcError {
    let message = match ask.reason {
        GitTrustReason::New => format!(
            "neohar runs no git command in {} until you trust it (nh git trust).",
            ask.path
        ),
        GitTrustReason::Changed => format!(
            "The settings of {} changed after you trusted it. neohar runs no git command in it \
             until you trust it again (nh git trust).",
            ask.path
        ),
    };
    let mut error = RpcError::new(ErrorCode::PermissionDenied, message);
    error.details = json!({ "kind": "untrusted", "trust": ask });
    error
}

fn not_a_repository() -> RpcError {
    RpcError::new(ErrorCode::NotFound, "the path is not in a git repository")
}

/// A function that runs when the trust of a repository changes, with its common git directory.
pub(crate) type Listener = Box<dyn Fn(&Path) + Send + Sync>;

/// The repositories that the person trusts, with the settings of each at the time of the trust.
pub(crate) struct Trusts {
    /// Reads the settings: `git config` runs no program.
    git: Git,
    actor: OnceLock<ActorHandle>,
    /// By common git directory. `None` before the store answered.
    kept: Mutex<Option<HashMap<PathBuf, Vec<Setting>>>>,
    listeners: Mutex<Vec<Listener>>,
}

impl Trusts {
    /// `base_env` is the environment of `git config`. It must have a `PATH`.
    pub(crate) fn new(base_env: &[(String, String)]) -> Arc<Self> {
        Arc::new(Self {
            git: Git::new(base_env.to_vec()),
            actor: OnceLock::new(),
            kept: Mutex::new(None),
            listeners: Mutex::new(Vec::new()),
        })
    }

    /// The state actor, for the store and the events. Without it (in a test), the trusts live in
    /// memory only.
    pub(crate) fn bind_actor(&self, actor: ActorHandle) {
        let _ = self.actor.set(actor);
    }

    /// Runs `listener` after each change of a trust.
    pub(crate) fn on_change(&self, listener: Listener) {
        lock(&self.listeners).push(listener);
    }

    fn changed(&self, common_dir: &Path) {
        for listener in lock(&self.listeners).iter() {
            listener(common_dir);
        }
    }

    /// Reads the trusts from the store once. Before the state actor is there, or when the store
    /// fails, no repository is trusted, and the next call reads again.
    async fn load(&self) {
        if lock(&self.kept).is_some() {
            return;
        }
        let Some(actor) = self.actor.get() else {
            return;
        };
        let rows = match actor.store(|store| store.git_trust()).await {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(%error, "the trusts of the git repositories are not read");
                return;
            }
        };
        let table = rows
            .into_iter()
            .filter_map(|row| {
                // A row that is not readable is no trust.
                let settings = serde_json::from_str::<Vec<Setting>>(&row.settings).ok()?;
                Some((PathBuf::from(row.common_dir), settings))
            })
            .collect();
        lock(&self.kept).get_or_insert(table);
    }

    /// The settings of a trusted repository at the time of the trust.
    async fn kept(&self, common_dir: &Path) -> Option<Vec<Setting>> {
        self.load().await;
        lock(&self.kept).as_ref()?.get(common_dir).cloned()
    }

    /// The settings of the repository of `cwd` that name a program. `git config` reads them and
    /// runs nothing.
    async fn settings(&self, cwd: &Path) -> Result<Vec<Setting>, GitError> {
        let out = self
            .git
            .output(
                cwd,
                &["config", "--list", "--includes", "--show-scope", "-z"].map(OsStr::new),
            )
            .await?;
        Ok(parse_settings(&out))
    }

    /// Whether git may run in the repository of `common_dir` now, from `cwd` in one of its
    /// worktrees. A repository that the person did not trust runs no git at all. A trusted one
    /// runs `git config` to read its settings again.
    pub(crate) async fn gate(
        &self,
        common_dir: &Path,
        cwd: &Path,
    ) -> Result<Option<Refusal>, GitError> {
        let Some(before) = self.kept(common_dir).await else {
            return Ok(Some(Refusal {
                reason: GitTrustReason::New,
                changed: Vec::new(),
            }));
        };
        let changed = changed_keys(&before, &self.settings(cwd).await?);
        Ok((!changed.is_empty()).then_some(Refusal {
            reason: GitTrustReason::Changed,
            changed,
        }))
    }

    /// What git may do in the repository of `folder`. A folder in no repository, a repository whose
    /// settings git cannot read, and a folder where git finds another repository than the walk,
    /// are [`Access::None`].
    pub(crate) async fn access(&self, folder: &Path) -> Access {
        let Some(found) = discover(folder) else {
            return Access::None;
        };
        match self.gate(&found.common_dir, &found.root).await {
            Ok(Some(refusal)) => Access::Ask(ask_of(&found, refusal)),
            Ok(None) => match self.git.place(folder).await {
                Ok(place) if place.common_dir == found.common_dir => Access::Open(place),
                Ok(place) => {
                    tracing::debug!(
                        folder = %folder.display(),
                        git = %place.common_dir.display(),
                        walk = %found.common_dir.display(),
                        "git finds another repository than the walk"
                    );
                    Access::None
                }
                Err(error) => {
                    tracing::debug!(folder = %folder.display(), %error, "no git worktree");
                    Access::None
                }
            },
            Err(error) => {
                tracing::debug!(folder = %folder.display(), %error, "the settings are not read");
                Access::None
            }
        }
    }

    /// `git.trust`: trusts the repository that holds `path`, with its settings of now.
    pub(crate) async fn trust(&self, path: &Path) -> Result<GitTrustResult, RpcError> {
        let found = discover(path).ok_or_else(not_a_repository)?;
        let settings = self
            .settings(&found.root)
            .await
            .map_err(|error| RpcError::new(ErrorCode::GitFailed, error.to_string()))?;
        let stored =
            serde_json::to_string(&settings).map_err(|e| RpcError::internal(e.to_string()))?;
        self.load().await;
        let common = text(&found.common_dir);
        let event = EventPayload::GitTrusted(GitTrustEvent {
            path: text(path),
            common_dir: common.clone(),
        });
        self.write(move |tx| {
            tx.trust_git_repo(&common, &stored, now_ms())?;
            tx.append(Some(&common), &event)?;
            Ok(())
        })
        .await?;
        lock(&self.kept)
            .get_or_insert_with(HashMap::new)
            .insert(found.common_dir.clone(), settings);
        self.changed(&found.common_dir);
        Ok(GitTrustResult {
            root: text(&found.root),
            common_dir: text(&found.common_dir),
        })
    }

    /// `git.untrust`: takes back the trust of the repository that holds `path`. `path` can also be
    /// the common git directory of a trust whose worktrees are gone.
    pub(crate) async fn untrust(&self, path: &Path) -> Result<GitTrustResult, RpcError> {
        self.load().await;
        let (root, common_dir) = match discover(path) {
            Some(found) => (found.root, found.common_dir),
            None => {
                let key = canonical(path);
                let known = lock(&self.kept)
                    .as_ref()
                    .is_some_and(|kept| kept.contains_key(&key));
                if !known {
                    return Err(not_a_repository());
                }
                (key.clone(), key)
            }
        };
        let common = text(&common_dir);
        let event = EventPayload::GitUntrusted(GitTrustEvent {
            path: text(path),
            common_dir: common.clone(),
        });
        self.write(move |tx| {
            if tx.untrust_git_repo(&common)? {
                tx.append(Some(&common), &event)?;
            }
            Ok(())
        })
        .await?;
        lock(&self.kept)
            .get_or_insert_with(HashMap::new)
            .remove(&common_dir);
        self.changed(&common_dir);
        Ok(GitTrustResult {
            root: text(&root),
            common_dir: text(&common_dir),
        })
    }

    /// Runs `change` in one transaction of the store. Without a state actor, nothing.
    async fn write(
        &self,
        change: impl FnOnce(&mut nh_store::Tx<'_>) -> Result<(), nh_store::StoreError> + Send + 'static,
    ) -> Result<(), RpcError> {
        let Some(actor) = self.actor.get() else {
            return Ok(());
        };
        actor
            .update(move |mut parts| parts.commit(change))
            .await
            .map_err(|error| RpcError::internal(error.to_string()))?
            .map_err(|error| RpcError::new(ErrorCode::StoreError, error.to_string()))
    }
}

#[cfg(test)]
mod tests;
