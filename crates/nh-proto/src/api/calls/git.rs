//! Params and results of `git.*`: the Git view of a window, and the same reads for the CLI.
//!
//! A call acts on one git worktree. The UI names it with `window`: the worktree that the Git view
//! of the window follows (`git.follow`). The CLI names it with `path`: the worktree that holds
//! that absolute path. A call gives one of the two.
//!
//! A write that changes the files of a worktree (`git.switch`, `git.pull`, `git.unstash`) asks
//! first when an agent works in it: without `force`, it answers `confirm_required` with the
//! agents in `details`. One write at a time runs in a worktree, and a second one is `busy`.
//!
//! The settings of a repository can name programs that git runs, so the core runs no git command
//! in a repository until the person trusts it (`git.trust`). Until then, `changes_head` and
//! `git_head` have `trust`, and a call answers `permission_denied` with
//! `{"kind": "untrusted", "trust"}` in `details`. When the settings that name a program change
//! after the trust, the core asks again.

use crate::api::{
    common::wire,
    ids::WindowId,
    model::{ChangeRow, GitBranchValue, GitCommit, GitHead, GitRepoValue, GitWorktreeValue},
    rich::DiffStats,
};

/// The most commits of one page of `git.log`.
pub const GIT_LOG_LIMIT: u32 = 200;

fn log_limit() -> u32 {
    GIT_LOG_LIMIT
}

wire! {
    /// `git.follow`: the Git view of a window shows the git worktree that holds `path`. The core
    /// fills `git_head:<window>`, `git_worktrees:<window>`, `git_branches:<window>` and
    /// `git_changes:<window>` while the window follows one of them, and looks again when the
    /// worktree or its git directory changes.
    pub struct GitFollow {
        pub window: WindowId,
        /// An absolute path in the worktree: the folder of a pane, or a row of `git_worktrees`.
        pub path: String,
    }

    /// `git.repo_add`: `git_repos` keeps the repository that holds `path`, also when no workspace
    /// and no pane is in it.
    pub struct GitRepoAdd {
        /// An absolute path in a git worktree.
        pub path: String,
    }

    /// `git.trust`: the person trusts the git repository that holds `path`. The core then runs git
    /// in it and in its worktrees. It keeps the settings of the repository that name a program,
    /// and asks again when they change. A trust again keeps the settings of now.
    pub struct GitTrust {
        /// An absolute path in a git worktree, for example `path` of `trust`.
        pub path: String,
    }

    /// `git.untrust`: the person takes back the trust of the git repository that holds `path`.
    /// The core runs no git command in it until the person trusts it again.
    pub struct GitUntrust {
        /// An absolute path in a git worktree.
        pub path: String,
    }

    /// The repository of `git.trust` or `git.untrust`.
    pub struct GitTrustResult {
        /// The root of the worktree that holds the path.
        pub root: String,
        /// The common git directory: the key of the trust, which its worktrees share.
        pub common_dir: String,
    }

    /// The worktree of a read: the one that the Git view of `window` follows, or the one that
    /// holds `path`.
    pub struct GitPlace {
        /// The worktree that the Git view of this window shows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub window: Option<WindowId>,
        /// The worktree that holds this absolute path. The CLI gives the current directory.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub path: Option<String>,
    }

    /// The rows of `git_repos`, in their order.
    pub struct GitRepoList {
        pub repos: Vec<GitRepoValue>,
    }

    /// `git.status`: the head and the changed files of a worktree, as `git_head:<window>` and
    /// `git_changes:<window>` have them.
    pub struct GitStatus {
        pub head: GitHead,
        pub changes: Vec<ChangeRow>,
    }

    /// The rows of `git_worktrees:<window>` for a worktree, in their order.
    pub struct GitWorktreeList {
        pub worktrees: Vec<GitWorktreeValue>,
    }

    /// The rows of `git_branches:<window>` for a worktree, in their order.
    pub struct GitBranchList {
        pub branches: Vec<GitBranchValue>,
    }

    /// `git.log`: the commits of `HEAD`, the newest first, in pages.
    pub struct GitLog {
        /// The worktree that the Git view of this window shows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub window: Option<WindowId>,
        /// The worktree that holds this absolute path. The CLI gives the current directory.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub path: Option<String>,
        /// The commits to leave out at the start: the number that the pages before had.
        #[serde(default)]
        pub skip: u32,
        /// The most commits of the page, at most 200.
        #[serde(default = "log_limit")]
        pub limit: u32,
    }

    pub struct GitLogResult {
        /// The commit of `HEAD` when the page was read. Another value than the page before means
        /// that the history changed: read it again from the start.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub head: Option<String>,
        pub commits: Vec<GitCommit>,
        /// More commits follow this page.
        pub more: bool,
    }

    /// `git.show`: one commit with its message and its files.
    pub struct GitShow {
        /// The worktree that the Git view of this window shows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub window: Option<WindowId>,
        /// The worktree that holds this absolute path. The CLI gives the current directory.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub path: Option<String>,
        /// The full or the short hash of the commit.
        pub commit: String,
    }

    pub struct GitShown {
        pub commit: GitCommit,
        /// The e-mail address of the author.
        pub email: String,
        /// The message after the subject, without the blank line between them.
        pub body: String,
        /// The short hashes of the parents. A merge has two or more. The files are the changes
        /// from the first parent.
        pub parents: Vec<String>,
        /// The changed files, in the byte order of the path. `stamp` is 0.
        pub files: Vec<ChangeRow>,
        /// The lines of all files.
        pub stats: DiffStats,
        /// The commit has more than 1,000 files, and `files` has the first 1,000.
        #[serde(default)]
        pub truncated: bool,
    }

    /// What a switch does with the changes of the worktree.
    #[derive(Copy, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum GitChanges {
        /// The changes go to the other branch, as `git switch` takes them. A change that the
        /// other branch would overwrite stops the switch (`conflict`).
        Bring,
        /// The changes go to a stash that names the branch. A switch back to that branch offers
        /// them again (`stash` of `git_head`).
        Leave,
    }

    /// `git.switch`: the worktree goes to another branch. A worktree with changes needs
    /// `changes`, else the answer is `confirm_required` with `{"kind": "changes", "files"}`. A
    /// branch that another worktree has out is `conflict` with `{"kind": "worktree", "worktree"}`.
    pub struct GitSwitch {
        /// The worktree that the Git view of this window shows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub window: Option<WindowId>,
        /// The worktree that holds this absolute path. The CLI gives the current directory.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub path: Option<String>,
        /// A branch of `git_branches`: its reference (`refs/heads/main`) or its name (`main`,
        /// `origin/topic`). A remote branch with no local branch of the same name gets a local
        /// branch that tracks it.
        pub branch: String,
        /// What happens to the changes of the worktree: `bring` or `leave`. A worktree with
        /// changes needs it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub changes: Option<GitChanges>,
        /// Switch also when an agent works in the worktree.
        #[serde(default)]
        pub force: bool,
    }

    /// `git.unstash`: puts the changes of a stash back into the worktree, and drops the stash.
    pub struct GitUnstash {
        /// The worktree that the Git view of this window shows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub window: Option<WindowId>,
        /// The worktree that holds this absolute path. The CLI gives the current directory.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub path: Option<String>,
        /// The commit of the stash, as `stash` of `git_head` gives it.
        pub stash: String,
        /// Put the changes back also when an agent works in the worktree.
        #[serde(default)]
        pub force: bool,
    }

    /// `git.fetch`: fetches the remote of the head with `--prune`, as a job. A question of ssh or
    /// of a credential helper shows the sheet of the app.
    pub struct GitFetch {
        /// The worktree that the Git view of this window shows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub window: Option<WindowId>,
        /// The worktree that holds this absolute path. The CLI gives the current directory.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub path: Option<String>,
    }

    /// `git.pull`: moves the branch to its upstream with a fast-forward only, as a job. A branch
    /// that has commits of its own and is behind is `conflict` with `{"kind": "diverged"}`.
    pub struct GitPull {
        /// The worktree that the Git view of this window shows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub window: Option<WindowId>,
        /// The worktree that holds this absolute path. The CLI gives the current directory.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub path: Option<String>,
        /// Pull also when an agent works in the worktree.
        #[serde(default)]
        pub force: bool,
    }

    /// `git.diff`: the diff of one file. Without `commit`, the file is a row of the changes of the
    /// worktree, from `HEAD`. With `commit`, it is a file of that commit, from its first parent.
    pub struct GitDiff {
        /// The worktree that the Git view of this window shows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub window: Option<WindowId>,
        /// The worktree that holds this absolute path. The CLI gives the current directory.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub path: Option<String>,
        /// The path of the file from the root of the worktree.
        pub file: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub commit: Option<String>,
    }
}
