//! The git commands against repositories in a temporary directory, with the real `git`.

use super::*;

fn env() -> Vec<(String, String)> {
    vec![
        (
            "PATH".to_owned(),
            "/usr/bin:/bin:/usr/local/bin:/opt/homebrew/bin".to_owned(),
        ),
        (
            "HOME".to_owned(),
            std::env::temp_dir().display().to_string(),
        ),
        ("GIT_CONFIG_GLOBAL".to_owned(), "/dev/null".to_owned()),
        ("GIT_CONFIG_SYSTEM".to_owned(), "/dev/null".to_owned()),
        ("GIT_AUTHOR_NAME".to_owned(), "Ada".to_owned()),
        ("GIT_AUTHOR_EMAIL".to_owned(), "ada@example.com".to_owned()),
        ("GIT_COMMITTER_NAME".to_owned(), "Ada".to_owned()),
        (
            "GIT_COMMITTER_EMAIL".to_owned(),
            "ada@example.com".to_owned(),
        ),
    ]
}

/// A new empty folder in the temporary directory.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nh-core-git-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    std::fs::canonicalize(&dir).expect("a path")
}

/// A repository with one commit on `main`: `a.txt` with two lines.
async fn repo(name: &str) -> (Git, PathBuf) {
    let dir = folder(name);
    let git = Git::new(env());
    git.run(&dir, &["init", "-q", "-b", "main"])
        .await
        .expect("init");
    write(&dir, "a.txt", b"one\ntwo\n");
    commit(&git, &dir, "first").await;
    (git, dir)
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
async fn the_root_and_the_git_directory_of_a_repository_are_found() {
    let (git, dir) = repo("root").await;
    std::fs::create_dir_all(dir.join("sub")).expect("a directory");
    assert_eq!(git.repo_root(&dir.join("sub")).await.expect("a root"), dir);
    assert_eq!(
        git.git_dir(&dir).await.expect("a git directory"),
        dir.join(".git")
    );
    let outside = folder("not-a-repo");
    assert!(git.repo_root(&outside).await.is_err());
}

#[tokio::test]
async fn the_changes_from_head_are_the_files_that_are_not_in_a_commit() {
    let (git, dir) = repo("changes-head").await;
    let (changes, branch) = git.uncommitted(&dir).await.expect("changes");
    let head = git.run(&dir, &["rev-parse", "HEAD"]).await.expect("HEAD");
    assert_eq!(changes.head, head.trim());
    assert!(changes.files.is_empty());
    assert_eq!(changes.stats(), DiffStats { add: 0, del: 0 });
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
    assert_eq!(
        changes.files.iter().map(|f| f.kind).collect::<Vec<_>>(),
        [
            ChangeKind::Modified,
            ChangeKind::Untracked,
            ChangeKind::Added
        ]
    );
    let a = row(&changes, "a.txt");
    assert_eq!((a.add, a.del), (1, 0));
    assert_ne!(a.stamp, 0);
    assert!(a.stamp < 1 << 53, "JavaScript holds the stamp exactly");
    assert_eq!(changes.stats(), DiffStats { add: 4, del: 0 });
    // The patch of a row goes from HEAD.
    let patch = git
        .file_diff(&dir, &changes.head, row(&changes, "a.txt"))
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
async fn an_untracked_file_is_a_row_and_an_ignored_file_is_not() {
    let (git, dir) = repo("untracked").await;
    write(&dir, ".gitignore", b"build/\n");
    commit(&git, &dir, "ignore").await;
    write(&dir, "notes/new.md", b"a\nb\nc");
    write(&dir, "build/out.o", b"\0\0\0");
    let (changes, _) = git.uncommitted(&dir).await.expect("changes");
    assert_eq!(paths(&changes), ["notes/new.md"]);
    let new = row(&changes, "notes/new.md");
    assert_eq!(
        (new.kind, new.add, new.binary),
        (ChangeKind::Untracked, 3, false),
        "a last line without a newline counts, as git counts it"
    );
}

#[tokio::test]
async fn a_rename_a_deletion_a_binary_file_and_odd_paths_are_rows() {
    let (git, dir) = repo("kinds").await;
    write(
        &dir,
        "long.txt",
        b"one\ntwo\nthree\nfour\nfive\nsix\nseven\n",
    );
    write(&dir, "gone.txt", b"bye\n");
    commit(&git, &dir, "files").await;
    git.run(&dir, &["mv", "long.txt", "moved.txt"])
        .await
        .expect("a rename");
    git.run(&dir, &["rm", "-q", "gone.txt"])
        .await
        .expect("a removal");
    write(&dir, "image.png", b"\x89PNG\0\x01\x02");
    write(&dir, "with space.txt", b"x\n");
    write(&dir, "new\nline.txt", b"y\n");
    git.run(&dir, &["add", "-A"]).await.expect("add");
    let (changes, _) = git.uncommitted(&dir).await.expect("changes");
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
}

#[tokio::test]
async fn an_untracked_symlink_is_one_line_and_a_fifo_is_no_row() {
    let (git, dir) = repo("special").await;
    std::os::unix::fs::symlink("/etc/hosts", dir.join("link")).expect("a symlink");
    let fifo = dir.join("pipe");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo runs");
    assert!(made.success());
    // A read of the FIFO would wait for ever, and the test would not end.
    let (changes, _) = git.uncommitted(&dir).await.expect("changes");
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
async fn a_file_of_head_that_is_removed_is_a_deleted_row() {
    let (git, dir) = repo("deleted").await;
    std::fs::remove_file(dir.join("a.txt")).expect("a removal");
    let (changes, _) = git.uncommitted(&dir).await.expect("changes");
    let a = row(&changes, "a.txt");
    assert_eq!((a.kind, a.del, a.stamp), (ChangeKind::Deleted, 2, 0));
}

#[tokio::test]
async fn a_repository_with_no_commit_compares_with_the_empty_tree() {
    let dir = folder("unborn");
    let git = Git::new(env());
    git.run(&dir, &["init", "-q", "-b", "trunk"])
        .await
        .expect("init");
    write(&dir, "staged.txt", b"s\n");
    git.run(&dir, &["add", "staged.txt"]).await.expect("add");
    write(&dir, "loose.txt", b"a\nb\n");
    let (changes, branch) = git.uncommitted(&dir).await.expect("changes");
    assert_eq!(branch.as_deref(), Some("trunk"));
    assert_eq!(paths(&changes), ["loose.txt", "staged.txt"]);
    assert_eq!(row(&changes, "staged.txt").kind, ChangeKind::Added);
    assert_eq!(row(&changes, "loose.txt").kind, ChangeKind::Untracked);
    let patch = git
        .file_diff(&dir, &changes.head, row(&changes, "staged.txt"))
        .await
        .expect("a patch");
    assert!(patch.contains("+s"), "{patch}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_worktree_that_is_gone_is_an_error() {
    let (git, dir) = repo("gone").await;
    let error = git
        .uncommitted(&dir.with_extension("none"))
        .await
        .expect_err("no worktree");
    assert!(matches!(error, GitError::WorktreeGone), "{error:?}");
}

#[tokio::test]
async fn the_count_does_not_write_the_index_and_works_while_a_commit_holds_it() {
    let (git, dir) = repo("locks").await;
    write(&dir, "picture.bin", b"\0\x01\x02");
    commit(&git, &dir, "a binary file").await;
    let index = git
        .run(&dir, &["rev-parse", "--git-path", "index"])
        .await
        .expect("a path");
    let index = dir.join(index.trim());
    // Files whose stat changes and whose content does not: `git diff` would write the index.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    for name in ["a.txt", "picture.bin"] {
        let path = dir.join(name);
        let text = std::fs::read(&path).expect("a file");
        std::fs::write(&path, text).expect("the same bytes");
    }
    let before = std::fs::metadata(&index).expect("an index").modified();
    let (changes, _) = git.uncommitted(&dir).await.expect("changes");
    assert!(
        changes.files.is_empty(),
        "a file with a new stat and the old content is not a change, nor is a binary file: {:?}",
        paths(&changes)
    );
    let after = std::fs::metadata(&index).expect("an index").modified();
    assert_eq!(before.ok(), after.ok(), "the count wrote the index");
    // A commit of an agent holds the lock now: the count still works, and leaves the lock.
    let lock = index.with_extension("lock");
    std::fs::write(&lock, b"").expect("a lock");
    git.uncommitted(&dir).await.expect("changes");
    assert!(lock.exists());
    std::fs::remove_file(&lock).expect("the lock goes");
}

#[tokio::test]
async fn the_diff_of_a_row_is_its_patch_from_head() {
    let (git, dir) = repo("patch").await;
    write(&dir, "a.txt", b"one\nTWO\n");
    write(&dir, "b*.txt", b"star\n");
    write(&dir, "bx.txt", b"other\n");
    git.run(&dir, &["add", "-A"]).await.expect("add");
    let (changes, _) = git.uncommitted(&dir).await.expect("changes");
    let patch = git
        .file_diff(&dir, &changes.head, row(&changes, "a.txt"))
        .await
        .expect("a patch");
    assert!(patch.contains("-two\n+TWO\n"), "{patch}");
    // A path is not a pattern: `b*.txt` gives only its own file.
    let patch = git
        .file_diff(&dir, &changes.head, row(&changes, "b*.txt"))
        .await
        .expect("a patch");
    assert!(
        patch.contains("+star") && !patch.contains("other"),
        "{patch}"
    );
    // A rename gives one file with both paths.
    git.run(&dir, &["checkout", "-q", "HEAD", "--", "a.txt"])
        .await
        .expect("a checkout");
    git.run(&dir, &["mv", "a.txt", "c.txt"])
        .await
        .expect("a rename");
    let (changes, _) = git.uncommitted(&dir).await.expect("changes");
    let patch = git
        .file_diff(&dir, &changes.head, row(&changes, "c.txt"))
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
