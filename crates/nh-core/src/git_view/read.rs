//! The git commands that the Git view reads with, and the parsers of their output.
//!
//! Every command is plumbing or a read that never writes the index (`for-each-ref`,
//! `worktree list`, `log`, `diff-tree`, `status` with `GIT_OPTIONAL_LOCKS=0`), so a commit of an
//! agent at the same time does not fail on `index.lock`. The parsers are pure functions, and
//! `tests.rs` tests them on the bytes that git writes.

use std::{
    collections::HashSet,
    ffi::OsStr,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use nh_proto::api::model::GitBranchKind;

use crate::changes::git::{Git, GitError, Record, parse_diff_index};

/// Where a folder is in git.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Place {
    /// The root of the worktree that holds the folder.
    pub root: PathBuf,
    /// The git directory of that worktree.
    pub git_dir: PathBuf,
    /// The git directory that all worktrees of the repository share.
    pub common_dir: PathBuf,
}

fn canonical(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

/// The worktree and the git directories of a folder.
pub(crate) async fn place(git: &Git, folder: &Path) -> Result<Place, GitError> {
    let out = git
        .run(
            folder,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--show-toplevel",
                "--absolute-git-dir",
                "--git-common-dir",
            ],
        )
        .await?;
    let mut lines = out.lines().map(str::trim);
    let (Some(root), Some(git_dir), Some(common_dir)) = (lines.next(), lines.next(), lines.next())
    else {
        return Err(GitError::Failed {
            args: "rev-parse".to_owned(),
            status: "0".to_owned(),
            stderr: format!("an answer of {} lines", out.lines().count()),
        });
    };
    Ok(Place {
        root: canonical(PathBuf::from(root)),
        git_dir: canonical(PathBuf::from(git_dir)),
        common_dir: canonical(PathBuf::from(common_dir)),
    })
}

/// The name of a repository: the folder of its main worktree, or of a bare repository without
/// `.git` at its end.
pub(crate) fn repo_name(common_dir: &Path) -> String {
    let last = |path: &Path| {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned())
    };
    if common_dir.file_name() == Some(OsStr::new(".git"))
        && let Some(parent) = common_dir.parent()
    {
        return last(parent);
    }
    let name = last(common_dir);
    name.strip_suffix(".git")
        .filter(|stem| !stem.is_empty())
        .map(str::to_owned)
        .unwrap_or(name)
}

/// The last part of a path, for the name of a worktree.
pub(crate) fn last_part(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy())
        .into_owned()
}

// --- branches ---------------------------------------------------------------------------------

/// One branch of `git for-each-ref`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Branch {
    /// For example `refs/heads/main`.
    pub reference: String,
    /// For example `main` or `origin/main`.
    pub name: String,
    pub kind: Kind,
    pub commit: String,
    /// The time of the committer, in unix seconds.
    pub time: i64,
    pub subject: String,
    pub upstream: Option<String>,
    /// The remote of the upstream.
    pub remote: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// The upstream is gone from its remote.
    pub gone: bool,
    /// The worktree that has the branch out.
    pub worktree: Option<PathBuf>,
    /// The worktree where git runs has it out.
    pub head: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Kind {
    #[default]
    Local,
    Remote,
}

impl Kind {
    pub fn wire(self) -> GitBranchKind {
        match self {
            Self::Local => GitBranchKind::Local,
            Self::Remote => GitBranchKind::Remote,
        }
    }
}

/// The fields of each branch, with a NUL between them. The subject is last, so a NUL in it is
/// no harm.
const BRANCH_FORMAT: &str = "--format=%(refname)%00%(objectname)%00%(committerdate:unix)%00\
%(upstream:short)%00%(upstream:remotename)%00%(upstream:track,nobracket)%00%(worktreepath)%00\
%(HEAD)%00%(symref)%00%(subject)";

/// The local and the remote branches, the newest commit first.
pub(crate) async fn branches(git: &Git, worktree: &Path) -> Result<Vec<Branch>, GitError> {
    let out = git
        .output(
            worktree,
            &[
                "for-each-ref",
                "--sort=-committerdate",
                BRANCH_FORMAT,
                "refs/heads",
                "refs/remotes",
            ]
            .map(OsStr::new),
        )
        .await?;
    Ok(parse_branches(&out))
}

pub(crate) fn parse_branches(out: &[u8]) -> Vec<Branch> {
    let text = String::from_utf8_lossy(out);
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.splitn(10, '\0').collect();
            let [
                reference,
                commit,
                time,
                upstream,
                remote,
                track,
                worktree,
                head,
                symref,
                subject,
            ] = fields[..]
            else {
                return None;
            };
            // `refs/remotes/origin/HEAD` names another branch.
            if !symref.is_empty() {
                return None;
            }
            let (kind, name) = if let Some(name) = reference.strip_prefix("refs/heads/") {
                (Kind::Local, name)
            } else {
                (Kind::Remote, reference.strip_prefix("refs/remotes/")?)
            };
            let (ahead, behind, gone) = parse_track(track);
            let some = |text: &str| (!text.is_empty()).then(|| text.to_owned());
            Some(Branch {
                reference: reference.to_owned(),
                name: name.to_owned(),
                kind,
                commit: commit.to_owned(),
                time: time.parse().unwrap_or(0),
                subject: subject.to_owned(),
                upstream: some(upstream),
                remote: some(remote),
                ahead,
                behind,
                gone,
                worktree: some(worktree).map(|path| canonical(PathBuf::from(path))),
                head: head == "*",
            })
        })
        .collect()
}

/// `%(upstream:track,nobracket)`: `ahead 1, behind 2`, `ahead 3`, `gone` or nothing.
pub(crate) fn parse_track(track: &str) -> (u32, u32, bool) {
    let (mut ahead, mut behind) = (0, 0);
    for part in track.split(',').map(str::trim) {
        if let Some(n) = part.strip_prefix("ahead ") {
            ahead = n.parse().unwrap_or(0);
        } else if let Some(n) = part.strip_prefix("behind ") {
            behind = n.parse().unwrap_or(0);
        }
    }
    (ahead, behind, track.trim() == "gone")
}

/// How many branches the Branch menu shows as recent.
pub(crate) const RECENT: usize = 5;

/// The branches that `HEAD` of the worktree was on last, the newest first, from its reflog.
pub(crate) async fn recent(git: &Git, worktree: &Path) -> Result<Vec<String>, GitError> {
    let out = git
        .run(
            worktree,
            &["reflog", "show", "--format=%gs", "-n", "300", "HEAD", "--"],
        )
        .await;
    // A repository with no commit has no reflog.
    let Ok(out) = out else {
        return Ok(Vec::new());
    };
    Ok(parse_recent(&out))
}

/// The branches of `checkout: moving from <a> to <b>`, the newest first, each once: the branch
/// that `HEAD` went to, then the one that it left.
pub(crate) fn parse_recent(reflog: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    reflog
        .lines()
        .filter_map(|line| line.strip_prefix("checkout: moving from "))
        .filter_map(|rest| rest.rsplit_once(" to "))
        .flat_map(|(from, to)| [to.trim().to_owned(), from.trim().to_owned()])
        .filter(|name| seen.insert(name.clone()))
        .collect()
}

/// The remotes of the repository.
pub(crate) async fn remotes(git: &Git, worktree: &Path) -> Result<Vec<String>, GitError> {
    let out = git.run(worktree, &["remote"]).await?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_owned)
        .collect())
}

/// The remote of a fetch: the remote of the upstream, else `origin`, else the first one.
pub(crate) fn fetch_remote(upstream_remote: Option<&str>, remotes: &[String]) -> Option<String> {
    upstream_remote
        .filter(|remote| remotes.iter().any(|r| r == remote))
        .map(str::to_owned)
        .or_else(|| remotes.iter().find(|r| *r == "origin").cloned())
        .or_else(|| remotes.first().cloned())
}

/// The time of the last fetch in unix milliseconds: the newest `FETCH_HEAD` of the worktree or of
/// the repository.
pub(crate) fn fetched_at(place: &Place) -> Option<i64> {
    [&place.git_dir, &place.common_dir]
        .into_iter()
        .filter_map(|dir| std::fs::metadata(dir.join("FETCH_HEAD")).ok())
        .filter_map(|meta| meta.modified().ok())
        .filter_map(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
        .max()
}

/// A stash of the repository.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Stash {
    pub commit: String,
    /// For example `stash@{0}`.
    pub name: String,
    /// For example `On main: neohar: main`.
    pub subject: String,
    /// Unix seconds.
    pub time: i64,
}

/// The message of the stash that a switch leaves for a branch.
pub(crate) fn stash_message(branch: &str) -> String {
    format!("neohar: {branch}")
}

/// The stashes of the repository, the newest first.
pub(crate) async fn stashes(git: &Git, worktree: &Path) -> Result<Vec<Stash>, GitError> {
    let out = git
        .output(
            worktree,
            &["stash", "list", "--format=%H%x00%gd%x00%ct%x00%gs"].map(OsStr::new),
        )
        .await?;
    Ok(parse_stashes(&out))
}

pub(crate) fn parse_stashes(out: &[u8]) -> Vec<Stash> {
    String::from_utf8_lossy(out)
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(4, '\0');
            let (commit, name, time, subject) = (
                fields.next()?,
                fields.next()?,
                fields.next()?,
                fields.next()?,
            );
            Some(Stash {
                commit: commit.to_owned(),
                name: name.to_owned(),
                subject: subject.to_owned(),
                time: time.parse().unwrap_or(0),
            })
        })
        .collect()
}

/// The newest stash that a switch left for `branch`.
pub(crate) fn stash_of<'a>(stashes: &'a [Stash], branch: &str) -> Option<&'a Stash> {
    let end = format!(": {}", stash_message(branch));
    stashes.iter().find(|stash| stash.subject.ends_with(&end))
}

// --- worktrees --------------------------------------------------------------------------------

/// One worktree of `git worktree list`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Worktree {
    pub path: PathBuf,
    pub commit: Option<String>,
    /// For example `refs/heads/main`. `None` for a detached `HEAD`.
    pub branch: Option<String>,
    pub bare: bool,
    pub locked: bool,
    pub prunable: bool,
}

/// The worktrees of the repository, the main one first.
pub(crate) async fn worktrees(git: &Git, worktree: &Path) -> Result<Vec<Worktree>, GitError> {
    let out = git
        .output(
            worktree,
            &["worktree", "list", "--porcelain", "-z"].map(OsStr::new),
        )
        .await?;
    let mut list = parse_worktrees(&out);
    for item in &mut list {
        if !item.prunable {
            item.path = canonical(std::mem::take(&mut item.path));
        }
    }
    Ok(list)
}

/// `git worktree list --porcelain -z`: a NUL after each line, and an empty line after each
/// worktree.
pub(crate) fn parse_worktrees(out: &[u8]) -> Vec<Worktree> {
    let mut list = Vec::new();
    let mut current: Option<Worktree> = None;
    for field in out.split(|b| *b == 0) {
        if field.is_empty() {
            list.extend(current.take());
            continue;
        }
        let (key, value) = match field.iter().position(|b| *b == b' ') {
            Some(at) => (&field[..at], &field[at + 1..]),
            None => (field, &b""[..]),
        };
        let text = || String::from_utf8_lossy(value).into_owned();
        match key {
            b"worktree" => {
                list.extend(current.take());
                current = Some(Worktree {
                    path: PathBuf::from(OsStr::from_bytes(value)),
                    ..Worktree::default()
                });
            }
            _ => {
                let Some(item) = current.as_mut() else {
                    continue;
                };
                match key {
                    b"HEAD" => item.commit = Some(text()),
                    b"branch" => item.branch = Some(text()),
                    b"bare" => item.bare = true,
                    b"locked" => item.locked = true,
                    b"prunable" => item.prunable = true,
                    _ => {}
                }
            }
        }
    }
    list.extend(current);
    list
}

/// The files of a worktree that are not committed, from `git status`. With
/// `GIT_OPTIONAL_LOCKS=0`, status does not write the index.
pub(crate) async fn dirty(git: &Git, worktree: &Path) -> Result<u32, GitError> {
    let out = git
        .output(
            worktree,
            &[
                "status",
                "--porcelain=v1",
                "-z",
                // Each new file counts, as in the changes of the worktree.
                "--untracked-files=all",
                "--ignore-submodules=dirty",
            ]
            .map(OsStr::new),
        )
        .await?;
    Ok(parse_status_count(&out))
}

/// The entries of `git status --porcelain=v1 -z`. A rename or a copy has its old path in the next
/// field.
pub(crate) fn parse_status_count(out: &[u8]) -> u32 {
    let mut count = 0u32;
    let mut fields = out.split(|b| *b == 0).filter(|f| !f.is_empty());
    while let Some(entry) = fields.next() {
        count = count.saturating_add(1);
        if matches!(entry.first(), Some(b'R' | b'C')) || matches!(entry.get(1), Some(b'R' | b'C')) {
            let _old = fields.next();
        }
    }
    count
}

// --- history ----------------------------------------------------------------------------------

/// One commit of `git log` or `git show`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Commit {
    pub hash: String,
    pub short: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    /// The time of the author, in unix seconds.
    pub time: i64,
    pub branches: Vec<String>,
    pub tags: Vec<String>,
    /// `HEAD` is at the commit.
    pub head: bool,
    pub subject: String,
    /// The message after the subject.
    pub body: String,
}

/// The fields of a commit, with a NUL between them. The body is last.
const COMMIT_FIELDS: &str = "%H%x00%h%x00%P%x00%an%x00%ae%x00%at%x00%D%x00%s%x00%b";

/// A page of the commits of `HEAD`, the newest first. A repository with no commit has none.
pub(crate) async fn log(
    git: &Git,
    worktree: &Path,
    skip: u32,
    count: u32,
) -> Result<Vec<Commit>, GitError> {
    let format = format!("--format={COMMIT_FIELDS}%x1e");
    let skip = format!("--skip={skip}");
    let count = format!("--max-count={count}");
    let result = git
        .output(
            worktree,
            &[
                "log",
                "--no-color",
                "--no-show-signature",
                &format,
                &skip,
                &count,
                "HEAD",
                "--",
            ]
            .map(OsStr::new),
        )
        .await;
    match result {
        Ok(out) => Ok(parse_commits(&out)),
        Err(GitError::Failed { stderr, .. }) if no_commit(&stderr) => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

/// What git says about `HEAD` in a repository with no commit.
fn no_commit(stderr: &str) -> bool {
    stderr.contains("does not have any commits")
        || stderr.contains("bad default revision")
        || stderr.contains("unknown revision")
}

/// One commit, with its message.
pub(crate) async fn show(git: &Git, worktree: &Path, hash: &str) -> Result<Commit, GitError> {
    let format = format!("--format={COMMIT_FIELDS}%x1e");
    let out = git
        .output(
            worktree,
            &[
                "show",
                "-s",
                "--no-color",
                "--no-show-signature",
                &format,
                hash,
                "--",
            ]
            .map(OsStr::new),
        )
        .await?;
    parse_commits(&out)
        .into_iter()
        .next()
        .ok_or(GitError::Failed {
            args: "show".to_owned(),
            status: "0".to_owned(),
            stderr: "no commit".to_owned(),
        })
}

/// The commits of `--format=<COMMIT_FIELDS>%x1e`. Git ends each one with a newline, also after the
/// separator.
pub(crate) fn parse_commits(out: &[u8]) -> Vec<Commit> {
    let text = String::from_utf8_lossy(out);
    text.split('\u{1e}')
        .filter_map(|record| {
            let record = record.trim_start_matches('\n');
            let fields: Vec<&str> = record.splitn(9, '\0').collect();
            let [
                hash,
                short,
                parents,
                author,
                email,
                time,
                refs,
                subject,
                body,
            ] = fields[..]
            else {
                return None;
            };
            if hash.is_empty() {
                return None;
            }
            let (head, branches, tags) = parse_refs(refs);
            Some(Commit {
                hash: hash.to_owned(),
                short: short.to_owned(),
                parents: parents.split_whitespace().map(str::to_owned).collect(),
                author: author.to_owned(),
                email: email.to_owned(),
                time: time.parse().unwrap_or(0),
                branches,
                tags,
                head,
                subject: subject.to_owned(),
                body: body.trim_end().to_owned(),
            })
        })
        .collect()
}

/// `%D`: `HEAD -> main, origin/main, origin/HEAD, tag: v1.0`. Gives whether `HEAD` is there, the
/// branches and the tags. `origin/HEAD` names another branch, so it is left out.
pub(crate) fn parse_refs(refs: &str) -> (bool, Vec<String>, Vec<String>) {
    let (mut head, mut branches, mut tags) = (false, Vec::new(), Vec::new());
    for part in refs.split(", ").map(str::trim).filter(|p| !p.is_empty()) {
        if let Some(branch) = part.strip_prefix("HEAD -> ") {
            head = true;
            branches.push(branch.to_owned());
        } else if part == "HEAD" {
            head = true;
        } else if let Some(tag) = part.strip_prefix("tag: ") {
            tags.push(tag.to_owned());
        } else if !part.ends_with("/HEAD") {
            branches.push(part.to_owned());
        }
    }
    (head, branches, tags)
}

/// The commits of `HEAD` that no remote branch has: the ones that a push would send. At most
/// `limit` of them.
pub(crate) async fn unpushed(
    git: &Git,
    worktree: &Path,
    limit: u32,
) -> Result<HashSet<String>, GitError> {
    let count = format!("--max-count={limit}");
    let out = git
        .run(
            worktree,
            &["rev-list", &count, "HEAD", "--not", "--remotes"],
        )
        .await?;
    Ok(out.lines().map(str::trim).map(str::to_owned).collect())
}

/// Whether `text` can be a hash that a person gives: 4 to 64 hex digits. A call never passes
/// anything else to git, so it cannot be an option.
pub(crate) fn is_hash(text: &str) -> bool {
    (4..=64).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The name that git reads a commit by: a hash, as a commit. A hash of another object fails.
pub(crate) fn commit_rev(hash: &str) -> String {
    format!("{hash}^{{commit}}")
}

/// Whether git refused a commit because the repository does not have it.
pub(crate) fn no_such_commit(error: &GitError) -> bool {
    let GitError::Failed { stderr, .. } = error else {
        return false;
    };
    [
        "unknown revision",
        "bad object",
        "bad revision",
        "ambiguous argument",
        "not a commit",
        "invalid object",
    ]
    .iter()
    .any(|text| stderr.contains(text))
}

/// The options of `git diff-tree` with one commit: the changes from its first parent, and from
/// the empty tree for a commit with no parent. A merge shows the changes that it brought to the
/// branch.
const ONE_COMMIT: [&str; 2] = ["--root", "--diff-merges=first-parent"];

/// The files of a commit, from its first parent. `rev` is a [`commit_rev`].
pub(crate) async fn commit_files(
    git: &Git,
    worktree: &Path,
    rev: &str,
) -> Result<Vec<Record>, GitError> {
    let mut args: Vec<&str> = vec![
        "diff-tree",
        "--no-commit-id",
        "-r",
        "-z",
        "-M",
        "--raw",
        "--numstat",
    ];
    args.extend(ONE_COMMIT);
    args.push(rev);
    let out = git
        .output(worktree, &args.iter().map(OsStr::new).collect::<Vec<_>>())
        .await?;
    let mut records = parse_diff_index(&out);
    records.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(records)
}

/// The patch of one file of a commit, from its first parent, with the old path of a rename. The
/// paths are literal, not patterns.
pub(crate) async fn commit_patch(
    git: &Git,
    worktree: &Path,
    rev: &str,
    old: Option<&[u8]>,
    path: &[u8],
) -> Result<String, GitError> {
    let mut args: Vec<&OsStr> = [
        "--literal-pathspecs",
        "diff-tree",
        "--no-commit-id",
        "-p",
        "-M",
        "--no-color",
        "--no-ext-diff",
    ]
    .map(OsStr::new)
    .to_vec();
    args.extend(ONE_COMMIT.map(OsStr::new));
    args.push(OsStr::new(rev));
    args.push(OsStr::new("--"));
    if let Some(old) = old {
        args.push(OsStr::from_bytes(old));
    }
    args.push(OsStr::from_bytes(path));
    let out = git.output(worktree, &args).await?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

// --- time ---------------------------------------------------------------------------------------

/// How long ago a moment was, as a person says it: "just now", "5 minutes ago", "3 days ago". A
/// moment after `now` is "just now". Both are unix milliseconds.
pub(crate) fn ago(now: i64, then: i64) -> String {
    let seconds = now.saturating_sub(then).max(0) / 1000;
    let count = |n: i64, one: &str| {
        if n == 1 {
            format!("1 {one} ago")
        } else {
            format!("{n} {one}s ago")
        }
    };
    match seconds {
        0..60 => "just now".to_owned(),
        60..3_600 => count(seconds / 60, "minute"),
        3_600..86_400 => count(seconds / 3_600, "hour"),
        86_400..2_592_000 => count(seconds / 86_400, "day"),
        2_592_000..31_536_000 => count(seconds / 2_592_000, "month"),
        _ => count(seconds / 31_536_000, "year"),
    }
}
