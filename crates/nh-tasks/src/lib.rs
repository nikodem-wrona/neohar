//! The pieces of a task workspace: the name of its worktree, the git
//! commands that make and remove it, the ports that it leases, and its setup commands.
//!
//! Nothing here knows the core. The core calls these functions from a job and keeps the result in
//! its tables. All git work goes through the `git` program, so the configuration and the hooks of
//! the user apply.

pub mod git;
pub mod ports;
pub mod setup;
pub mod slug;

pub use git::{ChangeKind, ChangedFile, Changes, Git, GitError, Worktree};
