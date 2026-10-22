//! The processes of a pane, as the matcher sees them.
//!
//! [`Process`] is what one process tells: its ids, its executable and its command line. A
//! [`Tree`] is the foreground group of a pane and its descendants down to depth 6, the leader
//! first. The matcher reads trees and does no I/O, so its tests use synthetic ones. The
//! [`Walker`] fills a tree from the system: `proc_listchildpids`, `proc_pidpath` and
//! `KERN_PROCARGS2` on macOS, `/proc` on Linux.
//!
//! [`table`] and [`family`] find every process that the child of a pane started, so that the core
//! can end them all when the pane closes.

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    path::PathBuf,
};

#[cfg(target_os = "macos")]
#[path = "sys_macos.rs"]
mod sys;

#[cfg(target_os = "linux")]
#[path = "sys_linux.rs"]
mod sys;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
#[path = "sys_other.rs"]
mod sys;

/// The most processes that one walk reads. A pane that forks without end does not stall the core.
pub const MAX_PROCESSES: usize = 256;

/// How deep the walk goes below the leader of the foreground group.
pub const MAX_DEPTH: usize = 6;

/// One process.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Process {
    pub pid: i32,
    pub ppid: i32,
    /// The start time in milliseconds since the epoch, or in the units of the system. With the
    /// `pid` it names the process for as long as it lives, because a `pid` comes again.
    pub start: u64,
    /// The canonical path of the executable, when the system tells it.
    pub exe: Option<PathBuf>,
    /// The command line. `argv[0]` is the name that the process had when it started.
    pub argv: Vec<String>,
    /// The environment, when the walk read it. Only a rule with `env` needs it.
    pub env: Option<BTreeMap<String, String>>,
}

impl Process {
    /// The name of the executable: the last part of its path, or else of `argv[0]`.
    pub fn exe_name(&self) -> Option<&str> {
        match &self.exe {
            Some(path) => path.file_name().and_then(|name| name.to_str()),
            None => self.argv.first().map(|arg| base_name(arg)),
        }
    }

    /// The name that the process was started under (`exec -a`).
    pub fn argv0_name(&self) -> Option<&str> {
        self.argv.first().map(|arg| base_name(arg))
    }

    /// The name of a process for a person: the name of the executable, or `?`.
    pub fn display_name(&self) -> &str {
        self.exe_name().unwrap_or("?")
    }
}

/// The last part of a path in text.
pub(crate) fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The processes of the foreground group of a pane and their descendants. The leader is first and
/// a process comes before its children, so the order is the order of the search by breadth.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tree {
    pub processes: Vec<Process>,
}

impl Tree {
    pub fn new(processes: Vec<Process>) -> Self {
        Self { processes }
    }

    pub fn is_empty(&self) -> bool {
        self.processes.is_empty()
    }

    pub fn get(&self, pid: i32) -> Option<&Process> {
        self.processes.iter().find(|process| process.pid == pid)
    }
}

/// The command line of a process and, when asked for, its environment.
pub(crate) type Args = (Vec<String>, Option<BTreeMap<String, String>>);

/// What the system tells about a process without reading its command line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Basic {
    pub ppid: i32,
    pub pgid: i32,
    /// When the process started, in the units of the system: milliseconds since the epoch on
    /// macOS, and clock ticks since the boot on Linux. With the `pid` it names the process.
    pub start: u64,
    pub zombie: bool,
}

/// What the walk read about a process before, to read it again only when it changed.
#[derive(Debug)]
struct Cached {
    start: u64,
    exe: Option<PathBuf>,
    process: Process,
}

/// Counters of the walk, for the tests and the budget run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WalkStats {
    pub walks: u64,
    /// How many times the command line of a process was read. A process that did not change is
    /// read once.
    pub args_reads: u64,
}

/// Walks the processes of one pane.
///
/// The walk starts at the leader of the foreground group and goes down to depth 6, in the order
/// of the search by breadth. The command line and the environment cost the most, so the walker
/// keeps them for each process, and reads them again when the start time or the executable
/// changes (`exec` keeps the pid and changes the program).
#[derive(Debug)]
pub struct Walker {
    want_env: bool,
    cache: HashMap<i32, Cached>,
    stats: WalkStats,
}

impl Walker {
    /// `want_env` is true when a rule of a manifest needs the environment (`Matcher::wants_env`).
    pub fn new(want_env: bool) -> Self {
        Self {
            want_env,
            cache: HashMap::new(),
            stats: WalkStats::default(),
        }
    }

    pub fn stats(&self) -> WalkStats {
        self.stats
    }

    /// Whether the rules need the environment now. A manifest that changed can change this.
    pub fn set_want_env(&mut self, want_env: bool) {
        if self.want_env != want_env {
            self.want_env = want_env;
            self.cache.clear();
        }
    }

    /// The tree below `leader`: the process and its descendants. `leader` is the process group
    /// that the PTY has in the foreground, and the leader of a group has the pid of the group. A
    /// group whose leader is gone gives an empty tree.
    pub fn walk(&mut self, leader: i32) -> Tree {
        self.stats.walks += 1;
        let mut processes = Vec::new();
        let mut seen = HashSet::new();
        let mut kept = HashSet::new();
        let mut queue = VecDeque::from([(leader, 0_usize)]);
        while let Some((pid, depth)) = queue.pop_front() {
            if processes.len() >= MAX_PROCESSES || !seen.insert(pid) {
                continue;
            }
            let Some(basic) = sys::basic(pid) else {
                continue;
            };
            if basic.zombie {
                continue;
            }
            processes.push(self.read(pid, basic));
            kept.insert(pid);
            if depth < MAX_DEPTH {
                queue.extend(
                    sys::children(pid)
                        .into_iter()
                        .map(|child| (child, depth + 1)),
                );
            }
        }
        self.cache.retain(|pid, _| kept.contains(pid));
        Tree::new(processes)
    }

    fn read(&mut self, pid: i32, basic: Basic) -> Process {
        let exe = sys::exe(pid);
        if let Some(cached) = self.cache.get_mut(&pid)
            && cached.start == basic.start
            && cached.exe == exe
        {
            cached.process.ppid = basic.ppid;
            return cached.process.clone();
        }
        self.stats.args_reads += 1;
        let (argv, env) = sys::args(pid, self.want_env).unwrap_or_default();
        let process = Process {
            pid,
            ppid: basic.ppid,
            start: basic.start,
            exe: exe.clone(),
            argv,
            env,
        };
        self.cache.insert(
            pid,
            Cached {
                start: basic.start,
                exe,
                process: process.clone(),
            },
        );
        process
    }

    /// Whether the process `(pid, start)` still lives. A pid that came again with another start
    /// time is another process, and a zombie has ended.
    pub fn alive(&self, key: (i32, u64)) -> bool {
        alive(key)
    }
}

/// One row of the table of all processes: the ids that tie a process to the one that started it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kin {
    pub pid: i32,
    pub ppid: i32,
    /// The session. The child of a pane leads a session, so its pid is the id of the session.
    pub sid: i32,
}

/// Every process that the system shows to this user, with its parent and its session.
pub fn table() -> Vec<Kin> {
    sys::all()
}

/// The processes that `leader` started, from a [`table`]: the processes of the session of
/// `leader`, and every descendant of these and of `leader`, at any depth. `leader` is in the list
/// if it is in the table.
///
/// A job in the background of a shell is in the session of the shell. A program that leaves the
/// session (`setsid`) is still a descendant while its parent lives. The session also finds the
/// processes whose parent ended, because a process keeps its session when it is moved to
/// `launchd` or `init`. The id of a session does not come again while a process is in it, so this
/// holds after `leader` ended too.
pub fn family(table: &[Kin], leader: i32) -> Vec<i32> {
    let mut children: HashMap<i32, Vec<i32>> = HashMap::new();
    for kin in table {
        children.entry(kin.ppid).or_default().push(kin.pid);
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut queue: VecDeque<i32> = table
        .iter()
        .filter(|kin| kin.pid == leader || kin.sid == leader)
        .map(|kin| kin.pid)
        .collect();
    while let Some(pid) = queue.pop_front() {
        if !seen.insert(pid) {
            continue;
        }
        out.push(pid);
        if let Some(below) = children.get(&pid) {
            queue.extend(below.iter().copied());
        }
    }
    out
}

/// Whether the process `(pid, start)` still lives.
pub fn alive((pid, start): (i32, u64)) -> bool {
    sys::basic(pid).is_some_and(|basic| basic.start == start && !basic.zombie)
}

/// The group of processes that a process belongs to, or `None` if it is gone.
pub fn group_of(pid: i32) -> Option<i32> {
    sys::basic(pid).map(|basic| basic.pgid)
}

/// The working directory of a process, or `None` if it is gone or belongs to another user. The
/// `cwd` of a pane is the one of its foreground group.
pub fn cwd(pid: i32) -> Option<PathBuf> {
    sys::cwd(pid)
}

#[cfg(test)]
mod tests;
