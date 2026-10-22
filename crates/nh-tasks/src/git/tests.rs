//! The git commands against a repository in a temporary directory.

use std::path::PathBuf;

use super::*;

pub(super) fn env() -> Vec<(String, String)> {
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

/// A repository with one commit on `main`.
pub(super) async fn repo(name: &str) -> (Git, PathBuf) {
    let dir = std::env::temp_dir().join(format!("nh-tasks-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    let dir = std::fs::canonicalize(&dir).expect("a path");
    let git = Git::new(env());
    git.run(&dir, &["init", "-q", "-b", "main"])
        .await
        .expect("init");
    std::fs::write(dir.join("a.txt"), "one\ntwo\n").expect("a file");
    git.run(&dir, &["add", "."]).await.expect("add");
    git.run(&dir, &["commit", "-q", "-m", "first"])
        .await
        .expect("commit");
    (git, dir)
}

#[tokio::test]
async fn the_root_and_the_branch_of_a_repository_are_found() {
    let (git, dir) = repo("root").await;
    std::fs::create_dir_all(dir.join("sub")).expect("a directory");
    assert_eq!(git.repo_root(&dir.join("sub")).await.expect("a root"), dir);
    assert_eq!(git.current_branch(&dir).await.expect("a branch"), "main");
    assert!(git.branch_exists(&dir, "main").await);
    assert!(!git.branch_exists(&dir, "nope").await);
    let outside = std::env::temp_dir().join("nh-tasks-not-a-repo");
    std::fs::create_dir_all(&outside).expect("a directory");
    assert!(git.repo_root(&outside).await.is_err());
}

#[tokio::test]
async fn a_worktree_is_added_listed_and_removed() {
    let (git, dir) = repo("worktree").await;
    let path = dir
        .parent()
        .expect("a parent")
        .join(format!("{}-wt", dir.file_name().unwrap().to_string_lossy()));
    git.worktree_add(&dir, &path, "neohar/fix-bug", "main")
        .await
        .expect("a worktree");
    assert!(path.join("a.txt").exists());
    let listed = git.worktrees(&dir).await.expect("a list");
    assert_eq!(listed.len(), 2);
    let added = listed
        .iter()
        .find(|w| w.branch.as_deref() == Some("neohar/fix-bug"));
    assert!(added.is_some(), "{listed:?}");
    assert!(git.branch_exists(&dir, "neohar/fix-bug").await);
    // The same branch twice is an error.
    let again = git
        .worktree_add(&dir, &path.with_extension("two"), "neohar/fix-bug", "main")
        .await;
    assert!(matches!(again, Err(GitError::Failed { .. })), "{again:?}");
    git.worktree_remove(&dir, &path, false)
        .await
        .expect("a removal");
    assert!(!path.exists());
}

#[tokio::test]
async fn a_dirty_worktree_is_seen_and_a_removal_without_force_is_refused_by_git() {
    let (git, dir) = repo("dirty").await;
    let path = dir
        .parent()
        .expect("a parent")
        .join(format!("{}-wt", dir.file_name().unwrap().to_string_lossy()));
    git.worktree_add(&dir, &path, "neohar/dirty", "main")
        .await
        .expect("a worktree");
    assert!(!git.dirty(&path).await.expect("a status"));
    std::fs::write(path.join("new.txt"), "x\n").expect("a file");
    assert!(
        git.dirty(&path).await.expect("a status"),
        "an untracked file is a change"
    );
    assert!(git.worktree_remove(&dir, &path, false).await.is_err());
    git.worktree_remove(&dir, &path, true)
        .await
        .expect("a forced removal");
    assert!(!path.exists());
}

#[test]
fn the_porcelain_of_the_list_gives_paths_and_branches() {
    let text = "worktree /work/app\nHEAD abc\nbranch refs/heads/main\n\nworktree /work/app-wt\nHEAD def\ndetached\n";
    let listed = parse_worktrees(text);
    assert_eq!(
        listed,
        [
            Worktree {
                path: PathBuf::from("/work/app"),
                branch: Some("main".to_owned())
            },
            Worktree {
                path: PathBuf::from("/work/app-wt"),
                branch: None
            },
        ]
    );
}
