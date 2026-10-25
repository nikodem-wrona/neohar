//! The walk, the settings and the trusts, against repositories in a temporary directory with the
//! real `git`. A program of a hostile repository writes a marker file, which must not appear.
//! The trusts here have no state actor, so they live in memory. The store and the path through a
//! core are tested in `crates/nh/tests/changes.rs` and `git_view.rs`.

use std::{
    os::unix::fs::PermissionsExt,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::*;
use crate::changes::git::tests::{assert_empty, env, folder, markers, repo, script, write};

#[test]
fn the_settings_of_the_fingerprint_are_known() {
    for key in [
        "core.worktree",
        "submodule.recurse",
        "submodule.lib.fetchRecurseSubmodules",
        "fetch.recurseSubmodules",
        "push.recurseSubmodules",
        "hook.lint.command",
        "hook.lint.event",
        "hook.lint.enabled",
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
        assert!(in_fingerprint(key), "{key}");
    }
    for key in [
        "core.bare",
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
        assert!(!in_fingerprint(key), "{key}");
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

/// The keys that changed after the trust, or `None` when git may run.
async fn changed(trusts: &Trusts, dir: &Path) -> Option<Vec<String>> {
    match trusts.access(dir).await {
        Access::Open(_) => None,
        Access::Ask(ask) => {
            assert_eq!(ask.reason, GitTrustReason::Changed, "{ask:?}");
            Some(ask.changed)
        }
        Access::None => panic!("the repository is gone"),
    }
}

#[tokio::test]
async fn a_new_or_changed_hook_file_asks_again() {
    let (_git, dir) = repo("trust-hooks").await;
    let hooks = dir.join(".git/hooks");
    let trusts = Trusts::new(&env());
    trusts.trust(&dir).await.expect("a trust");
    assert_eq!(changed(&trusts, &dir).await, None);

    // An agent writes a hook that the next switch from a click would run.
    script(&hooks.join("post-checkout"), "echo one");
    assert_eq!(
        changed(&trusts, &dir).await,
        Some(vec!["hooks/post-checkout".to_owned()])
    );
    trusts.trust(&dir).await.expect("a trust again");
    assert_eq!(changed(&trusts, &dir).await, None);
    // Its bytes change.
    script(&hooks.join("post-checkout"), "echo two");
    assert_eq!(
        changed(&trusts, &dir).await,
        Some(vec!["hooks/post-checkout".to_owned()])
    );
    trusts.trust(&dir).await.expect("a trust again");

    // A file that git does not run changes nothing: not executable, a sample, a FIFO.
    std::fs::write(hooks.join("pre-commit"), "#!/bin/sh\n").expect("a file");
    script(&hooks.join("pre-push.sample"), "echo sample");
    let made = std::process::Command::new("mkfifo")
        .arg(hooks.join("post-merge"))
        .status()
        .expect("mkfifo runs");
    assert!(made.success());
    assert_eq!(changed(&trusts, &dir).await, None);
    // Its mode changes, and git can run it now.
    std::fs::set_permissions(
        hooks.join("pre-commit"),
        std::fs::Permissions::from_mode(0o755),
    )
    .expect("a mode");
    assert_eq!(
        changed(&trusts, &dir).await,
        Some(vec!["hooks/pre-commit".to_owned()])
    );
}

#[tokio::test]
async fn the_hooks_of_core_hooks_path_count_and_the_settings_of_writes_count() {
    let (git, dir) = repo("trust-hooks-path").await;
    git.run(&dir, &["config", "core.hooksPath", "tools/hooks"])
        .await
        .expect("config");
    let trusts = Trusts::new(&env());
    trusts.trust(&dir).await.expect("a trust");
    // A hook in the folder of `core.hooksPath`, from the root of the worktree. The hooks of
    // `.git/hooks` do not run now.
    std::fs::create_dir_all(dir.join("tools/hooks")).expect("a folder");
    script(&dir.join("tools/hooks/post-merge"), "echo merge");
    script(&dir.join(".git/hooks/post-merge"), "echo ignored");
    assert_eq!(
        changed(&trusts, &dir).await,
        Some(vec!["hooks/post-merge".to_owned()])
    );
    trusts.trust(&dir).await.expect("a trust again");
    // Settings that move the writes, start git in submodules, or add a hook of the settings.
    for (key, value) in [
        ("core.worktree", "/tmp"),
        ("submodule.recurse", "true"),
        ("fetch.recurseSubmodules", "true"),
        ("hook.lint.command", "echo lint"),
    ] {
        git.run(&dir, &["config", key, value])
            .await
            .expect("config");
        let keys = changed(&trusts, &dir).await.expect("a change");
        assert_eq!(keys, [key.to_ascii_lowercase()]);
        git.run(&dir, &["config", "--unset", key])
            .await
            .expect("config");
        assert_eq!(changed(&trusts, &dir).await, None, "{key}");
    }
}

#[test]
fn the_folder_of_the_hooks_is_the_one_that_git_uses() {
    let common = Path::new("/w/app/.git");
    let root = Path::new("/w/app");
    let home = Some(Path::new("/home/ada"));
    let dir = |out: &[u8]| hooks_dir(out, common, root, home);
    assert_eq!(dir(b""), Path::new("/w/app/.git/hooks"));
    assert_eq!(
        dir(b"global\0core.hookspath\n~/hooks\0"),
        Path::new("/home/ada/hooks")
    );
    assert_eq!(
        dir(b"global\0core.hookspath\n/etc/hooks\0local\0core.hookspath\ntools/hooks\0"),
        Path::new("/w/app/tools/hooks"),
        "the last one wins, and a relative path is from the root"
    );
    assert_eq!(
        dir(b"local\0core.hookspath\n/srv/hooks\0command\0core.hookspath\n/dev/null\0"),
        Path::new("/srv/hooks"),
        "the command line of neohar does not count"
    );
}
