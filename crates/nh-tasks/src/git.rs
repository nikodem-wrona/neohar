//! The git commands of a task, through the `git` program.
//!
//! The program is the one in the PATH of the environment that the caller gives, so the user's
//! configuration, credentials and hooks apply to every command.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use tokio::process::Command;

mod changes;

pub use changes::{ChangeKind, ChangedFile, Changes};

/// Why a git command did not give a result.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("cannot run git: {0}")]
    Io(#[from] std::io::Error),
    #[error("git {args} failed ({status}): {stderr}")]
    Failed {
        args: String,
        status: String,
        stderr: String,
    },
    #[error("{0}")]
    Unexpected(String),
    /// The worktree of the task is not on the disk.
    #[error("the worktree is gone")]
    WorktreeGone,
    /// The base of the task is not a commit of the repository now.
    #[error("the base `{0}` is gone")]
    BaseGone(String),
    /// The base and `HEAD` have no commit in common.
    #[error("the base `{0}` and HEAD have no commit in common")]
    NoMergeBase(String),
}

/// A worktree that `git worktree list` knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    /// The branch, or none for a detached head.
    pub branch: Option<String>,
}

/// Runs git with an environment.
#[derive(Clone, Debug, Default)]
pub struct Git {
    env: Vec<(String, String)>,
}

impl Git {
    /// `env` is the environment of the process that git runs in. It must have a `PATH`.
    pub fn new(env: Vec<(String, String)>) -> Self {
        Self { env }
    }

    async fn run(&self, cwd: &Path, args: &[&str]) -> Result<String, GitError> {
        let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
        let out = self.output(cwd, &args).await?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// The bytes that git writes, for output with `-z` and paths that are not UTF-8.
    async fn output(&self, cwd: &Path, args: &[&OsStr]) -> Result<Vec<u8>, GitError> {
        let mut command = Command::new("git");
        command
            .args(args)
            .current_dir(cwd)
            .env_clear()
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            // A prompt for a password would wait for ever.
            .env("GIT_TERMINAL_PROMPT", "0")
            // A read must not lock the index: a commit of an agent at the same time would fail.
            .env("GIT_OPTIONAL_LOCKS", "0")
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        let output = command.output().await?;
        if output.status.success() {
            return Ok(output.stdout);
        }
        Err(GitError::Failed {
            args: args
                .iter()
                .map(|arg| arg.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" "),
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }

    /// The root of the repository that holds `path`.
    pub async fn repo_root(&self, path: &Path) -> Result<PathBuf, GitError> {
        let out = self.run(path, &["rev-parse", "--show-toplevel"]).await?;
        Ok(PathBuf::from(out.trim()))
    }

    /// The git directory of a worktree: `.git` of the main worktree, or the directory under
    /// `.git/worktrees/` of a linked one. A commit changes its files.
    pub async fn git_dir(&self, worktree: &Path) -> Result<PathBuf, GitError> {
        let out = self
            .run(worktree, &["rev-parse", "--absolute-git-dir"])
            .await?;
        Ok(PathBuf::from(out.trim()))
    }

    /// The current branch of a checkout. A detached head gives the short name of the commit.
    pub async fn current_branch(&self, repo: &Path) -> Result<String, GitError> {
        let out = self
            .run(repo, &["rev-parse", "--abbrev-ref", "HEAD"])
            .await?;
        let name = out.trim();
        if name == "HEAD" {
            let sha = self.run(repo, &["rev-parse", "--short", "HEAD"]).await?;
            return Ok(sha.trim().to_owned());
        }
        Ok(name.to_owned())
    }

    /// Whether the branch exists in the repository.
    pub async fn branch_exists(&self, repo: &Path, branch: &str) -> bool {
        self.run(
            repo,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
        )
        .await
        .is_ok()
    }

    /// `git worktree add -b <branch> <path> <base>`.
    pub async fn worktree_add(
        &self,
        repo: &Path,
        path: &Path,
        branch: &str,
        base: &str,
    ) -> Result<(), GitError> {
        let path = path.to_string_lossy();
        self.run(repo, &["worktree", "add", "-b", branch, &path, base])
            .await
            .map(|_| ())
    }

    /// `git worktree remove [--force] <path>`.
    pub async fn worktree_remove(
        &self,
        repo: &Path,
        path: &Path,
        force: bool,
    ) -> Result<(), GitError> {
        let path = path.to_string_lossy();
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push(&path);
        self.run(repo, &args).await.map(|_| ())
    }

    /// The worktrees of the repository, the main one first.
    pub async fn worktrees(&self, repo: &Path) -> Result<Vec<Worktree>, GitError> {
        let out = self.run(repo, &["worktree", "list", "--porcelain"]).await?;
        Ok(parse_worktrees(&out))
    }

    /// Whether the worktree has changes that a removal would lose: a change to a tracked file or a
    /// file that git does not track.
    pub async fn dirty(&self, worktree: &Path) -> Result<bool, GitError> {
        let out = self.run(worktree, &["status", "--porcelain"]).await?;
        Ok(!out.trim().is_empty())
    }
}

fn parse_worktrees(text: &str) -> Vec<Worktree> {
    let mut out = Vec::new();
    let mut current: Option<Worktree> = None;
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            out.extend(current.take());
            current = Some(Worktree {
                path: PathBuf::from(path),
                branch: None,
            });
        } else if let Some(branch) = line.strip_prefix("branch ")
            && let Some(worktree) = current.as_mut()
        {
            worktree.branch = Some(
                branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch)
                    .to_owned(),
            );
        }
    }
    out.extend(current);
    out
}

#[cfg(test)]
mod tests;
