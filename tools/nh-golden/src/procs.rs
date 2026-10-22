//! The processes of a recording: a tree for a scenario step, and a cleanup at the end.
//!
//! A harness can start helpers, and Codex can start a daemon that leaves the process group
//! (`daemon_auto_start`). The `processes` step writes what runs under the harness, and what runs
//! elsewhere with the scratch tree in its command line. At the end of a recording the recorder
//! kills every process that still has the scratch root in its command line, so that no daemon
//! outlives the recording.

use std::{collections::BTreeMap, process::Command};

use rustix::process::{Pid, Signal, kill_process};

/// One line of `ps`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proc {
    pub pid: i32,
    pub ppid: i32,
    pub command: String,
}

/// Reads the output of `ps -axo pid=,ppid=,command=`.
pub fn parse_ps(text: &str) -> Vec<Proc> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let (pid, rest) = line.split_once(char::is_whitespace)?;
            let (ppid, command) = rest.trim_start().split_once(char::is_whitespace)?;
            Some(Proc {
                pid: pid.parse().ok()?,
                ppid: ppid.parse().ok()?,
                command: command.trim().to_owned(),
            })
        })
        .collect()
}

/// All processes of the machine.
pub fn list() -> Vec<Proc> {
    Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,command="])
        .output()
        .ok()
        .map(|o| parse_ps(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

/// Replaces paths that differ between machines. `from` pairs are applied in order.
fn clean(command: &str, replacements: &[(String, String)]) -> String {
    let mut text = command.to_owned();
    for (from, to) in replacements.iter().filter(|(from, _)| !from.is_empty()) {
        text = text.replace(from, to);
    }
    text.chars().take(200).collect()
}

/// The tree under `root`: one line for each process, indented by depth. Children are sorted by
/// their command line, so that two runs print the same text. Then the processes elsewhere that
/// have `scratch` in their command line, under the line `elsewhere:`.
pub fn tree_text(
    root: i32,
    scratch: &[String],
    replacements: &[(String, String)],
    procs: &[Proc],
) -> String {
    let mut children: BTreeMap<i32, Vec<&Proc>> = BTreeMap::new();
    for p in procs {
        children.entry(p.ppid).or_default().push(p);
    }
    let mut lines = Vec::new();
    let mut inside = std::collections::BTreeSet::new();
    fn walk<'a>(
        pid: i32,
        depth: usize,
        children: &BTreeMap<i32, Vec<&'a Proc>>,
        procs: &'a [Proc],
        replacements: &[(String, String)],
        lines: &mut Vec<String>,
        inside: &mut std::collections::BTreeSet<i32>,
    ) {
        if !inside.insert(pid) {
            return;
        }
        if let Some(p) = procs.iter().find(|p| p.pid == pid) {
            lines.push(format!(
                "{}{}",
                "  ".repeat(depth),
                clean(&p.command, replacements)
            ));
        }
        let mut kids: Vec<&Proc> = children.get(&pid).cloned().unwrap_or_default();
        kids.sort_by_key(|p| clean(&p.command, replacements));
        for kid in kids {
            walk(
                kid.pid,
                depth + 1,
                children,
                procs,
                replacements,
                lines,
                inside,
            );
        }
    }
    walk(
        root,
        0,
        &children,
        procs,
        replacements,
        &mut lines,
        &mut inside,
    );
    let mut elsewhere: Vec<String> = procs
        .iter()
        .filter(|p| {
            !inside.contains(&p.pid) && p.pid != i32::try_from(std::process::id()).unwrap_or(-1)
        })
        .filter(|p| {
            scratch
                .iter()
                .any(|s| !s.is_empty() && p.command.contains(s.as_str()))
        })
        .map(|p| clean(&p.command, replacements))
        .collect();
    elsewhere.sort();
    if !elsewhere.is_empty() {
        lines.push("elsewhere:".to_owned());
        lines.extend(elsewhere.into_iter().map(|c| format!("  {c}")));
    }
    lines.join("\n")
}

/// Kills every process, but this one, that has a path under one of the `roots` in its command
/// line. A root is a directory, so the match needs the `/` after it.
pub fn kill_with_root(roots: &[String]) {
    let me = i32::try_from(std::process::id()).unwrap_or(-1);
    let marks: Vec<String> = roots
        .iter()
        .filter(|r| !r.is_empty())
        .map(|r| format!("{r}/"))
        .collect();
    for p in list() {
        if p.pid != me
            && marks.iter().any(|m| p.command.contains(m.as_str()))
            && let Some(pid) = Pid::from_raw(p.pid)
        {
            let _ = kill_process(pid, Signal::KILL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PS: &str = "    1     0 /sbin/launchd\n  100     1 /bin/zsh -l\n  200   100 codex --flag /tmp/nhg-x/work\n  201   200 /bin/zsh -c touch /tmp/nhg-x/work/a\n  202   200 helper-b\n  300     1 codex app-server --listen unix:///tmp/nhg-x/sock\n  400     1 /usr/bin/other\n";

    #[test]
    fn ps_lines_are_read() {
        let procs = parse_ps(PS);
        assert_eq!(procs.len(), 7);
        assert_eq!(
            procs[2],
            Proc {
                pid: 200,
                ppid: 100,
                command: "codex --flag /tmp/nhg-x/work".into()
            }
        );
        assert_eq!(procs[0].command, "/sbin/launchd");
    }

    #[test]
    fn the_tree_is_sorted_cleaned_and_shows_a_daemon_that_left() {
        let procs = parse_ps(PS);
        let text = tree_text(
            200,
            &["/tmp/nhg-x".to_owned()],
            &[("/tmp/nhg-x".to_owned(), "<scratch>".to_owned())],
            &procs,
        );
        assert_eq!(
            text,
            "codex --flag <scratch>/work\n  /bin/zsh -c touch <scratch>/work/a\n  helper-b\nelsewhere:\n  codex app-server --listen unix://<scratch>/sock"
        );
    }

    #[test]
    fn a_tree_without_a_daemon_has_no_elsewhere_part() {
        let procs = parse_ps(PS);
        let text = tree_text(100, &["/tmp/nhg-other".to_owned()], &[], &procs);
        assert!(!text.contains("elsewhere"), "{text}");
        assert!(text.starts_with("/bin/zsh -l\n  codex"), "{text}");
    }
}
