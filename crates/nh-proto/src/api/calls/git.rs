//! Params and results of `git.*`: the Git view of a window, and the same reads for the CLI.
//!
//! A call acts on one git worktree. The UI names it with `window`: the worktree that the Git view
//! of the window follows (`git.follow`). The CLI names it with `path`: the worktree that holds
//! that absolute path. A call gives one of the two.

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

    /// The worktree of a read: the one that the Git view of `window` follows, or the one that
    /// holds `path`.
    pub struct GitPlace {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub window: Option<WindowId>,
        /// An absolute path in the worktree. The CLI gives the current directory.
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub window: Option<WindowId>,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub window: Option<WindowId>,
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

    /// `git.diff`: the diff of one file. Without `commit`, the file is a row of the changes of the
    /// worktree, from `HEAD`. With `commit`, it is a file of that commit, from its first parent.
    pub struct GitDiff {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub window: Option<WindowId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub path: Option<String>,
        /// The path of the file from the root of the worktree.
        pub file: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub commit: Option<String>,
    }
}
