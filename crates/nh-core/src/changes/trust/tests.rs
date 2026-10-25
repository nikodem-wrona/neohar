//! The walk, the settings and the trusts, against repositories in a temporary directory with the
//! real `git`. A program of a hostile repository writes a marker file, which must not appear.
//! The trusts here have no state actor, so they live in memory. The store and the path through a
//! core are tested in `crates/nh/tests/changes.rs` and `git_view.rs`.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::changes::git::tests::{assert_empty, env, folder, markers, repo, script, write};

#[test]
fn the_settings_that_name_a_program_are_known() {
    for key in [
        "core.fsmonitor",
        "core.hooksPath",
        "core.sshCommand",
        "core.gitProxy",
        "core.pager",
        "core.editor",
        "core.askPass",
        "core.alternateRefsCommand",
        "filter.lfs.clean",
        "filter.evil.process",
        "diff.external",
        "diff.pdf.textconv",
        "diff.tool.command",
        "merge.ours.driver",
        "remote.origin.uploadpack",
        "remote.a.b.receivepack",
        "remote.origin.vcs",
        "credential.helper",
        "credential.https://example.com.helper",
        "gpg.program",
        "gpg.ssh.program",
        "sequence.editor",
        "uploadpack.packObjectsHook",
        "include.path",
        "includeIf.gitdir:~/work/.path",
        "alias.st",
        "pager.log",
        "interactive.diffFilter",
        "difftool.vim.cmd",
        "mergetool.meld.path",
        "submodule.lib.update",
        "trailer.sign.command",
        "lfs.customtransfer.x.path",
        "lfs.extension.foo.clean",
    ] {
        assert!(names_a_program(key), "{key}");
    }
    for key in [
        "core.bare",
        "core.worktree",
        "remote.origin.url",
        "remote.origin.fetch",
        "branch.main.remote",
        "user.name",
        "lfs.url",
        "lfs.https://example.com/info/lfs.access",
        "diff.renames",
        "credential.username",
        "include",
    ] {
        assert!(!names_a_program(key), "{key}");
    }
}

#[test]
fn only_the_settings_of_the_repository_count() {
    let out = b"global\0core.pager\nless\0local\0core.bare\nfalse\0\
local\0core.fsmonitor\ntouch x\0worktree\0filter.a.clean\ncat\0\
command\0core.hookspath\n/dev/null\0unknown\0credential.helper\nosxkeychain\0local\0alias.x\0";
    assert_eq!(
        parse_settings(out),
        [
            ("core.fsmonitor".to_owned(), Some("touch x".to_owned())),
            ("filter.a.clean".to_owned(), Some("cat".to_owned())),
            ("alias.x".to_owned(), None),
        ]
    );
}

#[test]
fn a_change_names_each_key_whose_values_changed() {
    let setting = |key: &str, value: &str| (key.to_owned(), Some(value.to_owned()));
    let before = vec![
        setting("core.pager", "less"),
        setting("alias.a", "x"),
        setting("alias.a", "y"),
    ];
    let mut now = before.clone();
    assert!(changed_keys(&before, &now).is_empty());
    // The order of two keys does not count.
    now.swap(0, 1);
    assert!(changed_keys(&before, &now).is_empty());
    now.retain(|(key, value)| !(key == "alias.a" && value.as_deref() == Some("y")));
    now.push(setting("core.fsmonitor", "evil"));
    assert_eq!(changed_keys(&before, &now), ["alias.a", "core.fsmonitor"]);
}

#[tokio::test]
async fn the_walk_finds_the_worktree_and_the_common_directory_as_git_does() {
    let (git, dir) = repo("walk").await;
    std::fs::create_dir_all(dir.join("src/deep")).expect("a folder");
    let found = discover(&dir.join("src/deep")).expect("a repository");
    assert_eq!(
        found,
        Found {
            root: dir.clone(),
            git_dir: dir.join(".git"),
            common_dir: dir.join(".git"),
        }
    );
    // A linked worktree has a `.git` file, and shares the common directory.
    let linked = folder("walk-linked").join("topic");
    let path = linked.display().to_string();
    git.run(&dir, &["worktree", "add", "-q", "-b", "topic", &path])
        .await
        .expect("a worktree");
    let found = discover(&linked.join(".")).expect("a linked worktree");
    let place = git.place(&linked).await.expect("a place");
    assert_eq!(
        (found.root, found.git_dir, found.common_dir.clone()),
        (place.root, place.git_dir, place.common_dir)
    );
    assert_eq!(found.common_dir, dir.join(".git"));
    assert_eq!(discover(&folder("walk-outside")), None);
}

#[tokio::test]
async fn the_walk_stops_at_a_bare_repository_and_at_a_broken_git_file() {
    let (_git, dir) = repo("walk-bare").await;
    write(&dir, "evil/HEAD", b"ref: refs/heads/main\n");
    write(&dir, "evil/objects/.keep", b"");
    write(&dir, "evil/refs/heads/.keep", b"");
    std::fs::create_dir_all(dir.join("evil/sub")).expect("a folder");
    assert_eq!(discover(&dir.join("evil/sub")), None, "git refuses it");
    assert_eq!(discover(&dir.join("evil")), None);
    write(&dir, "broken/.git", b"gitdir: /nowhere\n");
    assert_eq!(discover(&dir.join("broken")), None, "git stops there");
    // An empty `.git` folder is no repository: the walk goes on up, as git does.
    std::fs::create_dir_all(dir.join("plain/.git")).expect("a folder");
    let found = discover(&dir.join("plain")).expect("the repository above");
    assert_eq!(found.root, dir);
}

/// A repository whose settings run programs: an fsmonitor, a clean filter for every file, and an
/// alias. A file has a change, so a count would run the filter.
async fn hostile(name: &str) -> (Git, PathBuf, PathBuf) {
    let (git, dir) = repo(name).await;
    let marks = markers(name);
    let fsmonitor = format!("touch {}/fsmonitor; false", marks.display());
    let clean = format!("touch {}/clean; cat", marks.display());
    let alias = format!("!touch {}/alias", marks.display());
    for (key, value) in [
        ("core.fsmonitor", fsmonitor.as_str()),
        ("filter.evil.clean", clean.as_str()),
        ("alias.status", alias.as_str()),
    ] {
        git.run(&dir, &["config", key, value])
            .await
            .expect("config");
    }
    write(&dir, ".gitattributes", b"* filter=evil\n");
    write(&dir, "a.txt", b"one\ntwo\nthree\n");
    (git, dir, marks)
}

#[tokio::test]
async fn git_runs_nothing_in_a_repository_until_the_person_trusts_it() {
    let (_git, dir, marks) = hostile("trust-hostile").await;
    let trusts = Trusts::new(&env());
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let common = dir.join(".git");
    let expected = common.clone();
    trusts.on_change(Box::new(move |changed| {
        assert_eq!(changed, expected);
        counted.fetch_add(1, Ordering::Relaxed);
    }));

    let Access::Ask(ask) = trusts.access(&dir.join(".")).await else {
        panic!("a repository that nobody trusted asks");
    };
    assert_eq!(
        (ask.path.as_str(), ask.reason, ask.changed.len()),
        (dir.to_str().expect("a path"), GitTrustReason::New, 0)
    );
    assert_eq!(ask.common_dir, common.display().to_string());
    assert_empty(&marks);

    // The trust reads the settings with `git config`, which runs none of them.
    let trusted = trusts.trust(&dir).await.expect("a trust");
    assert_eq!(trusted.common_dir, common.display().to_string());
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let Access::Open(place) = trusts.access(&dir).await else {
        panic!("a trusted repository is open");
    };
    assert_eq!(place.root, dir);
    assert_empty(&marks);

    // An agent writes a setting that runs a program: the core asks again.
    let edited = std::fs::read_to_string(common.join("config")).expect("the config");
    let more = format!(
        "{edited}[core]\n\tsshCommand = touch {}/ssh\n",
        marks.display()
    );
    std::fs::write(common.join("config"), more).expect("a change");
    let refusal = trusts
        .gate(&common, &dir)
        .await
        .expect("the settings are read")
        .expect("a refusal");
    assert_eq!(
        (refusal.reason, refusal.changed),
        (GitTrustReason::Changed, vec!["core.sshcommand".to_owned()])
    );
    let Access::Ask(ask) = trusts.access(&dir).await else {
        panic!("a change of the settings asks");
    };
    assert_eq!(ask.reason, GitTrustReason::Changed);
    // A setting that runs no program changes nothing.
    std::fs::write(
        common.join("config"),
        format!(
            "{}[user]\n\tname = Ada\n",
            std::fs::read_to_string(common.join("config")).expect("the config")
        ),
    )
    .expect("a change");
    trusts.trust(&dir).await.expect("a trust again");
    assert!(matches!(trusts.access(&dir).await, Access::Open(_)));
    std::fs::write(
        common.join("config"),
        format!(
            "{}[branch \"main\"]\n\tremote = origin\n",
            std::fs::read_to_string(common.join("config")).expect("the config")
        ),
    )
    .expect("a change");
    assert!(matches!(trusts.access(&dir).await, Access::Open(_)));

    // The end of the trust asks again.
    trusts.untrust(&dir).await.expect("no trust");
    let Access::Ask(ask) = trusts.access(&dir).await else {
        panic!("the end of a trust asks");
    };
    assert_eq!(ask.reason, GitTrustReason::New);
    assert_eq!(calls.load(Ordering::Relaxed), 3);
    assert_empty(&marks);
}

#[tokio::test]
async fn the_worktrees_of_a_repository_share_its_trust() {
    let (git, dir) = repo("trust-worktrees").await;
    let linked = folder("trust-worktrees-linked").join("topic");
    let path = linked.display().to_string();
    git.run(&dir, &["worktree", "add", "-q", "-b", "topic", &path])
        .await
        .expect("a worktree");
    let trusts = Trusts::new(&env());
    assert!(matches!(trusts.access(&linked).await, Access::Ask(_)));
    trusts.trust(&linked).await.expect("a trust");
    assert!(matches!(trusts.access(&dir).await, Access::Open(_)));
    let Access::Open(place) = trusts.access(&linked).await else {
        panic!("the linked worktree is open too");
    };
    assert_eq!(place.root, std::fs::canonicalize(&linked).expect("a path"));
}

#[tokio::test]
async fn an_included_file_of_the_worktree_counts_as_a_setting_of_the_repository() {
    let (git, dir) = repo("trust-include").await;
    write(&dir, "shared.gitconfig", b"[user]\n\tname = Ada\n");
    git.run(&dir, &["config", "include.path", "../shared.gitconfig"])
        .await
        .expect("config");
    let trusts = Trusts::new(&env());
    trusts.trust(&dir).await.expect("a trust");
    assert!(matches!(trusts.access(&dir).await, Access::Open(_)));
    // A file of the worktree that an agent can change.
    write(&dir, "shared.gitconfig", b"[core]\n\tpager = evil\n");
    let Access::Ask(ask) = trusts.access(&dir).await else {
        panic!("a change of an included file asks");
    };
    assert_eq!(ask.changed, ["core.pager"]);
}

#[tokio::test]
async fn a_trust_and_its_end_need_a_repository() {
    let trusts = Trusts::new(&env());
    let outside = folder("trust-outside");
    for result in [trusts.trust(&outside).await, trusts.untrust(&outside).await] {
        let error = result.expect_err("no repository");
        assert_eq!(error.code, ErrorCode::NotFound, "{}", error.message);
    }
    let error = untrusted(&GitTrustAsk {
        path: "/work/app".to_owned(),
        common_dir: "/work/app/.git".to_owned(),
        reason: GitTrustReason::Changed,
        changed: vec!["core.fsmonitor".to_owned()],
    });
    assert_eq!(error.code, ErrorCode::PermissionDenied);
    assert_eq!(error.details["kind"], "untrusted");
    assert_eq!(error.details["trust"]["changed"][0], "core.fsmonitor");
}

#[tokio::test]
async fn a_git_of_the_repository_never_runs_for_the_settings() {
    let (_git, dir) = repo("trust-relative").await;
    let marks = markers("trust-relative");
    std::fs::create_dir_all(dir.join("bin")).expect("a folder");
    script(
        &dir.join("bin/git"),
        &format!("touch {}/fake; exit 0", marks.display()),
    );
    let mut env = env();
    for (key, value) in &mut env {
        if key == "PATH" {
            *value = format!("bin:{value}");
        }
    }
    let trusts = Trusts::new(&env);
    trusts.trust(&dir).await.expect("a trust");
    assert!(matches!(trusts.access(&dir).await, Access::Open(_)));
    assert_empty(&marks);
}
