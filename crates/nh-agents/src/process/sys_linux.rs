//! The system reads of the process walk on Linux: `/proc/<pid>/{stat,cmdline,exe,
//! environ}` and the lists of children in `/proc/<pid>/task/<tid>/children`. The table of all
//! processes reads `/proc/<pid>/stat` of each process.

use std::{fs, path::PathBuf};

use super::{Args, Basic, Kin};

pub(super) fn basic(pid: i32) -> Option<Basic> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    parse_stat(&stat)
}

/// The fields of `/proc/<pid>/stat` after the name of the command. The name is in parentheses and
/// can hold spaces and parentheses, so the fields start after the last `)`.
fn parse_stat(stat: &str) -> Option<Basic> {
    let rest = stat.get(stat.rfind(')')? + 1..)?;
    let mut fields = rest.split_ascii_whitespace();
    let state = fields.next()?;
    let ppid = fields.next()?.parse().ok()?;
    let pgid = fields.next()?.parse().ok()?;
    // Field 22 is `starttime`, in clock ticks since the boot. It is the 19th after the state.
    let start = fields.nth(16)?.parse().ok()?;
    Some(Basic {
        ppid,
        pgid,
        start,
        zombie: state == "Z" || state == "X",
    })
}

/// Every process in `/proc`, with its parent and its session.
pub(super) fn all() -> Vec<Kin> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse::<i32>().ok())
        .filter_map(|pid| {
            let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            parse_kin(pid, &stat)
        })
        .collect()
}

/// The parent and the session in `/proc/<pid>/stat`: the second and the fourth field after the
/// state.
fn parse_kin(pid: i32, stat: &str) -> Option<Kin> {
    let rest = stat.get(stat.rfind(')')? + 1..)?;
    let mut fields = rest.split_ascii_whitespace();
    let ppid = fields.nth(1)?.parse().ok()?;
    let sid = fields.nth(1)?.parse().ok()?;
    Some(Kin { pid, ppid, sid })
}

pub(super) fn exe(pid: i32) -> Option<PathBuf> {
    let path = fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    // A program that was replaced while it runs shows ` (deleted)` after the path.
    let text = path.to_string_lossy();
    match text.strip_suffix(" (deleted)") {
        Some(clean) => Some(PathBuf::from(clean)),
        None => Some(path),
    }
}

pub(super) fn cwd(pid: i32) -> Option<PathBuf> {
    let path = fs::read_link(format!("/proc/{pid}/cwd")).ok()?;
    // A folder that was removed while a process is in it shows ` (deleted)` after the path.
    let text = path.to_string_lossy();
    match text.strip_suffix(" (deleted)") {
        Some(clean) => Some(PathBuf::from(clean)),
        None => Some(path),
    }
}

pub(super) fn children(pid: i32) -> Vec<i32> {
    let Ok(tasks) = fs::read_dir(format!("/proc/{pid}/task")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut listed = false;
    // A child belongs to the thread that forked it, so every thread is read. A process with more
    // than 128 threads is cut: a harness has a few.
    for task in tasks.flatten().take(128) {
        let Ok(text) = fs::read_to_string(task.path().join("children")) else {
            continue;
        };
        listed = true;
        out.extend(
            text.split_ascii_whitespace()
                .filter_map(|pid| pid.parse::<i32>().ok()),
        );
    }
    if listed {
        out.sort_unstable();
        out.dedup();
        return out;
    }
    // The kernel has no `children` files (it is built without `CONFIG_PROC_CHILDREN`). Scan.
    scan_children(pid)
}

fn scan_children(parent: i32) -> Vec<i32> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse::<i32>().ok())
        .filter(|pid| basic(*pid).is_some_and(|basic| basic.ppid == parent))
        .collect()
}

/// The command line and, when asked, the environment of the process.
pub(super) fn args(pid: i32, want_env: bool) -> Option<Args> {
    let cmdline = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let argv = cmdline
        .split(|byte| *byte == 0)
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .collect::<Vec<_>>();
    // The text ends with a NUL, which leaves one empty string at the end.
    let argv = match argv.split_last() {
        Some((last, rest)) if last.is_empty() => rest.to_vec(),
        _ => argv,
    };
    let env = want_env.then(|| {
        fs::read(format!("/proc/{pid}/environ"))
            .map(|bytes| {
                bytes
                    .split(|byte| *byte == 0)
                    .filter_map(|entry| {
                        let text = String::from_utf8_lossy(entry);
                        let (key, value) = text.split_once('=')?;
                        Some((key.to_owned(), value.to_owned()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    });
    Some((argv, env))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fields_after_the_name_are_read_even_when_the_name_has_parentheses() {
        let stat = "4242 (a (b) c) S 4000 4242 4000 34816 4242 4194560 1 0 0 0 1 2 0 0 20 0 3 0 987654 1000 100 \
                    18446744073709551615 0 0 0 0 0 0 0 0 0 0 0 0 17 0 0 0 0 0 0";
        let basic = parse_stat(stat).expect("fields");
        assert_eq!((basic.ppid, basic.pgid, basic.start), (4000, 4242, 987_654));
        assert!(!basic.zombie);
        let zombie = parse_stat(&stat.replace(") S ", ") Z ")).expect("fields");
        assert!(zombie.zombie);
        assert!(parse_stat("4242 no parentheses").is_none());
        let kin = parse_kin(4242, stat).expect("fields");
        assert_eq!((kin.pid, kin.ppid, kin.sid), (4242, 4000, 4000));
    }
}
