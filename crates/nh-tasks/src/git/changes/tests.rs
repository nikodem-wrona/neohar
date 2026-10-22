//! The changes of a task against repositories in a temporary directory, with the real `git`.

use std::path::PathBuf;

use super::*;
use crate::git::tests::repo;

/// A repository with one commit on `main`, and the worktree of the task on `neohar/<name>`.
async fn task(name: &str) -> (Git, PathBuf, PathBuf) {
    let (git, dir) = repo(&format!("changes-{name}")).await;
    let worktree = start(&git, &dir, name).await;
    (git, dir, worktree)
}

/// The worktree of a task that starts from `main` now.
async fn start(git: &Git, dir: &Path, name: &str) -> PathBuf {
    let worktree = dir.with_extension("wt");
    let _ = std::fs::remove_dir_all(&worktree);
    git.worktree_add(dir, &worktree, &format!("neohar/{name}"), "main")
        .await
        .expect("a worktree");
    worktree
}

async fn commit(git: &Git, dir: &Path, message: &str) {
    git.run(dir, &["add", "-A"]).await.expect("add");
    git.run(dir, &["commit", "-q", "-m", message])
        .await
        .expect("a commit");
}

fn write(dir: &Path, name: &str, text: &[u8]) {
    let path = dir.join(OsStr::from_bytes(name.as_bytes()));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("a directory");
    }
    std::fs::write(path, text).expect("a file");
}

fn row<'a>(changes: &'a Changes, path: &str) -> &'a ChangedFile {
    changes
        .files
        .iter()
        .find(|file| file.path == path.as_bytes())
        .unwrap_or_else(|| panic!("no row {path:?} in {:?}", paths(changes)))
}

fn paths(changes: &Changes) -> Vec<String> {
    changes
        .files
        .iter()
        .map(|file| String::from_utf8_lossy(&file.path).into_owned())
        .collect()
}

#[tokio::test]
async fn a_task_without_changes_has_no_rows_and_the_merge_base_of_its_start() {
    let (git, dir, worktree) = task("none").await;
    let changes = git.changes(&worktree, "main").await.expect("changes");
    let head = git
        .run(&dir, &["rev-parse", "main"])
        .await
        .expect("a commit");
    assert_eq!(changes.merge_base, head.trim());
    assert!(head.starts_with(&changes.short) && changes.short.len() >= 7);
    assert_eq!((changes.commits, changes.files.len()), (0, 0));
    assert_eq!(changes.stats(), DiffStats { add: 0, del: 0 });
}

#[tokio::test]
async fn a_commit_a_change_in_the_index_and_a_change_in_a_file_are_rows() {
    let (git, _dir, worktree) = task("three").await;
    // A commit: 2 lines added and 1 removed in a.txt.
    write(&worktree, "a.txt", b"one\nthree\nfour\n");
    commit(&git, &worktree, "edit").await;
    // In the index only.
    write(&worktree, "staged.txt", b"s\n");
    git.run(&worktree, &["add", "staged.txt"])
        .await
        .expect("add");
    // In the file only, on top of the commit.
    write(&worktree, "a.txt", b"one\nthree\nfour\nfive\n");
    let changes = git.changes(&worktree, "main").await.expect("changes");
    assert_eq!(paths(&changes), ["a.txt", "staged.txt"]);
    assert_eq!(changes.commits, 1);
    let a = row(&changes, "a.txt");
    assert_eq!(
        (a.kind, a.add, a.del, a.uncommitted),
        (ChangeKind::Modified, 3, 1, true)
    );
    assert_ne!(a.stamp, 0);
    assert!(a.stamp < 1 << 53, "JavaScript holds the stamp exactly");
    let staged = row(&changes, "staged.txt");
    assert_eq!(
        (staged.kind, staged.add, staged.uncommitted),
        (ChangeKind::Added, 1, true)
    );
    assert_eq!(changes.stats(), DiffStats { add: 4, del: 1 });

    // After a commit of everything, nothing is uncommitted, and the rows stay.
    commit(&git, &worktree, "more").await;
    let changes = git.changes(&worktree, "main").await.expect("changes");
    assert_eq!(changes.commits, 2);
    assert!(changes.files.iter().all(|file| !file.uncommitted));
    assert_eq!(changes.stats(), DiffStats { add: 4, del: 1 });
}

#[tokio::test]
async fn an_untracked_file_is_a_row_and_an_ignored_file_is_not() {
    let (git, _dir, worktree) = task("untracked").await;
    write(&worktree, ".gitignore", b"build/\n");
    commit(&git, &worktree, "ignore").await;
    write(&worktree, "notes/new.md", b"a\nb\nc");
    write(&worktree, "build/out.o", b"\0\0\0");
    let changes = git.changes(&worktree, "main").await.expect("changes");
    assert_eq!(paths(&changes), [".gitignore", "notes/new.md"]);
    let new = row(&changes, "notes/new.md");
    assert_eq!(
        (new.kind, new.add, new.binary, new.uncommitted),
        (ChangeKind::Untracked, 3, false, true),
        "a last line without a newline counts, as git counts it"
    );
}

#[tokio::test]
async fn a_rename_a_deletion_a_binary_file_and_odd_paths_are_rows() {
    let (git, dir) = repo("changes-kinds").await;
    write(
        &dir,
        "long.txt",
        b"one\ntwo\nthree\nfour\nfive\nsix\nseven\n",
    );
    write(&dir, "gone.txt", b"bye\n");
    commit(&git, &dir, "files").await;
    let worktree = start(&git, &dir, "kinds").await;
    git.run(&worktree, &["mv", "long.txt", "moved.txt"])
        .await
        .expect("a rename");
    git.run(&worktree, &["rm", "-q", "gone.txt"])
        .await
        .expect("a removal");
    write(&worktree, "image.png", b"\x89PNG\0\x01\x02");
    write(&worktree, "with space.txt", b"x\n");
    write(&worktree, "new\nline.txt", b"y\n");
    commit(&git, &worktree, "kinds").await;
    let changes = git.changes(&worktree, "main").await.expect("changes");
    assert_eq!(changes.commits, 1);
    // The byte order of the path.
    assert_eq!(
        paths(&changes),
        [
            "gone.txt",
            "image.png",
            "moved.txt",
            "new\nline.txt",
            "with space.txt"
        ]
    );
    let gone = row(&changes, "gone.txt");
    assert_eq!(
        (gone.kind, gone.del, gone.stamp),
        (ChangeKind::Deleted, 1, 0)
    );
    let moved = row(&changes, "moved.txt");
    assert_eq!(moved.kind, ChangeKind::Renamed);
    assert_eq!(moved.old_path.as_deref(), Some(&b"long.txt"[..]));
    assert_eq!((moved.add, moved.del), (0, 0));
    let image = row(&changes, "image.png");
    assert_eq!((image.kind, image.binary), (ChangeKind::Added, true));
    assert_eq!(row(&changes, "with space.txt").add, 1);
    assert_eq!(row(&changes, "new\nline.txt").add, 1);
    assert!(changes.files.iter().all(|file| !file.uncommitted));
}

#[tokio::test]
async fn an_untracked_symlink_is_one_line_and_a_fifo_is_no_row() {
    let (git, _dir, worktree) = task("special").await;
    std::os::unix::fs::symlink("/etc/hosts", worktree.join("link")).expect("a symlink");
    let fifo = worktree.join("pipe");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo runs");
    assert!(made.success());
    // A read of the FIFO would wait for ever, and the test would not end.
    let changes = git.changes(&worktree, "main").await.expect("changes");
    let link = row(&changes, "link");
    assert_eq!(
        (link.kind, link.add, link.binary),
        (ChangeKind::Untracked, 1, false)
    );
    assert!(
        changes.files.iter().all(|file| file.path != b"pipe"),
        "git lists no FIFO"
    );
}

#[tokio::test]
async fn a_file_of_the_merge_base_that_the_task_removes_is_a_deleted_row() {
    let (git, _dir, worktree) = task("deleted").await;
    std::fs::remove_file(worktree.join("a.txt")).expect("a removal");
    let changes = git.changes(&worktree, "main").await.expect("changes");
    let a = row(&changes, "a.txt");
    assert_eq!(
        (a.kind, a.del, a.stamp, a.uncommitted),
        (ChangeKind::Deleted, 2, 0, true)
    );
}

#[tokio::test]
async fn the_merge_base_stays_when_the_base_moves_on() {
    let (git, dir, worktree) = task("moved-base").await;
    let start = git
        .run(&dir, &["rev-parse", "main"])
        .await
        .expect("a commit");
    write(&worktree, "task.txt", b"t\n");
    commit(&git, &worktree, "task").await;
    // The base gets a commit of its own after the start.
    write(&dir, "base.txt", b"b\nb\n");
    commit(&git, &dir, "base").await;
    let changes = git.changes(&worktree, "main").await.expect("changes");
    assert_eq!(changes.merge_base, start.trim());
    assert_eq!(
        paths(&changes),
        ["task.txt"],
        "the work of the base is not a row"
    );
    assert_eq!(changes.commits, 1);
}

#[tokio::test]
async fn a_line_that_a_change_undoes_without_a_commit_counts_once() {
    let (git, _dir, worktree) = task("undo").await;
    write(&worktree, "a.txt", b"one\ntwo\nthree\n");
    commit(&git, &worktree, "add three").await;
    // The file is as the merge base has it again, but not in a commit.
    write(&worktree, "a.txt", b"one\ntwo\n");
    let changes = git.changes(&worktree, "main").await.expect("changes");
    assert!(changes.files.is_empty(), "{:?}", paths(&changes));
    assert_eq!(changes.stats(), DiffStats { add: 0, del: 0 });
}

#[tokio::test]
async fn the_changes_from_head_are_the_files_that_are_not_in_a_commit() {
    let (git, dir) = repo("changes-head").await;
    let (changes, branch) = git.uncommitted(&dir).await.expect("changes");
    let head = git.run(&dir, &["rev-parse", "HEAD"]).await.expect("HEAD");
    assert_eq!(changes.merge_base, head.trim());
    assert!(head.starts_with(&changes.short) && changes.short.len() >= 7);
    assert_eq!((changes.commits, changes.files.len()), (0, 0));
    assert_eq!(branch.as_deref(), Some("main"));

    // A commit is not a change. A change in a file, one in the index, and a file that git does
    // not track are.
    write(&dir, "b.txt", b"b\n");
    commit(&git, &dir, "b").await;
    write(&dir, "a.txt", b"one\ntwo\nthree\n");
    write(&dir, "staged.txt", b"s\n");
    git.run(&dir, &["add", "staged.txt"]).await.expect("add");
    write(&dir, "new.md", b"x\ny\n");
    let (changes, _) = git.uncommitted(&dir).await.expect("changes");
    assert_eq!(paths(&changes), ["a.txt", "new.md", "staged.txt"]);
    assert_eq!(changes.commits, 0);
    assert!(changes.files.iter().all(|file| file.uncommitted));
    assert_eq!(
        changes.files.iter().map(|f| f.kind).collect::<Vec<_>>(),
        [
            ChangeKind::Modified,
            ChangeKind::Untracked,
            ChangeKind::Added
        ]
    );
    assert_eq!(changes.stats(), DiffStats { add: 4, del: 0 });
    // The patch of a row goes from HEAD.
    let patch = git
        .file_diff(&dir, &changes.merge_base, row(&changes, "a.txt"))
        .await
        .expect("a patch");
    assert!(patch.contains("+three"), "{patch}");

    // A detached HEAD has no branch.
    git.run(&dir, &["checkout", "-q", "--detach"])
        .await
        .expect("detach");
    let (changes, branch) = git.uncommitted(&dir).await.expect("changes");
    assert_eq!((branch, changes.files.len()), (None, 3));
}

#[tokio::test]
async fn a_repository_with_no_commit_compares_with_the_empty_tree() {
    let dir = std::env::temp_dir().join(format!("nh-tasks-{}-unborn", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    let git = Git::new(crate::git::tests::env());
    git.run(&dir, &["init", "-q", "-b", "trunk"])
        .await
        .expect("init");
    write(&dir, "staged.txt", b"s\n");
    git.run(&dir, &["add", "staged.txt"]).await.expect("add");
    write(&dir, "loose.txt", b"a\nb\n");
    let (changes, branch) = git.uncommitted(&dir).await.expect("changes");
    assert_eq!(branch.as_deref(), Some("trunk"));
    assert_eq!((changes.short.as_str(), changes.commits), ("", 0));
    assert_eq!(paths(&changes), ["loose.txt", "staged.txt"]);
    assert_eq!(row(&changes, "staged.txt").kind, ChangeKind::Added);
    assert_eq!(row(&changes, "loose.txt").kind, ChangeKind::Untracked);
    let patch = git
        .file_diff(&dir, &changes.merge_base, row(&changes, "staged.txt"))
        .await
        .expect("a patch");
    assert!(patch.contains("+s"), "{patch}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_base_that_is_gone_is_an_error_that_names_it() {
    let (git, dir, worktree) = task("gone-base").await;
    git.run(&dir, &["branch", "-q", "old"])
        .await
        .expect("a branch");
    git.run(&dir, &["branch", "-q", "-D", "old"])
        .await
        .expect("a removal");
    let error = git.changes(&worktree, "old").await.expect_err("no base");
    assert_eq!(error.to_string(), "the base `old` is gone");
    let error = git
        .changes(&worktree.with_extension("none"), "main")
        .await
        .expect_err("no worktree");
    assert!(matches!(error, GitError::WorktreeGone), "{error:?}");
}

#[tokio::test]
async fn the_count_does_not_write_the_index_and_works_while_a_commit_holds_it() {
    let (git, dir, worktree) = task("locks").await;
    write(&worktree, "picture.bin", b"\0\x01\x02");
    commit(&git, &worktree, "a binary file").await;
    let index = git
        .run(&worktree, &["rev-parse", "--git-path", "index"])
        .await
        .expect("a path");
    let index = worktree.join(index.trim());
    // Files whose stat changes and whose content does not: `git diff` would write the index.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    for name in ["a.txt", "picture.bin"] {
        let path = worktree.join(name);
        let text = std::fs::read(&path).expect("a file");
        std::fs::write(&path, text).expect("the same bytes");
    }
    let before = std::fs::metadata(&index).expect("an index").modified();
    let changes = git.changes(&worktree, "main").await.expect("changes");
    assert_eq!(
        paths(&changes),
        ["picture.bin"],
        "a file with a new stat and the old content is not a change"
    );
    assert!(
        !row(&changes, "picture.bin").uncommitted,
        "nor is a binary file"
    );
    let after = std::fs::metadata(&index).expect("an index").modified();
    assert_eq!(before.ok(), after.ok(), "the count wrote the index");
    // A commit of an agent holds the lock now: the count still works, and leaves the lock.
    let lock = index.with_extension("lock");
    std::fs::write(&lock, b"").expect("a lock");
    git.changes(&worktree, "main").await.expect("changes");
    assert!(lock.exists());
    std::fs::remove_file(&lock).expect("the lock goes");
    let _ = dir;
}

#[tokio::test]
async fn the_diff_of_a_row_is_its_patch_from_the_merge_base() {
    let (git, _dir, worktree) = task("patch").await;
    write(&worktree, "a.txt", b"one\nTWO\n");
    write(&worktree, "b*.txt", b"star\n");
    write(&worktree, "bx.txt", b"other\n");
    commit(&git, &worktree, "edit").await;
    let changes = git.changes(&worktree, "main").await.expect("changes");
    let patch = git
        .file_diff(&worktree, &changes.merge_base, row(&changes, "a.txt"))
        .await
        .expect("a patch");
    assert!(patch.contains("-two\n+TWO\n"), "{patch}");
    // A path is not a pattern: `b*.txt` gives only its own file.
    let patch = git
        .file_diff(&worktree, &changes.merge_base, row(&changes, "b*.txt"))
        .await
        .expect("a patch");
    assert!(
        patch.contains("+star") && !patch.contains("other"),
        "{patch}"
    );
    // A rename gives one file with both paths.
    git.run(&worktree, &["mv", "a.txt", "c.txt"])
        .await
        .expect("a rename");
    let changes = git.changes(&worktree, "main").await.expect("changes");
    let patch = git
        .file_diff(&worktree, &changes.merge_base, row(&changes, "c.txt"))
        .await
        .expect("a patch");
    assert!(
        patch.contains("rename from a.txt") && patch.contains("rename to c.txt"),
        "{patch}"
    );
}

#[test]
fn the_records_of_diff_index_join_the_raw_part_and_the_numstat_part() {
    let out = b":000000 100644 0000 0000 A\0bin.dat\0:100644 100644 aaaa 0000 M\0stat.txt\0\
:100644 100644 bbbb bbbb R100\0old name\0new\tname\0\
-\t-\tbin.dat\0\
0\t0\t\0old name\0new\tname\0";
    let records = parse_diff_index(out);
    assert_eq!(records.len(), 2, "stat.txt has no numstat record");
    assert_eq!(records[0].path, b"bin.dat");
    assert_eq!((records[0].status, records[0].lines), (b'A', None));
    assert_eq!(records[1].path, b"new\tname");
    assert_eq!(records[1].old_path.as_deref(), Some(&b"old name"[..]));
    assert_eq!((records[1].status, records[1].lines), (b'R', Some((0, 0))));
}
