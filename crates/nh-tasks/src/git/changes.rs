//! The changes of a task: from the merge base of the base and `HEAD` to the files in the
//! worktree, with the commits of the branch, the changes that are not committed, and the files
//! that git does not track and does not ignore. The changes of a worktree that is not the worktree
//! of a task go from `HEAD`.
//!
//! The count reads the trees with the plumbing command `git diff-index`, never with `git diff`.
//! When `git diff` finds a file whose stat changed, it writes the index again, and it takes
//! `index.lock` for that also with `GIT_OPTIONAL_LOCKS=0`. A commit of an agent at that moment
//! would fail. `diff-index` does not write the index. Its numstat leaves out a text file whose
//! stat changed but whose content did not. A binary file shows `-` in both columns, so the count
//! compares the hash of each such file with the blob of the tree.

use std::{
    collections::HashSet,
    ffi::OsStr,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::Path,
};

use nh_proto::api::rich::DiffStats;

use super::{Git, GitError};

/// What happened to a file between the merge base and the worktree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    /// Also a change of type and a file with a conflict.
    Modified,
    Added,
    Deleted,
    Renamed,
    /// A file that git does not track and does not ignore.
    Untracked,
}

/// One file of the changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangedFile {
    /// The path from the root of the worktree, as git gives its bytes.
    pub path: Vec<u8>,
    /// The path in the merge base of a renamed file.
    pub old_path: Option<Vec<u8>>,
    pub kind: ChangeKind,
    pub add: u32,
    pub del: u32,
    /// Git shows no lines for the file.
    pub binary: bool,
    /// The file has a change that is not in a commit.
    pub uncommitted: bool,
    /// Changes when the file in the worktree changes: from its size and its modification time.
    /// A deleted file has 0.
    pub stamp: u64,
}

/// The changes of a worktree against a base.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changes {
    /// The merge base of the base and `HEAD`.
    pub merge_base: String,
    /// The short name of the merge base.
    pub short: String,
    /// The commits from the merge base to `HEAD`.
    pub commits: u32,
    /// Every changed file, in the byte order of the path.
    pub files: Vec<ChangedFile>,
}

impl Changes {
    /// The lines of all files.
    pub fn stats(&self) -> DiffStats {
        self.files
            .iter()
            .fold(DiffStats { add: 0, del: 0 }, |sum, file| DiffStats {
                add: sum.add.saturating_add(file.add),
                del: sum.del.saturating_add(file.del),
            })
    }
}

/// The largest file that does not track whose lines the count reads.
const COUNT_LIMIT: u64 = 1 << 20;

impl Git {
    /// The changes of `worktree` against `base`. A base that is gone, or a worktree that
    /// is gone, is an error with a text for the person.
    pub async fn changes(&self, worktree: &Path, base: &str) -> Result<Changes, GitError> {
        if !worktree.is_dir() {
            return Err(GitError::WorktreeGone);
        }
        let merge_base = self.merge_base(worktree, base).await?;
        let short = self
            .run(worktree, &["rev-parse", "--short", &merge_base])
            .await?
            .trim()
            .to_owned();
        let rows = self.diff_index(worktree, &merge_base).await?;
        let uncommitted: HashSet<Vec<u8>> = self
            .diff_index(worktree, "HEAD")
            .await?
            .into_iter()
            .flat_map(|record| std::iter::once(record.path).chain(record.old_path))
            .collect();
        let untracked = self.untracked(worktree).await?;
        let commits = self
            .run(
                worktree,
                &["rev-list", "--count", &format!("{merge_base}..HEAD")],
            )
            .await?
            .trim()
            .parse()
            .unwrap_or(0);
        let files = files(worktree, rows, &untracked, |record| {
            uncommitted.contains(&record.path)
                || record
                    .old_path
                    .as_ref()
                    .is_some_and(|old| uncommitted.contains(old))
        });
        Ok(Changes {
            merge_base,
            short,
            commits,
            files,
        })
    }

    /// The changes of `worktree` that are not in a commit: from `HEAD` to the files, with
    /// the files that git does not track and does not ignore. Every row is `uncommitted`, and the
    /// count has no commits. A repository with no commit yet compares with the empty tree, and its
    /// short name is empty. Also gives the branch of `HEAD`, or `None` for a detached `HEAD`.
    pub async fn uncommitted(
        &self,
        worktree: &Path,
    ) -> Result<(Changes, Option<String>), GitError> {
        if !worktree.is_dir() {
            return Err(GitError::WorktreeGone);
        }
        let (head, short, branch) = match self
            .run(worktree, &["rev-parse", "HEAD", "--abbrev-ref", "HEAD"])
            .await
        {
            Ok(out) => {
                let mut lines = out.lines().map(str::trim);
                let head = lines.next().unwrap_or_default().to_owned();
                let branch = lines
                    .next()
                    .filter(|name| *name != "HEAD")
                    .map(str::to_owned);
                let short = self
                    .run(worktree, &["rev-parse", "--short", &head])
                    .await?
                    .trim()
                    .to_owned();
                (head, short, branch)
            }
            // No commit yet: `HEAD` names a branch that does not exist.
            Err(GitError::Failed { .. }) => {
                let branch = self
                    .run(worktree, &["symbolic-ref", "--quiet", "--short", "HEAD"])
                    .await
                    .ok()
                    .map(|out| out.trim().to_owned());
                let empty = self
                    .run(worktree, &["hash-object", "-t", "tree", "/dev/null"])
                    .await?
                    .trim()
                    .to_owned();
                (empty, String::new(), branch)
            }
            Err(error) => return Err(error),
        };
        let rows = self.diff_index(worktree, &head).await?;
        let untracked = self.untracked(worktree).await?;
        let files = files(worktree, rows, &untracked, |_| true);
        Ok((
            Changes {
                merge_base: head,
                short,
                commits: 0,
                files,
            },
            branch,
        ))
    }

    /// The paths that git does not track and does not ignore, each with a NUL at its end.
    async fn untracked(&self, worktree: &Path) -> Result<Vec<u8>, GitError> {
        self.output(
            worktree,
            &["ls-files", "--others", "--exclude-standard", "-z"].map(OsStr::new),
        )
        .await
    }

    /// The patch of one file from the merge base to the worktree, with the old path of a rename.
    /// The paths are literal, not patterns.
    pub async fn file_diff(
        &self,
        worktree: &Path,
        merge_base: &str,
        file: &ChangedFile,
    ) -> Result<String, GitError> {
        let mut args: Vec<&OsStr> = [
            "--literal-pathspecs",
            "diff-index",
            "-p",
            "-M",
            "--no-color",
            "--no-ext-diff",
            merge_base,
            "--",
        ]
        .map(OsStr::new)
        .to_vec();
        if let Some(old) = &file.old_path {
            args.push(OsStr::from_bytes(old));
        }
        args.push(OsStr::from_bytes(&file.path));
        let out = self.output(worktree, &args).await?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    async fn merge_base(&self, worktree: &Path, base: &str) -> Result<String, GitError> {
        match self.run(worktree, &["merge-base", base, "HEAD"]).await {
            Ok(out) => Ok(out.trim().to_owned()),
            Err(GitError::Failed { .. }) => {
                let commit = format!("{base}^{{commit}}");
                let known = self
                    .run(worktree, &["rev-parse", "--verify", "--quiet", &commit])
                    .await
                    .is_ok();
                Err(if known {
                    GitError::NoMergeBase(base.to_owned())
                } else {
                    GitError::BaseGone(base.to_owned())
                })
            }
            Err(error) => Err(error),
        }
    }

    /// The files whose content differs between `tree` and the worktree.
    async fn diff_index(&self, worktree: &Path, tree: &str) -> Result<Vec<Record>, GitError> {
        let out = self
            .output(
                worktree,
                &["diff-index", "--raw", "--numstat", "-z", "-M", tree].map(OsStr::new),
            )
            .await?;
        let mut records = parse_diff_index(&out);
        // A binary file whose stat changed but whose content did not.
        let suspects: Vec<usize> = records
            .iter()
            .enumerate()
            .filter(|(_, r)| r.lines.is_none() && r.status == b'M' && r.unchanged_mode())
            .map(|(i, _)| i)
            .collect();
        if !suspects.is_empty() {
            let mut args: Vec<&OsStr> = ["hash-object", "--"].map(OsStr::new).to_vec();
            args.extend(
                suspects
                    .iter()
                    .map(|i| OsStr::from_bytes(&records[*i].path)),
            );
            let hashes = self.output(worktree, &args).await?;
            let hashes = String::from_utf8_lossy(&hashes);
            let same: HashSet<usize> = suspects
                .iter()
                .zip(hashes.lines())
                .filter(|(i, hash)| records[**i].old_blob == hash.as_bytes())
                .map(|(i, _)| *i)
                .collect();
            let mut index = 0;
            records.retain(|_| {
                index += 1;
                !same.contains(&(index - 1))
            });
        }
        Ok(records)
    }
}

/// The rows of a count: the records of `diff-index` and the files that git does not track, in
/// the byte order of the path. `uncommitted` tells whether a record has a change that is not in a
/// commit.
fn files(
    worktree: &Path,
    rows: Vec<Record>,
    untracked: &[u8],
    uncommitted: impl Fn(&Record) -> bool,
) -> Vec<ChangedFile> {
    let mut files: Vec<ChangedFile> = rows
        .into_iter()
        .map(|record| {
            let uncommitted = uncommitted(&record);
            let kind = match record.status {
                b'A' | b'C' => ChangeKind::Added,
                b'D' => ChangeKind::Deleted,
                b'R' => ChangeKind::Renamed,
                _ => ChangeKind::Modified,
            };
            let stamp = if kind == ChangeKind::Deleted {
                0
            } else {
                stamp(&worktree.join(OsStr::from_bytes(&record.path)))
            };
            let (add, del) = record.lines.unwrap_or((0, 0));
            ChangedFile {
                path: record.path,
                old_path: record.old_path,
                kind,
                add,
                del,
                binary: record.lines.is_none(),
                uncommitted,
                stamp,
            }
        })
        .collect();
    let known: HashSet<Vec<u8>> = files.iter().map(|file| file.path.clone()).collect();
    for path in untracked.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        if known.contains(path) {
            continue;
        }
        let full = worktree.join(OsStr::from_bytes(path));
        let (lines, binary) = read_lines(&full);
        files.push(ChangedFile {
            path: path.to_vec(),
            old_path: None,
            kind: ChangeKind::Untracked,
            add: lines,
            del: 0,
            binary,
            uncommitted: true,
            stamp: stamp(&full),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

/// One file of `git diff-index --raw --numstat -z`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Record {
    path: Vec<u8>,
    old_path: Option<Vec<u8>>,
    /// The first letter of the status: `A`, `C`, `D`, `M`, `R`, `T`, `U` or `X`.
    status: u8,
    old_mode: Vec<u8>,
    new_mode: Vec<u8>,
    /// The blob in the tree.
    old_blob: Vec<u8>,
    /// The lines added and removed, or `None` for a binary file.
    lines: Option<(u32, u32)>,
}

impl Record {
    fn unchanged_mode(&self) -> bool {
        self.old_mode == self.new_mode
    }
}

/// The files of `git diff-index --raw --numstat -z`: the raw records come first, then the numstat
/// records, in the same order. A file has a numstat record only when its content or its mode
/// changed, so a file without one is left out. A rename has two paths in both parts.
fn parse_diff_index(out: &[u8]) -> Vec<Record> {
    let mut tokens = out.split(|b| *b == 0).peekable();
    let mut raw: Vec<Record> = Vec::new();
    while let Some(token) = tokens.next_if(|t| t.starts_with(b":")) {
        let fields: Vec<&[u8]> = token[1..].split(|b| *b == b' ').collect();
        let [old_mode, new_mode, old_blob, _, status] = fields[..] else {
            continue;
        };
        let letter = status.first().copied().unwrap_or(b'M');
        let mut record = Record {
            status: letter,
            old_mode: old_mode.to_vec(),
            new_mode: new_mode.to_vec(),
            old_blob: old_blob.to_vec(),
            ..Record::default()
        };
        if matches!(letter, b'R' | b'C') {
            record.old_path = tokens.next().map(<[u8]>::to_vec);
        }
        record.path = tokens.next().map(<[u8]>::to_vec).unwrap_or_default();
        raw.push(record);
    }
    let mut out = Vec::new();
    let mut next = 0;
    while let Some(token) = tokens.next() {
        if token.is_empty() {
            continue;
        }
        let mut parts = token.splitn(3, |b| *b == b'\t');
        let (Some(add), Some(del), Some(rest)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let path = if rest.is_empty() {
            // A rename: the old path and the new path follow.
            let _old = tokens.next();
            tokens.next().unwrap_or_default()
        } else {
            rest
        };
        let lines = match (number(add), number(del)) {
            (Some(add), Some(del)) => Some((add, del)),
            _ => None,
        };
        // The records are in the same order. Look further only when one is missing.
        let found = raw[next..]
            .iter()
            .position(|record| record.path == path)
            .map(|offset| next + offset);
        if let Some(index) = found {
            let mut record = std::mem::take(&mut raw[index]);
            record.lines = lines;
            out.push(record);
            next = index + 1;
        }
    }
    out
}

fn number(text: &[u8]) -> Option<u32> {
    std::str::from_utf8(text).ok()?.parse().ok()
}

/// The stamps are less than 2^53, so that the page holds them exactly as JavaScript numbers.
const STAMP_MASK: u64 = (1 << 53) - 1;

/// A number that changes when the file changes: a hash of its modification time, its size and its
/// inode. A file that is not there has 0, and a file that is there never has 0.
fn stamp(path: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    std::fs::symlink_metadata(path).map_or(0, |meta| {
        let mut hasher = std::hash::DefaultHasher::new();
        (meta.mtime(), meta.mtime_nsec(), meta.len(), meta.ino()).hash(&mut hasher);
        (hasher.finish() & STAMP_MASK).max(1)
    })
}

/// The lines of a file that git does not track, and whether it is binary. A file of more than
/// [`COUNT_LIMIT`] counts no lines. A symlink is one line, its target, as git shows it. A
/// FIFO, a socket or a device is never opened: a read of a FIFO can wait for ever.
fn read_lines(path: &Path) -> (u32, bool) {
    use std::io::Read;
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return (0, false);
    };
    if meta.file_type().is_symlink() {
        return (1, false);
    }
    if !meta.is_file() {
        return (0, false);
    }
    let Ok(file) = std::fs::File::open(path) else {
        return (0, false);
    };
    let mut bytes = Vec::new();
    // A large file shows whether it is binary in its first 8 KiB, as git looks.
    let limit = if meta.len() > COUNT_LIMIT {
        8 << 10
    } else {
        COUNT_LIMIT
    };
    if file.take(limit).read_to_end(&mut bytes).is_err() {
        return (0, false);
    }
    if bytes.iter().take(8 << 10).any(|b| *b == 0) {
        return (0, true);
    }
    if meta.len() > COUNT_LIMIT {
        return (0, false);
    }
    let mut lines = bytes.iter().filter(|b| **b == b'\n').count();
    // A last line without a newline is a line too, as git counts it.
    if bytes.last().is_some_and(|b| *b != b'\n') {
        lines += 1;
    }
    (u32::try_from(lines).unwrap_or(u32::MAX), false)
}

#[cfg(test)]
mod tests;
