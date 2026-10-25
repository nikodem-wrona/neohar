//! The walk on real processes. The tests start `sh` and `sleep`, which both macOS and
//! Linux have, and read their trees.

use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use super::*;

/// A child that is killed and waited for at the end of the test.
struct Guard(Child);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn(script: &str) -> Guard {
    Guard(
        Command::new("/bin/sh")
            .args(["-c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("a shell"),
    )
}

fn pid(child: &Guard) -> i32 {
    i32::try_from(child.0.id()).expect("a pid")
}

/// Walks until `done` says yes. A shell needs a moment to start its children.
fn walk_until(walker: &mut Walker, leader: i32, done: impl Fn(&Tree) -> bool) -> Tree {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let tree = walker.walk(leader);
        if done(&tree) || Instant::now() > deadline {
            return tree;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn has_sleep(tree: &Tree) -> bool {
    tree.processes
        .iter()
        .any(|process| process.exe_name() == Some("sleep"))
}

#[test]
fn the_tree_has_the_leader_first_and_its_descendants_after_it() {
    // The shell runs `sleep` and then `true`, so it forks `sleep` instead of replacing itself.
    let child = spawn("sleep 30; true");
    let leader = pid(&child);
    let mut walker = Walker::new(false);
    let tree = walk_until(&mut walker, leader, has_sleep);
    assert!(tree.processes.len() >= 2, "{tree:?}");
    let first = &tree.processes[0];
    assert_eq!(first.pid, leader);
    assert_eq!(first.argv.first().map(String::as_str), Some("/bin/sh"));
    assert!(first.exe.is_some());
    let sleep = tree
        .processes
        .iter()
        .find(|process| process.exe_name() == Some("sleep"))
        .expect("sleep");
    assert_eq!(sleep.ppid, leader);
    assert_eq!(sleep.argv, ["sleep", "30"]);
    assert!(sleep.env.is_none(), "the environment was not asked for");
}

#[test]
fn the_environment_is_read_only_when_asked() {
    // macOS does not show the environment of a program that Apple signed (`/bin/sh`), and it
    // kills a copy of one. The test process is a program of the user, like `claude`, so the walk
    // reads its own environment.
    let me = i32::try_from(std::process::id()).expect("a pid");
    let tree = Walker::new(true).walk(me);
    let env = tree.processes[0].env.as_ref().expect("an environment");
    assert!(env.contains_key("PATH"), "{env:?}");
    let tree = Walker::new(false).walk(me);
    assert!(
        tree.processes[0].env.is_none(),
        "the environment was not asked for"
    );
}

#[test]
fn a_process_that_did_not_change_is_read_once() {
    let child = spawn("sleep 30; true");
    let leader = pid(&child);
    let mut walker = Walker::new(false);
    let tree = walk_until(&mut walker, leader, has_sleep);
    let count = u64::try_from(tree.processes.len()).expect("a count");
    let before = walker.stats().args_reads;
    for _ in 0..5 {
        assert_eq!(walker.walk(leader).processes.len(), tree.processes.len());
    }
    assert_eq!(
        walker.stats().args_reads,
        before,
        "the cache served the walks"
    );
    assert!(before >= count);
}

#[test]
fn a_process_that_runs_another_program_is_read_again() {
    // The shell sleeps and then replaces itself with `sleep 31`: the pid and the start time stay,
    // and the program changes.
    let child = spawn("sleep 0.3; exec sleep 31");
    let leader = pid(&child);
    let mut walker = Walker::new(false);
    let first = walker.walk(leader);
    assert_eq!(
        first.processes[0].argv.first().map(String::as_str),
        Some("/bin/sh")
    );
    let tree = walk_until(&mut walker, leader, |tree| {
        tree.processes
            .first()
            .is_some_and(|process| process.exe_name() == Some("sleep"))
    });
    assert_eq!(tree.processes[0].argv, ["sleep", "31"]);
    assert_eq!(tree.processes[0].pid, leader);
}

#[test]
fn a_process_that_is_gone_is_not_alive_and_gives_an_empty_tree() {
    let mut child = spawn("sleep 30; true");
    let leader = pid(&child);
    let mut walker = Walker::new(false);
    let tree = walker.walk(leader);
    let key = (leader, tree.processes[0].start);
    assert!(walker.alive(key));
    // The same pid with another start time is another process.
    assert!(!walker.alive((leader, key.1 + 5_000)));
    child.0.kill().expect("a kill");
    child.0.wait().expect("a wait");
    assert!(!walker.alive(key));
    assert!(walker.walk(leader).is_empty());
    assert!(
        walker.cache.is_empty(),
        "the cache holds no process that is gone"
    );
}

#[test]
fn the_walk_stops_at_depth_6() {
    // A chain of shells: each runs the next as a child (`; true` keeps it from replacing itself).
    let mut script = "sleep 30; true".to_owned();
    for _ in 0..9 {
        script = format!("/bin/sh -c '{}'; true", script.replace('\'', "'\\''"));
    }
    let child = spawn(&script);
    let leader = pid(&child);
    let mut walker = Walker::new(false);
    let tree = walk_until(&mut walker, leader, |tree| tree.processes.len() > MAX_DEPTH);
    // The leader is at depth 0, so the walk reads 7 processes at most on a chain.
    std::thread::sleep(Duration::from_millis(300));
    let tree = walker
        .walk(leader)
        .processes
        .len()
        .max(tree.processes.len());
    assert_eq!(tree, MAX_DEPTH + 1);
}

#[test]
fn the_group_of_a_process_is_known() {
    let child = spawn("sleep 30; true");
    let leader = pid(&child);
    let group = group_of(leader).expect("a group");
    // A child of the test runner is in the group of the runner, which is not the pid of the child.
    assert!(group > 0);
    assert_eq!(group_of(i32::MAX), None);
}

#[test]
fn the_ancestors_of_a_process_go_up_to_this_process_and_beyond() {
    // `sh` starts `sleep`: the parents of `sleep` are `sh`, this test process, and its parents.
    let child = spawn("sleep 30; true");
    let shell = pid(&child);
    let me = i32::try_from(std::process::id()).expect("a pid");
    let tree = walk_until(&mut Walker::new(false), shell, has_sleep);
    let sleep = tree
        .processes
        .iter()
        .find(|process| process.exe_name() == Some("sleep"))
        .map(|process| process.pid)
        .expect("sleep runs");
    let up = ancestors(sleep);
    assert_eq!(up.first(), Some(&shell), "{up:?}");
    assert!(up.contains(&me), "{up:?}");
    assert!(!up.contains(&1), "launchd and init are not in the list");
    assert!(ancestors(i32::MAX).is_empty());
    // The child is in the session of this process: it did not start a session of its own.
    assert_eq!(session_of(sleep), session_of(me));
    assert!(session_of(me).is_some());
    assert_eq!(session_of(i32::MAX), None);
}

#[test]
fn the_working_directory_follows_a_cd() {
    // The folder of the system for temporary files has a link in its path on macOS (`/var` is
    // `/private/var`). The system gives the real path.
    let folder = std::env::temp_dir()
        .canonicalize()
        .expect("the temporary folder");
    let child = spawn(&format!(
        "cd '{}' && sleep 30; true",
        folder.display().to_string().replace('\'', "'\\''")
    ));
    let leader = pid(&child);
    let deadline = Instant::now() + Duration::from_secs(10);
    while cwd(leader).as_ref() != Some(&folder) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(cwd(leader), Some(folder));
    assert_eq!(cwd(i32::MAX), None);
}

fn kin(pid: i32, ppid: i32, sid: i32) -> Kin {
    Kin { pid, ppid, sid }
}

#[test]
fn the_family_holds_the_session_and_every_descendant() {
    // 100 leads the session. 101 is a job in the background whose parent ended, so `launchd` (1)
    // took it. 102 left the session, and its child 103 is in the session of 102. 200 is another
    // pane, and 300 is a process of the system.
    let table = [
        kin(1, 0, 1),
        kin(100, 50, 100),
        kin(101, 1, 100),
        kin(102, 100, 102),
        kin(103, 102, 102),
        kin(104, 101, 100),
        kin(200, 50, 200),
        kin(201, 200, 200),
        kin(300, 1, 300),
    ];
    let mut family = family(&table, 100);
    family.sort_unstable();
    assert_eq!(family, [100, 101, 102, 103, 104]);
}

#[test]
fn the_family_of_a_leader_that_ended_is_its_session() {
    let table = [kin(1, 0, 1), kin(101, 1, 100), kin(105, 101, 105)];
    let mut family = family(&table, 100);
    family.sort_unstable();
    assert_eq!(family, [101, 105]);
    assert!(super::family(&table, 999).is_empty());
}

#[test]
fn the_table_has_this_process_and_its_children() {
    let child = spawn("sleep 30; true");
    let shell = pid(&child);
    let deadline = Instant::now() + Duration::from_secs(10);
    let found = loop {
        let table = table();
        let me = i32::try_from(std::process::id()).expect("a pid");
        assert!(
            table.iter().any(|kin| kin.pid == me),
            "the test is in the table"
        );
        let found = family(&table, shell);
        if found.len() >= 2 || Instant::now() > deadline {
            break found;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    // The shell and its `sleep`. The shell does not lead a session, so only descendants count.
    assert_eq!(found.first(), Some(&shell));
    assert_eq!(found.len(), 2, "{found:?}");
}
