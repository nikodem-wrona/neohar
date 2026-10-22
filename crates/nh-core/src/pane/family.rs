//! Ends every process that the child of a pane started.
//!
//! A pane that closes kills its child and the foreground group of its PTY. A job in the background
//! of the shell, a server that a harness started in a session of its own, and the children of
//! these are in neither, and they ran on after the pane closed. Here the core finds the whole
//! family with [`nh_agents::process::family`] and kills it.
//!
//! The walk follows the parents, and a process whose parent ends goes to `launchd`. So nothing
//! is killed until the walk is complete: each round stops the processes that it found
//! (`SIGSTOP`), and a process that is stopped cannot start another. When a round finds no new
//! process, all of them get `SIGKILL`.

use std::collections::HashSet;

use nh_agents::process::{Kin, family, table};
use rustix::process::{Pid, Signal, kill_process};

/// The most rounds of the walk. A family that forks faster than the walk stops it is cut here.
const ROUNDS: usize = 8;

/// Kills the family of `child`, the process that the holder `holder` started. Returns the number
/// of processes that were stopped and killed.
///
/// A pid comes again after its process ended, so the walk first checks that `child` is still the
/// child of `holder`. When no process has the pid, the child ended, and its session can still
/// hold processes. When another process has the pid, it is not the child, and nothing is killed.
pub(crate) fn kill_family(child: i32, holder: i32) -> usize {
    let me = i32::try_from(std::process::id()).unwrap_or(0);
    let spared = |pid: i32| pid <= 1 || pid == me || pid == holder;
    let mut stopped: HashSet<i32> = HashSet::new();
    for round in 0..ROUNDS {
        let rows = table();
        if round == 0 && !is_ours(&rows, child, holder) {
            return 0;
        }
        let found: Vec<i32> = family(&rows, child)
            .into_iter()
            .filter(|pid| !spared(*pid) && !stopped.contains(pid))
            .collect();
        if found.is_empty() {
            break;
        }
        for pid in found {
            // A process of another user (`sudo`) refuses the signal. It is left alone.
            if signal(pid, Signal::STOP) {
                stopped.insert(pid);
            }
        }
    }
    for pid in &stopped {
        signal(*pid, Signal::KILL);
    }
    stopped.len()
}

/// Whether the pid `child` names the child of `holder`, or names no process.
fn is_ours(rows: &[Kin], child: i32, holder: i32) -> bool {
    if child <= 1 {
        return false;
    }
    rows.iter()
        .find(|kin| kin.pid == child)
        .is_none_or(|kin| kin.ppid == holder)
}

fn signal(pid: i32, signal: Signal) -> bool {
    Pid::from_raw(pid).is_some_and(|pid| kill_process(pid, signal).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kin(pid: i32, ppid: i32, sid: i32) -> Kin {
        Kin { pid, ppid, sid }
    }

    #[test]
    fn a_pid_that_another_process_took_is_not_ours() {
        let rows = [kin(100, 50, 100), kin(200, 60, 200)];
        assert!(is_ours(&rows, 100, 50));
        assert!(!is_ours(&rows, 200, 50));
        // The child ended: its session can still hold processes.
        assert!(is_ours(&rows, 300, 50));
        assert!(!is_ours(&rows, 1, 0));
        assert!(!is_ours(&rows, 0, 0));
    }
}
