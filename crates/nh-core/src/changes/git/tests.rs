//! The git commands against repositories in a temporary directory, with the real `git`.

use super::*;

pub(crate) fn env() -> Vec<(String, String)> {
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
pub(crate) fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nh-core-git-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    std::fs::canonicalize(&dir).expect("a path")
}

/// A repository with one commit on `main`: `a.txt` with two lines.
pub(crate) async fn repo(name: &str) -> (Git, PathBuf) {
    let dir = folder(name);
    let git = Git::new(env());
    git.run(&dir, &["init", "-q", "-b", "main"])
        .await
        .expect("init");
    write(&dir, "a.txt", b"one\ntwo\n");
    commit(&git, &dir, "first").await;
    (git, dir)
}

pub(crate) async fn commit(git: &Git, dir: &Path, message: &str) {
    git.run(dir, &["add", "-A"]).await.expect("add");
    git.run(dir, &["commit", "-q", "-m", message])
        .await
        .expect("a commit");
}

pub(crate) fn write(dir: &Path, name: &str, text: &[u8]) {
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
    assert_eq!(
        git.place(&dir.join("sub")).await.expect("a place").root,
        dir
    );
    assert_eq!(
        git.git_dir(&dir).await.expect("a git directory"),
        dir.join(".git")
    );
    let outside = folder("not-a-repo");
    assert!(git.place(&outside).await.is_err());
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

// --- the settings that always apply ------------------------------------------------------------

/// A folder for the files that a program of a hostile repository writes. It must stay empty.
pub(crate) fn markers(name: &str) -> PathBuf {
    folder(&format!("{name}-markers"))
}

#[track_caller]
pub(crate) fn assert_empty(dir: &Path) {
    let found: Vec<_> = std::fs::read_dir(dir)
        .expect("the markers")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .collect();
    assert!(
        found.is_empty(),
        "a program of the repository ran: {found:?}"
    );
}

/// A shell script with mode 0755.
pub(crate) fn script(path: &Path, text: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{text}\n")).expect("a script");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("a mode");
}

#[tokio::test]
async fn git_runs_no_fsmonitor_program_of_the_repository() {
    let (git, dir) = repo("fsmonitor").await;
    let marks = markers("fsmonitor");
    let program = format!("touch {}/fsmonitor; false", marks.display());
    git.run(&dir, &["config", "core.fsmonitor", &program])
        .await
        .expect("config");
    write(&dir, "a.txt", b"one\ntwo\nthree\n");
    let (changes, _) = git.uncommitted(&dir).await.expect("changes");
    assert_eq!(paths(&changes), ["a.txt"]);
    assert_empty(&marks);
}

#[tokio::test]
async fn git_uses_no_bare_repository_that_a_clone_commits() {
    let (git, dir) = repo("embedded").await;
    let marks = markers("embedded");
    // A bare repository in a folder of the checkout, whose worktree is the checkout.
    write(&dir, "evil/HEAD", b"ref: refs/heads/main\n");
    write(&dir, "evil/objects/.keep", b"");
    write(&dir, "evil/refs/heads/.keep", b"");
    let config = format!(
        "[core]\n\trepositoryformatversion = 0\n\tbare = false\n\tworktree = ..\n\
         \tfsmonitor = \"touch {}/embedded; false\"\n",
        marks.display()
    );
    write(&dir, "evil/config", config.as_bytes());
    commit(&git, &dir, "an embedded repository").await;
    let evil = dir.join("evil");
    assert!(git.place(&evil).await.is_err());
    assert!(git.uncommitted(&evil).await.is_err());
    assert_empty(&marks);
}

#[tokio::test]
async fn a_read_runs_no_hook_and_a_write_from_a_click_does() {
    let (git, dir) = repo("hooks").await;
    let marks = markers("hooks");
    script(
        &dir.join(".git/hooks/reference-transaction"),
        &format!("touch {}/hook", marks.display()),
    );
    git.run(&dir, &["update-ref", "refs/heads/read", "HEAD"])
        .await
        .expect("a ref");
    assert_empty(&marks);
    git.with_hooks()
        .run(&dir, &["update-ref", "refs/heads/click", "HEAD"])
        .await
        .expect("a ref");
    assert!(
        marks.join("hook").exists(),
        "a write from a click runs hooks"
    );
}

#[tokio::test]
async fn a_remote_of_the_ext_transport_runs_no_command() {
    let (git, dir) = repo("ext").await;
    let marks = markers("ext");
    let url = format!("ext::sh -c touch% {}/ext", marks.display());
    git.run(&dir, &["config", "protocol.ext.allow", "always"])
        .await
        .expect("config");
    git.run(&dir, &["remote", "add", "origin", &url])
        .await
        .expect("a remote");
    assert!(git.run(&dir, &["fetch", "origin"]).await.is_err());
    assert_empty(&marks);
}

#[test]
fn the_program_comes_from_an_absolute_entry_of_the_path() {
    let path = |value: &str| vec![("PATH".to_owned(), value.to_owned())];
    let found = find_program(&path("bin::.:/usr/bin:/bin")).expect("a git");
    assert!(found.is_absolute() && found.ends_with("git"), "{found:?}");
    assert_eq!(find_program(&path("bin:.:node_modules/.bin")), None);
    assert_eq!(find_program(&[]), None);
}

#[tokio::test]
async fn a_git_of_the_repository_in_a_relative_entry_of_the_path_never_runs() {
    let dir = folder("relative-path");
    let marks = markers("relative-path");
    std::fs::create_dir_all(dir.join("bin")).expect("a folder");
    for fake in [dir.join("bin/git"), dir.join("git")] {
        script(&fake, &format!("touch {}/fake; exit 0", marks.display()));
    }
    let mut env = env();
    for (key, value) in &mut env {
        if key == "PATH" {
            *value = format!("bin:.::{value}");
        }
    }
    let git = Git::new(env);
    let program = git.program().expect("a git");
    assert!(
        program.is_absolute() && !program.starts_with(&dir),
        "{program:?}"
    );
    git.run(&dir, &["--version"]).await.expect("git runs");
    assert_empty(&marks);
}

#[tokio::test]
async fn a_read_that_waits_stops_at_the_limit() {
    let (git, dir) = repo("limit").await;
    // Git opens the file of excludes for `ls-files --exclude-standard`, and an open of a FIFO
    // waits for a writer.
    let fifo = folder("limit-fifo").join("excludes");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo runs");
    assert!(made.success());
    let path = fifo.display().to_string();
    git.run(&dir, &["config", "core.excludesFile", &path])
        .await
        .expect("config");
    let started = std::time::Instant::now();
    let error = git
        .with_limit(Duration::from_secs(2))
        .uncommitted(&dir)
        .await
        .expect_err("the read waits");
    assert!(matches!(error, GitError::Timeout { .. }), "{error:?}");
    assert!(started.elapsed() < Duration::from_secs(60));
}

#[tokio::test]
async fn an_output_stops_at_its_cap() {
    let (git, dir) = repo("cap").await;
    let args = ["log", "--format=%H%n%H%n%H", "HEAD"].map(OsStr::new);
    let (out, cut) = git.output_capped(&dir, &args, 8).await.expect("an output");
    assert_eq!((out.len(), cut), (8, true));
    let (out, cut) = git
        .output_capped(&dir, &args, 1 << 10)
        .await
        .expect("an output");
    assert_eq!((out.len(), cut), (123, false));
}

#[tokio::test]
async fn git_gets_only_the_absolute_entries_of_the_path_for_its_own_programs() {
    let (_, dir) = repo("helper-path").await;
    let marks = markers("helper-path");
    // A remote `evil::<address>` runs the program `git-remote-evil`, which git finds in the PATH.
    std::fs::create_dir_all(dir.join("bin")).expect("a folder");
    script(
        &dir.join("bin/git-remote-evil"),
        &format!("touch {}/helper; exit 1", marks.display()),
    );
    let mut env = env();
    let mut absolute = String::new();
    for (key, value) in &mut env {
        if key == "PATH" {
            absolute.clone_from(value);
            *value = format!("bin:.::{value}");
        }
    }
    let git = Git::new(env);
    assert_eq!(git.env_value("PATH"), Some(absolute.as_str()));
    git.run(&dir, &["remote", "add", "ev", "evil::anything"])
        .await
        .expect("a remote");
    assert!(git.run(&dir, &["fetch", "ev"]).await.is_err());
    assert!(git.with_hooks().run(&dir, &["fetch", "ev"]).await.is_err());
    assert_empty(&marks);
}

#[test]
fn the_names_of_the_hooks_of_the_settings_come_once_each() {
    let out = b"hook.lint.command\0hook.lint.event\0hook.a.b=c.command\0HOOK.Up.event\0";
    assert_eq!(parse_hook_names(out), ["lint", "a.b=c", "Up"]);
    assert!(parse_hook_names(b"").is_empty());
}

#[tokio::test]
async fn no_git_runs_in_a_submodule_also_for_a_switch_from_a_click() {
    let (git, dir) = repo("submodule").await;
    let marks = markers("submodule");
    // A submodule with two commits: `main` has the first, `other` the second.
    let (_, sub) = repo("submodule-sub").await;
    let first = git
        .run(&sub, &["rev-parse", "HEAD"])
        .await
        .expect("a commit");
    write(&sub, "b.txt", b"b\n");
    commit(&git, &sub, "second").await;
    let url = sub.display().to_string();
    git.run(
        &dir,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            &url,
            "sub",
        ],
    )
    .await
    .expect("a submodule");
    let module = dir.join("sub");
    git.run(&module, &["checkout", "-q", first.trim()])
        .await
        .expect("the first commit");
    commit(&git, &dir, "a submodule").await;
    git.run(&dir, &["switch", "-q", "-c", "other"])
        .await
        .expect("a branch");
    git.run(&module, &["checkout", "-q", "main"])
        .await
        .expect("the second commit");
    commit(&git, &dir, "the second commit of the submodule").await;
    git.run(&dir, &["switch", "-q", "main"])
        .await
        .expect("back to main");
    git.run(&module, &["checkout", "-q", first.trim()])
        .await
        .expect("the first commit");
    // The submodule has hooks of its own, and the repository asks git to go into it.
    let hooks = dir.join(".git/modules/sub/hooks");
    for hook in ["post-checkout", "reference-transaction"] {
        script(
            &hooks.join(hook),
            &format!("touch {}/{hook}", marks.display()),
        );
    }
    git.run(&dir, &["config", "submodule.recurse", "true"])
        .await
        .expect("config");
    git.with_hooks()
        .run(&dir, &["switch", "-q", "other"])
        .await
        .expect("a switch");
    assert_empty(&marks);
    let head = git
        .run(&module, &["rev-parse", "HEAD"])
        .await
        .expect("HEAD");
    assert_eq!(
        head.trim(),
        first.trim(),
        "the submodule stays where it was"
    );
}
