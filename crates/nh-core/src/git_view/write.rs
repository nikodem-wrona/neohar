//! The writes of the Git view: `git.switch`, `git.unstash`, `git.fetch` and `git.pull`.
//!
//! Each write is one git command that a click starts. The core never retries a write, and each
//! write that works appends one event with its command. One write at a time runs in a worktree
//! (a fetch: in a repository), and a second one is `busy`.
//!
//! **The agent guard.** A write that changes the files of a worktree (a switch, a pull, an
//! unstash) asks first when an agent starts, works, waits for the person or has a state that the
//! core does not know in that worktree: without `force` it answers `confirm_required` with the
//! agents. An agent that is idle or that exited does not stop it.
//!
//! **Credentials.** A fetch from a click sets `GIT_ASKPASS` and `SSH_ASKPASS` to `nh askpass`, so a
//! passphrase or a token shows the sheet of the app, as the ssh links do. A fetch in the
//! background never asks: it stops, and the head says why (`fetch_note`).
//!
//! **Hooks.** A write from a click runs the hooks of the repository, as the person expects of git.
//! A fetch in the background runs no hook and fetches no submodule. A name that comes from git (a
//! branch, a remote, a stash) follows `--end-of-options`, so a name that starts with `-` is never
//! an option.

use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use nh_proto::api::{
    calls::git::GitChanges,
    envelope::CallError,
    events::{EventPayload, GitWrite},
    ids::{JobId, RunId},
    model::{GitAgent, GitAgentState, GitJob, GitJobKind},
};
use serde_json::json;

use super::{
    GitViewService, Look, Work, agents, git_failed, lock,
    read::{self, Branch, Kind, Place},
};
use crate::{
    ErrorCode,
    changes::git::{Git, GitError},
    error::RpcError,
    persist::now_ms,
};

/// The longest time that a fetch or a pull runs. A fetch that waits for a network that does not
/// answer stops then.
const JOB_LIMIT: Duration = Duration::from_secs(10 * 60);

/// How a fetch from a click reaches the person: the `nh` that answers the questions of ssh and
/// git, and the socket of this core.
#[derive(Clone, Debug)]
pub(crate) struct Askpass {
    pub nh: PathBuf,
    pub socket: PathBuf,
    pub session: String,
}

/// The name of the program that git and ssh run for a question, next to the socket.
const ASKPASS_SCRIPT: &str = "git-askpass";

impl Askpass {
    /// Writes the program that git and ssh run: `exec '<nh>' askpass "$@"`. A variable of git holds
    /// a path, not a command with arguments. The file has mode 0700, in the private folder of
    /// the sockets.
    fn script(&self) -> std::io::Result<PathBuf> {
        let dir = self
            .socket
            .parent()
            .ok_or_else(|| std::io::Error::other("the socket has no folder"))?;
        let path = dir.join(ASKPASS_SCRIPT);
        let quoted = self.nh.to_string_lossy().replace('\'', r"'\''");
        let text = format!("#!/bin/sh\nexec '{quoted}' askpass \"$@\"\n");
        if std::fs::read_to_string(&path).is_ok_and(|old| old == text) {
            return Ok(path);
        }
        let temporary = dir.join(format!(".{ASKPASS_SCRIPT}.{}", std::process::id()));
        std::fs::write(&temporary, text)?;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o700))?;
        std::fs::rename(&temporary, &path)?;
        Ok(path)
    }

    /// The variables of a fetch from a click.
    fn env(&self) -> std::io::Result<Vec<(String, String)>> {
        let script = self.script()?.to_string_lossy().into_owned();
        Ok(vec![
            ("GIT_ASKPASS".to_owned(), script.clone()),
            ("SSH_ASKPASS".to_owned(), script),
            ("SSH_ASKPASS_REQUIRE".to_owned(), "force".to_owned()),
            (
                "NEOHAR_SOCKET".to_owned(),
                self.socket.to_string_lossy().into_owned(),
            ),
            ("NEOHAR_SESSION".to_owned(), self.session.clone()),
        ])
    }
}

/// The variables of a fetch in the background: no program answers a question, so a fetch that
/// needs a passphrase or a token fails at once. A credential helper still works.
fn quiet_env() -> Vec<(String, String)> {
    [
        ("GIT_ASKPASS", ""),
        ("SSH_ASKPASS", ""),
        ("SSH_ASKPASS_REQUIRE", "never"),
    ]
    .map(|(key, value)| (key.to_owned(), value.to_owned()))
    .to_vec()
}

/// The arguments of a fetch of `remote`. The name comes from the configuration, so it follows
/// `--end-of-options`: a remote named `--upload-pack=<command>` is a name, not an option. A fetch
/// in the background fetches no submodule, which has a configuration of its own.
pub(super) fn fetch_args(remote: &str, background: bool) -> Vec<&str> {
    let mut args = vec!["fetch", "--prune"];
    if background {
        args.push("--no-recurse-submodules");
    }
    args.extend(["--end-of-options", remote]);
    args
}

/// A write that runs in a worktree or a repository. The key goes free when it drops.
pub(super) struct Writing {
    service: Arc<GitViewService>,
    key: PathBuf,
}

impl Drop for Writing {
    fn drop(&mut self) {
        lock(&self.service.state).writing.remove(&self.key);
    }
}

/// The name of a harness, as a person knows it.
fn harness_name(agent: &GitAgent) -> &'static str {
    match agent.harness {
        nh_proto::api::common::Harness::Claude => "Claude Code",
        nh_proto::api::common::Harness::Codex => "Codex",
    }
}

/// What the agents do, for the question of the guard: "Claude Code is working in app-topic. A
/// switch changes its files."
pub(super) fn guard_text(busy: &[&GitAgent], worktree: &str, write: &str) -> String {
    match busy {
        [agent] => {
            let doing = match agent.state {
                GitAgentState::Starting => "is starting",
                GitAgentState::Working => "is working",
                GitAgentState::Waiting => "waits for you",
                _ => "has a state that neohar does not know",
            };
            format!(
                "{} {doing} in {worktree}. A {write} changes its files.",
                harness_name(agent)
            )
        }
        _ => format!(
            "{} agents work in {worktree}. A {write} changes their files.",
            busy.len()
        ),
    }
}

/// What a switch does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Plan {
    /// The worktree has the branch out already.
    Stay,
    /// `git switch <name>`.
    Local(String),
    /// `git switch --track -c <name> <remote branch>`: a new local branch that tracks it.
    Track { name: String, remote: String },
    /// Another worktree has the branch out.
    Elsewhere { name: String, worktree: PathBuf },
}

/// The local name of a remote branch: `origin/feat/x` is `feat/x` for the remote `origin`.
fn local_name<'a>(branch: &'a Branch, remotes: &[String]) -> &'a str {
    remotes
        .iter()
        .filter(|remote| branch.name.starts_with(&format!("{remote}/")))
        .max_by_key(|remote| remote.len())
        .map(|remote| &branch.name[remote.len() + 1..])
        .or_else(|| branch.name.split_once('/').map(|(_, rest)| rest))
        .unwrap_or(&branch.name)
}

/// What a switch to `wanted` (a reference or a name of `git_branches`) does in the worktree at
/// `root`. `None` for a branch that the repository does not have.
pub(super) fn plan(look: &Look, root: &Path, wanted: &str) -> Option<Plan> {
    let target = look
        .branches
        .iter()
        .find(|b| b.reference == wanted)
        .or_else(|| look.branches.iter().find(|b| b.name == wanted))?;
    let local = match target.kind {
        Kind::Local => target,
        Kind::Remote => {
            let name = local_name(target, &look.remotes);
            match look
                .branches
                .iter()
                .find(|b| b.kind == Kind::Local && b.name == name)
            {
                Some(local) => local,
                None => {
                    return Some(Plan::Track {
                        name: name.to_owned(),
                        remote: target.name.clone(),
                    });
                }
            }
        }
    };
    if local.head {
        return Some(Plan::Stay);
    }
    match &local.worktree {
        Some(worktree) if worktree != root => Some(Plan::Elsewhere {
            name: local.name.clone(),
            worktree: worktree.clone(),
        }),
        _ => Some(Plan::Local(local.name.clone())),
    }
}

fn stderr_of(error: &GitError) -> &str {
    match error {
        GitError::Failed { stderr, .. } => stderr,
        _ => "",
    }
}

/// The error of a switch, an unstash or a pull that git refused: a change that would be lost or a
/// conflict is `conflict`, a branch that another worktree has out is `conflict` with that
/// worktree, and anything else is `git_failed`.
fn refusal(error: &GitError) -> RpcError {
    let stderr = stderr_of(error);
    let first = stderr.lines().next().unwrap_or(stderr).to_owned();
    let mut rpc = if stderr.contains("would be overwritten") || stderr.contains("CONFLICT") {
        let mut rpc = RpcError::new(ErrorCode::Conflict, first);
        rpc.details = json!({ "kind": "changes", "git": stderr });
        rpc
    } else if stderr.contains("already used by worktree") || stderr.contains("already checked out")
    {
        let mut rpc = RpcError::new(ErrorCode::Conflict, first);
        rpc.details = json!({ "kind": "worktree", "git": stderr });
        rpc
    } else {
        git_failed(error)
    };
    if rpc.message.is_empty() {
        rpc.message = error.to_string();
    }
    rpc
}

/// The note of a fetch that failed. A fetch in the background cannot ask for a passphrase or a
/// token, so the note says to click Fetch.
pub(super) fn fetch_note(stderr: &str, background: bool) -> String {
    let asks = [
        "terminal prompts disabled",
        "could not read Username",
        "could not read Password",
        "Permission denied",
        "Host key verification failed",
        "passphrase",
        "Authentication failed",
    ];
    if background && asks.iter().any(|text| stderr.contains(text)) {
        return "The fetch needs your passphrase or a token. Click Fetch.".to_owned();
    }
    let first = stderr
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("git fetch failed")
        .trim();
    format!("The fetch failed: {first}")
}

impl GitViewService {
    /// Starts a write in a worktree or a repository, or `busy` when one runs there.
    fn begin(self: &Arc<Self>, key: &Path) -> Result<Writing, RpcError> {
        if !lock(&self.state).writing.insert(key.to_path_buf()) {
            return Err(RpcError::new(
                ErrorCode::Busy,
                "A git command runs in this worktree. Try again when it ends.",
            ));
        }
        Ok(Writing {
            service: Arc::clone(self),
            key: key.to_path_buf(),
        })
    }

    /// The agents of the worktree. When one of them is busy and `force` is false, the answer is
    /// `confirm_required` with them in `details`.
    fn guard(
        &self,
        place: &Place,
        look: &Look,
        force: bool,
        write: &str,
    ) -> Result<Vec<RunId>, RpcError> {
        let agents = agents::agents_in(&place.root, &look.roots(), &self.placed());
        let busy: Vec<&GitAgent> = agents
            .iter()
            .filter(|agent| agents::busy(agent.state))
            .collect();
        if !busy.is_empty() && !force {
            let mut error = RpcError::new(
                ErrorCode::ConfirmRequired,
                guard_text(&busy, &read::last_part(&place.root), write),
            );
            error.details = json!({ "kind": "agents", "agents": busy });
            return Err(error);
        }
        Ok(agents.iter().map(|agent| agent.run).collect())
    }

    /// Appends the event of a write that worked.
    async fn record(&self, payload: EventPayload, subject: &Path) {
        let Some(actor) = self.actor.get() else {
            return;
        };
        let subject = subject.to_string_lossy().into_owned();
        let appended = actor
            .update(move |mut parts| parts.commit(|tx| tx.append(Some(&subject), &payload)))
            .await;
        match appended {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => tracing::warn!(%error, "a write of git is not in the log"),
            Err(error) => tracing::warn!(%error, "a write of git is not in the log"),
        }
    }

    /// Looks again at once at every window that shows the repository, after a write.
    async fn look_again(&self, common_dir: &Path) {
        let windows: Vec<_> = lock(&self.state)
            .follows
            .iter()
            .filter(|(_, follow)| {
                follow
                    .place
                    .as_ref()
                    .is_some_and(|place| place.common_dir == common_dir)
            })
            .map(|(window, _)| *window)
            .collect();
        for window in windows {
            self.look_window(window, super::Depth::Full).await;
        }
    }

    /// Sets the work of a worktree or a repository, and publishes the heads that show it.
    fn set_work(&self, key: &Path, change: impl FnOnce(&mut Work)) {
        let Some(hub) = self.hub.get() else {
            return;
        };
        let placed = self.placed();
        let mut guard = lock(&self.state);
        let state = &mut *guard;
        change(state.work.entry(key.to_path_buf()).or_default());
        for (window, follow) in &mut state.follows {
            let shows = follow
                .place
                .as_ref()
                .is_some_and(|place| place.root == key || place.common_dir == key);
            if shows {
                let work = super::state_work(&state.work, follow.place.as_ref());
                super::publish(hub, *window, follow, &placed, now_ms(), false, work);
            }
        }
    }

    /// `git.switch`.
    pub(super) async fn switch(
        self: &Arc<Self>,
        place: Place,
        branch: &str,
        changes: Option<GitChanges>,
        force: bool,
    ) -> Result<(), RpcError> {
        let _writing = self.begin(&place.root)?;
        let look = self.look(&place).await.map_err(|e| git_failed(&e))?;
        let plan = plan(&look, &place.root, branch)
            .ok_or_else(|| RpcError::new(ErrorCode::NotFound, format!("no branch {branch}")))?;
        let (to, args): (String, Vec<String>) = match plan {
            Plan::Stay => return Ok(()),
            Plan::Elsewhere { name, worktree } => {
                let mut error = RpcError::new(
                    ErrorCode::Conflict,
                    format!(
                        "The worktree {} has the branch {name} out.",
                        read::last_part(&worktree)
                    ),
                );
                error.details = json!({
                    "kind": "worktree",
                    "worktree": worktree.to_string_lossy(),
                });
                return Err(error);
            }
            Plan::Local(name) => (
                name.clone(),
                ["switch", "--end-of-options", &name]
                    .map(str::to_owned)
                    .to_vec(),
            ),
            Plan::Track { name, remote } => (
                name.clone(),
                [
                    "switch",
                    "--track",
                    "-c",
                    &name,
                    "--end-of-options",
                    &remote,
                ]
                .map(str::to_owned)
                .to_vec(),
            ),
        };
        let writer = self.git.with_hooks();
        let agents = self.guard(&place, &look, force, "switch")?;
        let count = self
            .count(&place.root)
            .await
            .map_err(|error| RpcError::new(ErrorCode::GitFailed, error))?;
        let files = count.totals.as_ref().map_or(0, |totals| totals.files);
        let from = look
            .current()
            .map(|branch| branch.name.clone())
            .or_else(|| look.commit.clone());
        let from_text = from.clone().unwrap_or_else(|| "HEAD".to_owned());
        if files > 0 && changes.is_none() {
            let mut error = RpcError::new(
                ErrorCode::ConfirmRequired,
                format!(
                    "The worktree has {files} changed {}. Bring them to {to}, or leave them on {from_text}.",
                    if files == 1 { "file" } else { "files" }
                ),
            );
            error.details = json!({ "kind": "changes", "files": files, "from": from, "to": to });
            return Err(error);
        }
        let mut stash = None;
        if files > 0 && changes == Some(GitChanges::Leave) {
            let message = read::stash_message(&from_text);
            writer
                .run(
                    &place.root,
                    &[
                        "stash",
                        "push",
                        "--include-untracked",
                        "--message",
                        &message,
                    ],
                )
                .await
                .map_err(|error| refusal(&error))?;
            stash = self
                .git
                .run(
                    &place.root,
                    &["rev-parse", "--verify", "--quiet", "refs/stash"],
                )
                .await
                .ok()
                .map(|out| out.trim().to_owned());
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        if let Err(error) = writer.run(&place.root, &refs).await {
            // The changes go back to where they were.
            if stash.is_some()
                && let Err(back) = writer.run(&place.root, &["stash", "pop"]).await
            {
                tracing::warn!(%back, "the changes of a switch that failed stay in a stash");
            }
            self.look_again(&place.common_dir).await;
            return Err(refusal(&error));
        }
        let event = EventPayload::GitSwitched(GitWrite {
            path: place.root.to_string_lossy().into_owned(),
            command: format!("git {}", args.join(" ")),
            from,
            to: Some(to),
            stash,
            agents,
            background: false,
        });
        self.record(event, &place.root).await;
        self.look_again(&place.common_dir).await;
        Ok(())
    }

    /// `git.unstash`.
    pub(super) async fn unstash(
        self: &Arc<Self>,
        place: Place,
        stash: &str,
        force: bool,
    ) -> Result<(), RpcError> {
        let not_found = || RpcError::new(ErrorCode::NotFound, format!("no stash {stash}"));
        if !read::is_hash(stash) {
            return Err(not_found());
        }
        let _writing = self.begin(&place.root)?;
        let look = self.look(&place).await.map_err(|e| git_failed(&e))?;
        let found = look
            .stashes
            .iter()
            .find(|candidate| candidate.commit.starts_with(stash))
            .ok_or_else(not_found)?
            .clone();
        let agents = self.guard(&place, &look, force, "restore")?;
        self.git
            .with_hooks()
            .run(
                &place.root,
                &["stash", "pop", "--end-of-options", &found.name],
            )
            .await
            .map_err(|error| refusal(&error))?;
        let event = EventPayload::GitUnstashed(GitWrite {
            path: place.root.to_string_lossy().into_owned(),
            command: format!("git stash pop {}", found.name),
            from: None,
            to: look.current().map(|branch| branch.name.clone()),
            stash: Some(found.commit),
            agents,
            background: false,
        });
        self.record(event, &place.root).await;
        self.look_again(&place.common_dir).await;
        Ok(())
    }

    /// `git.fetch`, and a fetch in the background.
    pub(super) async fn fetch(
        self: &Arc<Self>,
        place: Place,
        background: bool,
    ) -> Result<JobId, RpcError> {
        let look = self.look(&place).await.map_err(|e| git_failed(&e))?;
        let remote = read::fetch_remote(
            look.current().and_then(|branch| branch.remote.as_deref()),
            &look.remotes,
        )
        .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "The repository has no remote."))?;
        let writing = self.begin(&place.common_dir)?;
        // A fetch in the background runs no hook: reference-transaction would run with no click.
        let git = if background {
            self.git.with_env(&quiet_env())
        } else {
            let writer = self.git.with_hooks();
            match self.askpass.as_ref().map(Askpass::env) {
                Some(Ok(env)) => writer.with_env(&env),
                Some(Err(error)) => {
                    tracing::warn!(%error, "a fetch cannot ask for a passphrase");
                    writer.with_env(&quiet_env())
                }
                None => writer.with_env(&quiet_env()),
            }
        }
        .with_limit(JOB_LIMIT);
        let job = self.jobs.start(format!("Fetch {remote}"))?;
        let id = job.id();
        let key = place.common_dir.clone();
        self.set_work(&key, |work| {
            work.job = Some(GitJob {
                kind: GitJobKind::Fetch,
                job: id,
            });
        });
        let this = Arc::clone(self);
        self.handle.spawn(async move {
            let _writing = writing;
            let args = fetch_args(&remote, background);
            let command = format!("git {}", args.join(" "));
            job.message(command.clone());
            let token = job.token();
            let run = git.run(&place.root, &args);
            let result = tokio::select! {
                result = tokio::time::timeout(JOB_LIMIT, run) => result
                    .unwrap_or_else(|_| Err(GitError::Io(std::io::Error::other("the fetch took more than 10 minutes")))),
                () = token.cancelled() => Err(GitError::Io(std::io::Error::other("the fetch was cancelled"))),
            };
            let note = match &result {
                Ok(_) => None,
                Err(error) => {
                    let text = match stderr_of(error) {
                        "" => error.to_string(),
                        stderr => stderr.to_owned(),
                    };
                    Some(fetch_note(&text, background))
                }
            };
            this.set_work(&key, |work| {
                work.job = None;
                work.note.clone_from(&note);
            });
            match result {
                Ok(_) => {
                    job.done(json!({ "remote": remote }));
                    let event = EventPayload::GitFetched(GitWrite {
                        path: key.to_string_lossy().into_owned(),
                        command,
                        from: None,
                        to: None,
                        stash: None,
                        agents: Vec::new(),
                        background,
                    });
                    this.record(event, &key).await;
                }
                Err(error) => job.fail(CallError {
                    code: ErrorCode::GitFailed,
                    message: note.unwrap_or_else(|| error.to_string()),
                    details: json!({ "git": stderr_of(&error) }),
                }),
            }
            this.look_again(&key).await;
        });
        Ok(id)
    }

    /// `git.pull`: `git merge --ff-only @{u}` as a job.
    pub(super) async fn pull(
        self: &Arc<Self>,
        place: Place,
        force: bool,
    ) -> Result<JobId, RpcError> {
        let writing = self.begin(&place.root)?;
        let look = self.look(&place).await.map_err(|e| git_failed(&e))?;
        let current = look.current().ok_or_else(|| {
            RpcError::new(
                ErrorCode::Conflict,
                "HEAD is detached, so it has no upstream.",
            )
        })?;
        let upstream = current.upstream.clone().ok_or_else(|| {
            RpcError::new(
                ErrorCode::NotFound,
                format!("The branch {} has no upstream.", current.name),
            )
        })?;
        if current.ahead > 0 && current.behind > 0 {
            let mut error = RpcError::new(
                ErrorCode::Conflict,
                format!(
                    "The branch {} and {upstream} diverged: {}↑ {}↓. This version pulls with a fast-forward only.",
                    current.name, current.ahead, current.behind
                ),
            );
            error.details =
                json!({ "kind": "diverged", "ahead": current.ahead, "behind": current.behind });
            return Err(error);
        }
        let agents = self.guard(&place, &look, force, "pull")?;
        let branch = current.name.clone();
        let job = self.jobs.start(format!("Pull {upstream}"))?;
        let id = job.id();
        let key = place.root.clone();
        self.set_work(&key, |work| {
            work.job = Some(GitJob {
                kind: GitJobKind::Pull,
                job: id,
            });
        });
        let this = Arc::clone(self);
        let git: Git = self.git.with_hooks().with_limit(JOB_LIMIT);
        self.handle.spawn(async move {
            let _writing = writing;
            let command = "git merge --ff-only @{u}".to_owned();
            job.message(command.clone());
            let token = job.token();
            let run = git.run(&place.root, &["merge", "--ff-only", "@{u}"]);
            let result = tokio::select! {
                result = tokio::time::timeout(JOB_LIMIT, run) => result
                    .unwrap_or_else(|_| Err(GitError::Io(std::io::Error::other("the pull took more than 10 minutes")))),
                () = token.cancelled() => Err(GitError::Io(std::io::Error::other("the pull was cancelled"))),
            };
            this.set_work(&key, |work| work.job = None);
            match result {
                Ok(_) => {
                    job.done(json!({ "branch": branch }));
                    let event = EventPayload::GitPulled(GitWrite {
                        path: key.to_string_lossy().into_owned(),
                        command,
                        from: None,
                        to: Some(branch),
                        stash: None,
                        agents,
                        background: false,
                    });
                    this.record(event, &key).await;
                }
                Err(error) => {
                    let refused = refusal(&error);
                    job.fail(CallError {
                        code: refused.code,
                        message: refused.message,
                        details: refused.details,
                    });
                }
            }
            this.look_again(&place.common_dir).await;
        });
        Ok(id)
    }

    /// The repositories whose fetch in the background is due: a window shows one of their
    /// worktrees, they have a remote, no fetch runs, and the last fetch or try is older than
    /// `git.fetch_minutes`. Each one gets a try now.
    pub(super) fn due_fetches(&self, state: &mut super::State, now: i64) -> Vec<Place> {
        let minutes = self.config.config().git.fetch_minutes;
        if minutes == 0 {
            return Vec::new();
        }
        let every = i64::from(minutes).saturating_mul(60_000);
        let mut due: Vec<Place> = Vec::new();
        for follow in state.follows.values() {
            let (Some(place), Some(look)) = (&follow.place, &follow.look) else {
                continue;
            };
            if look.remotes.is_empty() || due.iter().any(|d| d.common_dir == place.common_dir) {
                continue;
            }
            let busy = state.writing.contains(&place.common_dir)
                || state
                    .work
                    .get(&place.common_dir)
                    .is_some_and(|work| work.job.is_some());
            let last = look
                .fetched_at
                .max(state.fetch_tries.get(&place.common_dir).copied())
                .unwrap_or(0);
            if !busy && now.saturating_sub(last) >= every {
                due.push(place.clone());
            }
        }
        for place in &due {
            state.fetch_tries.insert(place.common_dir.clone(), now);
        }
        due
    }
}
