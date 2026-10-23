//! The parsers of the git output and the rows that the Git view makes from them. The whole path
//! through a core and a real repository is in `crates/nh/tests/git_view.rs`.

use nh_proto::api::{common::Harness, ids::PaneId};

use super::{read::*, *};

fn branch(name: &str, kind: Kind, time: i64) -> Branch {
    let reference = match kind {
        Kind::Local => format!("refs/heads/{name}"),
        Kind::Remote => format!("refs/remotes/{name}"),
    };
    Branch {
        reference,
        name: name.to_owned(),
        kind,
        commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        time,
        subject: format!("work on {name}"),
        ..Branch::default()
    }
}

#[test]
fn a_branch_line_has_its_upstream_its_track_and_its_worktree() {
    let out = b"refs/heads/main\0aaaa\x001790000000\0origin/main\0origin\0ahead 1, behind 2\0/w/app\0*\0\0fix the build\n\
refs/heads/topic\0bbbb\x001780000000\0\0\0\0\0 \0\0start\n\
refs/remotes/origin/HEAD\0aaaa\x001790000000\0\0\0\0\0 \0refs/remotes/origin/main\0fix the build\n\
refs/remotes/origin/main\0aaaa\x001790000000\0\0\0\0\0 \0\0a subject with \0 a NUL\n";
    let branches = parse_branches(out);
    let names: Vec<&str> = branches.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(
        names,
        ["main", "topic", "origin/main"],
        "origin/HEAD is left out"
    );
    let main = &branches[0];
    assert_eq!(
        (main.kind, main.ahead, main.behind, main.gone, main.head),
        (Kind::Local, 1, 2, false, true)
    );
    assert_eq!(main.upstream.as_deref(), Some("origin/main"));
    assert_eq!(main.remote.as_deref(), Some("origin"));
    assert_eq!(main.time, 1_790_000_000);
    assert!(main.worktree.is_some());
    assert!(!branches[1].head && branches[1].upstream.is_none());
    assert_eq!(branches[2].kind, Kind::Remote);
    assert_eq!(branches[2].subject, "a subject with \0 a NUL");
}

#[test]
fn the_track_text_gives_ahead_behind_and_gone() {
    assert_eq!(parse_track("ahead 3"), (3, 0, false));
    assert_eq!(parse_track("behind 4"), (0, 4, false));
    assert_eq!(parse_track("ahead 1, behind 12"), (1, 12, false));
    assert_eq!(parse_track("gone"), (0, 0, true));
    assert_eq!(parse_track(""), (0, 0, false));
}

#[test]
fn the_recent_branches_are_those_of_the_checkouts_of_the_reflog_each_once() {
    let reflog = "checkout: moving from topic to main\n\
commit: fix the build\n\
checkout: moving from main to topic\n\
checkout: moving from feature/a to main\n\
reset: moving to HEAD~1\n";
    assert_eq!(parse_recent(reflog), ["main", "topic", "feature/a"]);
}

#[test]
fn a_worktree_list_has_the_main_worktree_first_with_each_field() {
    let out = b"worktree /w/app\0HEAD 1111111111111111111111111111111111111111\0branch refs/heads/main\0\0\
worktree /w/app-topic\0HEAD 2222222222222222222222222222222222222222\0detached\0locked reason\0\0\
worktree /w/gone\0HEAD 3333333333333333333333333333333333333333\0branch refs/heads/old\0prunable gitdir file points to non-existent location\0\0";
    let list = parse_worktrees(out);
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].path, PathBuf::from("/w/app"));
    assert_eq!(list[0].branch.as_deref(), Some("refs/heads/main"));
    assert!(list[1].branch.is_none() && list[1].locked && !list[1].prunable);
    assert!(list[2].prunable);
    let bare = parse_worktrees(b"worktree /w/app.git\0bare\0\0");
    assert!(bare[0].bare);
}

#[test]
fn a_status_counts_a_rename_once() {
    let out = b" M a.txt\0R  new.txt\0old.txt\0?? notes.md\0";
    assert_eq!(parse_status_count(out), 3);
    assert_eq!(parse_status_count(b""), 0);
}

#[test]
fn a_commit_has_its_refs_and_its_body() {
    let out = b"aaaa1111\0aaaa111\0bbbb2222 cccc3333\0Ada\0ada@example.com\x001790000000\0HEAD -> main, origin/main, origin/HEAD, tag: v1.0\0Merge topic\0Two lines\nof body\n\n\x1e\n\
bbbb2222\0bbbb222\0\0Bo\0bo@example.com\x001780000000\0\0first\0\x1e\n";
    let commits = parse_commits(out);
    assert_eq!(commits.len(), 2);
    let merge = &commits[0];
    assert_eq!(merge.parents, ["bbbb2222", "cccc3333"]);
    assert!(merge.head);
    assert_eq!(merge.branches, ["main", "origin/main"]);
    assert_eq!(merge.tags, ["v1.0"]);
    assert_eq!(merge.body, "Two lines\nof body");
    assert_eq!(merge.time, 1_790_000_000);
    let first = &commits[1];
    assert!(first.parents.is_empty() && !first.head && first.branches.is_empty());
    assert_eq!((first.subject.as_str(), first.body.as_str()), ("first", ""));
}

#[test]
fn a_detached_head_is_a_ref_without_a_branch() {
    assert_eq!(parse_refs("HEAD"), (true, Vec::new(), Vec::new()));
    assert_eq!(
        parse_refs("origin/topic"),
        (false, vec!["origin/topic".to_owned()], Vec::new())
    );
}

#[test]
fn only_hex_digits_are_a_hash() {
    assert!(is_hash("abc123") && is_hash("ABCD"));
    assert!(!is_hash("abc"), "too short");
    assert!(!is_hash("--output=/tmp/x"));
    assert!(!is_hash("HEAD~1"));
    assert!(!is_hash(&"a".repeat(65)));
}

#[test]
fn the_time_text_counts_the_largest_whole_unit() {
    let now = 1_800_000_000_000;
    assert_eq!(ago(now, now), "just now");
    assert_eq!(ago(now, now + 5_000), "just now", "a time in the future");
    assert_eq!(ago(now, now - 60_000), "1 minute ago");
    assert_eq!(ago(now, now - 18 * 60_000), "18 minutes ago");
    assert_eq!(ago(now, now - 3 * 3_600_000), "3 hours ago");
    assert_eq!(ago(now, now - 86_400_000), "1 day ago");
    assert_eq!(ago(now, now - 40 * 86_400_000), "1 month ago");
    assert_eq!(ago(now, now - 800 * 86_400_000), "2 years ago");
}

#[test]
fn the_name_of_a_repository_is_its_folder() {
    assert_eq!(repo_name(Path::new("/w/neohar/.git")), "neohar");
    assert_eq!(repo_name(Path::new("/srv/neohar.git")), "neohar");
    assert_eq!(repo_name(Path::new("/srv/plain")), "plain");
}

#[test]
fn a_fetch_reads_the_remote_of_the_upstream_else_origin() {
    let remotes = vec!["fork".to_owned(), "origin".to_owned()];
    assert_eq!(
        read::fetch_remote(Some("fork"), &remotes).as_deref(),
        Some("fork")
    );
    assert_eq!(
        read::fetch_remote(None, &remotes).as_deref(),
        Some("origin")
    );
    assert_eq!(
        read::fetch_remote(None, &["up".to_owned()]).as_deref(),
        Some("up")
    );
    assert_eq!(read::fetch_remote(None, &[]), None);
}

fn look() -> Look {
    let mut main = branch("main", Kind::Local, 300);
    main.head = true;
    main.worktree = Some(PathBuf::from("/w/app"));
    let mut topic = branch("topic", Kind::Local, 100);
    topic.worktree = Some(PathBuf::from("/w/app-topic"));
    Look {
        commit: Some("0123456".to_owned()),
        branches: vec![
            main,
            branch("origin/main", Kind::Remote, 300),
            branch("newer", Kind::Local, 250),
            branch("old", Kind::Local, 50),
            topic,
        ],
        recent: vec!["main".to_owned(), "old".to_owned(), "gone".to_owned()],
        remotes: vec!["origin".to_owned()],
        worktrees: vec![
            Worktree {
                path: PathBuf::from("/w/app"),
                branch: Some("refs/heads/main".to_owned()),
                commit: Some("0123456789".to_owned()),
                ..Worktree::default()
            },
            Worktree {
                path: PathBuf::from("/w/app-topic"),
                branch: Some("refs/heads/topic".to_owned()),
                ..Worktree::default()
            },
            Worktree {
                path: PathBuf::from("/w/app/.claude/worktrees/x"),
                ..Worktree::default()
            },
        ],
        dirty: HashMap::from([(PathBuf::from("/w/app-topic"), 2)]),
        fetched_at: None,
        stashes: Vec::new(),
    }
}

#[test]
fn the_branches_come_current_then_recent_then_local_then_remote() {
    let rows = branch_rows(Path::new("/w/app"), &look(), 1_000_000);
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, ["main", "old", "newer", "topic", "origin/main"]);
    assert!(rows[0].current && !rows[0].recent);
    assert!(rows[1].recent);
    assert_eq!(rows[0].worktree, None, "the worktree of the view");
    assert_eq!(rows[3].worktree.as_deref(), Some("/w/app-topic"));
    assert_eq!(rows[4].kind, nh_proto::api::model::GitBranchKind::Remote);
    assert_eq!(rows[0].time_text.as_deref(), Some("11 minutes ago"));
}

fn placed(folder: &str, pane: &str, state: GitAgentState) -> Placed {
    Placed {
        folder: PathBuf::from(folder),
        agent: GitAgent {
            run: "01M4F1W7EM2GN3EV6NHXZJEGGA".parse().expect("an id"),
            pane: pane.parse::<PaneId>().expect("an id"),
            harness: Harness::Claude,
            name: None,
            state,
        },
    }
}

#[test]
fn an_agent_is_in_the_innermost_worktree_that_holds_its_folder() {
    let agents = [
        placed(
            "/w/app/src",
            "01M4F1W7EM2GN3EV6NHXZJEGGB",
            GitAgentState::Working,
        ),
        placed(
            "/w/app/.claude/worktrees/x",
            "01M4F1W7EM2GN3EV6NHXZJEGGC",
            GitAgentState::Idle,
        ),
        placed(
            "/w/elsewhere",
            "01M4F1W7EM2GN3EV6NHXZJEGGD",
            GitAgentState::Idle,
        ),
    ];
    let rows = worktree_rows(Path::new("/w/app-topic"), &look(), &agents);
    assert_eq!(rows.len(), 3);
    assert!(rows[0].main && !rows[0].current);
    assert_eq!(rows[0].branch.as_deref(), Some("main"));
    assert_eq!(rows[0].commit.as_deref(), Some("0123456"));
    assert_eq!(rows[0].agents.len(), 1);
    assert_eq!(rows[0].agents[0].state, GitAgentState::Working);
    assert!(rows[1].current && rows[1].agents.is_empty());
    assert_eq!(rows[1].dirty, Some(2));
    assert_eq!(
        rows[2].agents.len(),
        1,
        "the nested worktree has its own agent"
    );
    assert!(rows[2].branch.is_none());
}

#[test]
fn the_head_has_the_branch_its_upstream_and_the_fetch_text() {
    let mut look = look();
    look.branches[0].upstream = Some("origin/main".to_owned());
    look.branches[0].remote = Some("origin".to_owned());
    look.branches[0].behind = 3;
    let place = Place {
        root: PathBuf::from("/w/app"),
        git_dir: PathBuf::from("/w/app/.git"),
        common_dir: PathBuf::from("/w/app/.git"),
    };
    let now = 10_000_000;
    let head = head_value(
        Path::new("/w/app/src"),
        Some(&place),
        Some(&look),
        &Count::default(),
        None,
        Live::default(),
        now,
    );
    assert_eq!(head.branch.as_deref(), Some("main"));
    assert_eq!(head.repo_name.as_deref(), Some("app"));
    assert_eq!(head.worktree_name.as_deref(), Some("app"));
    assert_eq!((head.ahead, head.behind), (0, 3));
    assert_eq!(head.remote.as_deref(), Some("origin"));
    assert_eq!(head.fetched_text.as_deref(), Some("Never fetched"));
    look.fetched_at = Some(now - 18 * 60_000);
    let head = head_value(
        Path::new("/w/app"),
        Some(&place),
        Some(&look),
        &Count::default(),
        None,
        Live::default(),
        now,
    );
    assert_eq!(
        head.fetched_text.as_deref(),
        Some("Last fetched 18 minutes ago")
    );
    look.remotes.clear();
    let head = head_value(
        Path::new("/w/app"),
        Some(&place),
        Some(&look),
        &Count::default(),
        None,
        Live::default(),
        now,
    );
    assert_eq!((head.remote, head.fetched_text), (None, None));
    let outside = head_value(
        Path::new("/tmp"),
        None,
        None,
        &Count::default(),
        None,
        Live::default(),
        now,
    );
    assert!(outside.worktree.is_none() && outside.branch.is_none());
}

#[test]
fn a_repository_takes_its_first_source_and_counts_its_live_agents() {
    let found = |common: &str, root: &str| {
        Some(Found {
            common_dir: PathBuf::from(common),
            root: PathBuf::from(root),
        })
    };
    let candidates = vec![
        (PathBuf::from("/w/lib"), GitRepoSource::Workspace),
        (PathBuf::from("/w/app/src"), GitRepoSource::Pane),
        (PathBuf::from("/w/app-topic"), GitRepoSource::Pane),
        (PathBuf::from("/tmp"), GitRepoSource::Pane),
        (PathBuf::from("/srv/bare.git"), GitRepoSource::Added),
    ];
    let known = HashMap::from([
        (PathBuf::from("/w/lib"), found("/w/lib/.git", "/w/lib")),
        (PathBuf::from("/w/app/src"), found("/w/app/.git", "/w/app")),
        (
            PathBuf::from("/w/app-topic"),
            found("/w/app/.git", "/w/app-topic"),
        ),
        (PathBuf::from("/tmp"), None),
        (
            PathBuf::from("/srv/bare.git"),
            found("/srv/bare.git", "/srv/bare-work"),
        ),
    ]);
    let agents = [
        placed(
            "/w/app-topic",
            "01M4F1W7EM2GN3EV6NHXZJEGGB",
            GitAgentState::Working,
        ),
        placed(
            "/w/app/src",
            "01M4F1W7EM2GN3EV6NHXZJEGGC",
            GitAgentState::Exited,
        ),
    ];
    let rows = repo_rows_of(&candidates, &known, &agents);
    let names: Vec<(&str, u32)> = rows.iter().map(|r| (r.name.as_str(), r.agents)).collect();
    assert_eq!(names, [("app", 1), ("bare", 0), ("lib", 0)]);
    assert_eq!(rows[0].source, GitRepoSource::Pane);
    assert_eq!(rows[0].root, "/w/app", "the main worktree");
    assert_eq!(rows[1].root, "/srv/bare-work", "a bare repository");
    assert_eq!(rows[2].source, GitRepoSource::Workspace);
}

#[test]
fn a_file_of_a_commit_is_a_row_without_a_stamp() {
    let record = Record {
        path: b"src/new.rs".to_vec(),
        old_path: Some(b"src/old.rs".to_vec()),
        status: b'R',
        lines: Some((4, 1)),
        ..Record::default()
    };
    let row = commit_file_row(&record);
    assert_eq!(
        (row.dir.as_str(), row.name.as_str(), row.add, row.del),
        ("src/", "new.rs", 4, 1)
    );
    assert_eq!(row.status, nh_proto::api::model::ChangeStatus::Renamed);
    assert_eq!(row.old_path.as_deref(), Some("src/old.rs"));
    assert_eq!(row.stamp, 0);
    assert_eq!(file_status(&record), rich::FileStatus::Renamed);
}

#[test]
fn a_switch_plans_from_the_branches_of_the_look() {
    use super::write::{Plan, plan};
    let mut look = look();
    look.remotes = vec!["origin".to_owned(), "up/stream".to_owned()];
    look.branches
        .push(branch("origin/feat/x", Kind::Remote, 10));
    look.branches
        .push(branch("up/stream/fix", Kind::Remote, 10));
    let root = Path::new("/w/app");
    assert_eq!(plan(&look, root, "main"), Some(Plan::Stay));
    assert_eq!(
        plan(&look, root, "refs/heads/newer"),
        Some(Plan::Local("newer".to_owned()))
    );
    assert_eq!(
        plan(&look, root, "topic"),
        Some(Plan::Elsewhere {
            name: "topic".to_owned(),
            worktree: PathBuf::from("/w/app-topic"),
        })
    );
    // A remote branch with a local branch of the same name goes to the local one.
    assert_eq!(plan(&look, root, "origin/main"), Some(Plan::Stay));
    assert_eq!(
        plan(&look, root, "refs/remotes/origin/feat/x"),
        Some(Plan::Track {
            name: "feat/x".to_owned(),
            remote: "origin/feat/x".to_owned(),
        })
    );
    assert_eq!(
        plan(&look, root, "up/stream/fix"),
        Some(Plan::Track {
            name: "fix".to_owned(),
            remote: "up/stream/fix".to_owned(),
        }),
        "the longest remote that matches"
    );
    assert_eq!(plan(&look, root, "nothing"), None);
}

#[test]
fn the_guard_names_the_agent_and_what_it_does() {
    use super::write::guard_text;
    let working = placed(
        "/w/app",
        "01M4F1W7EM2GN3EV6NHXZJEGGB",
        GitAgentState::Working,
    )
    .agent;
    let waiting = placed(
        "/w/app",
        "01M4F1W7EM2GN3EV6NHXZJEGGC",
        GitAgentState::Waiting,
    )
    .agent;
    assert_eq!(
        guard_text(&[&working], "app-topic", "switch"),
        "Claude Code is working in app-topic. A switch changes its files."
    );
    assert_eq!(
        guard_text(&[&waiting], "app", "pull"),
        "Claude Code waits for you in app. A pull changes its files."
    );
    assert_eq!(
        guard_text(&[&working, &waiting], "app", "switch"),
        "2 agents work in app. A switch changes their files."
    );
    assert!(agents::busy(GitAgentState::Unknown) && agents::busy(GitAgentState::Starting));
    assert!(!agents::busy(GitAgentState::Idle) && !agents::busy(GitAgentState::Exited));
}

#[test]
fn a_fetch_in_the_background_that_needs_a_secret_says_to_click_fetch() {
    use super::write::fetch_note;
    let asked =
        "fatal: could not read Username for 'https://example.com': terminal prompts disabled";
    assert_eq!(
        fetch_note(asked, true),
        "The fetch needs your passphrase or a token. Click Fetch."
    );
    assert_eq!(
        fetch_note(asked, false),
        format!("The fetch failed: {asked}")
    );
    assert_eq!(
        fetch_note("\nfatal: no route to host\n", true),
        "The fetch failed: fatal: no route to host"
    );
}

#[test]
fn the_stash_of_a_branch_is_the_newest_with_its_message() {
    let out = b"aaa\0stash@{0}\x001790000300\0On topic: neohar: topic\n\
bbb\0stash@{1}\x001790000200\0On main: neohar: main\n\
ccc\0stash@{2}\x001790000100\0On main: neohar: main\n\
ddd\0stash@{3}\x001790000000\0WIP on main: 0123456 first\n";
    let stashes = parse_stashes(out);
    assert_eq!(stashes.len(), 4);
    let found = stash_of(&stashes, "main").expect("a stash");
    assert_eq!(
        (found.commit.as_str(), found.name.as_str()),
        ("bbb", "stash@{1}")
    );
    assert_eq!(found.time, 1_790_000_200);
    assert!(stash_of(&stashes, "ma").is_none(), "the whole name");
    assert!(stash_of(&stashes, "other").is_none());
}
