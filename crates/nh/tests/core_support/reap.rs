//! Finds and kills the holders of a test directory, also when no core knows them.
//!
//! A holder detaches from the core that starts it (double fork, `setsid`). Its parent is then
//! `launchd`, so no tree of processes leads to it, and it outlives the core by design. Each holder
//! that a core of a test starts has its socket at `<dir>/run/<session>/holders/<pane>.sock` and its
//! log at `<dir>/data/<session>/logs/holder-<pane>.log`, and its command line is
//! `nh hold --pane <pane> …`. A holder binds its socket again when the socket is gone, so a holder
//! that a guard forgets makes the directory again after the guard removed it.
//!
//! The pane id is a ULID. These functions find a holder by the pane id on its command line, never
//! by a pid from a database: the system can give the pid of a holder that ended to another process.

#![allow(dead_code)]

use std::{
    collections::HashSet,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

use rustix::process::{Pid, Signal, kill_process, test_kill_process};

/// How long [`kill_under`] waits for the holders that it killed to be gone.
const GONE_WAIT: Duration = Duration::from_secs(5);

/// The pane ids of the holders that have a socket or a log under `dir`, in all sessions.
pub fn panes_under(dir: &Path) -> HashSet<String> {
    let mut panes = HashSet::new();
    for (root, sub, prefix, suffix) in [
        ("run", "holders", "", ".sock"),
        ("data", "logs", "holder-", ".log"),
    ] {
        let Ok(sessions) = std::fs::read_dir(dir.join(root)) else {
            continue;
        };
        for session in sessions.flatten() {
            let Ok(files) = std::fs::read_dir(session.path().join(sub)) else {
                continue;
            };
            for file in files.flatten() {
                let name = file.file_name();
                let pane = name
                    .to_str()
                    .and_then(|name| name.strip_prefix(prefix))
                    .and_then(|name| name.strip_suffix(suffix));
                if let Some(pane) = pane {
                    panes.insert(pane.to_owned());
                }
            }
        }
    }
    panes
}

/// The pid of each holder process of these panes that runs now, from `ps`.
pub fn running(panes: &HashSet<String>) -> Vec<i32> {
    if panes.is_empty() {
        return Vec::new();
    }
    let Ok(out) = Command::new("ps")
        .args(["-Ao", "pid=,args=", "-ww"])
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let (pid, args) = line.trim_start().split_once(' ')?;
            let (_, after) = args.split_once(" hold --pane ")?;
            let pane = after.split_whitespace().next()?;
            if !panes.contains(pane) {
                return None;
            }
            pid.parse().ok()
        })
        .collect()
}

/// Kills each holder that has a socket or a log under `dir`, and waits until it is gone. Returns
/// the number of holders that it killed. The program of a holder gets `SIGHUP` when its terminal
/// closes.
///
/// Call it after the core of `dir` stopped: a core that runs can start one more holder.
pub fn kill_under(dir: &Path) -> usize {
    let pids = running(&panes_under(dir));
    for raw in &pids {
        if let Some(pid) = Pid::from_raw(*raw) {
            let _ = kill_process(pid, Signal::KILL);
        }
    }
    let deadline = Instant::now() + GONE_WAIT;
    while pids.iter().any(|raw| alive(*raw)) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    pids.len()
}

fn alive(raw: i32) -> bool {
    Pid::from_raw(raw).is_some_and(|pid| test_kill_process(pid).is_ok())
}
