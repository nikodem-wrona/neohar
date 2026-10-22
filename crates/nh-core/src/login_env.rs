//! The login environment of the person.
//!
//! An app that Finder or the Dock starts gets the short environment of launchd, and a process that
//! ssh starts gets a short one too. Panes, agent runs and the discovery of harnesses
//! need the environment of the login shell of the person: its `PATH` above all. The host reads it
//! once at its start: `$SHELL -l -i -c` with a command that prints a sentinel and then `env -0`,
//! with a limit of three seconds. A profile can print text before the sentinel, so the text before
//! it is left out. When the shell fails, the environment of the process is the result, with
//! `/opt/homebrew/bin`, `/usr/local/bin` and `~/.local/bin` added to its `PATH`.
//!
//! Before the shell runs, the variables that a harness or a pane of neohar gave to the process are
//! left out: an app that Claude Code or a pane started must not give them to every pane.
//!
//! The shell runs in a new session, so that an interactive shell finds no terminal to take, and in
//! the home directory, so that a profile that reads the current directory sees the same one each
//! time. Its stdin is closed.

#![allow(unsafe_code)]

use std::{
    io::Read,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex, PoisonError},
    thread,
    time::{Duration, Instant},
};

/// How long the login shell may take.
pub const TIMEOUT: Duration = Duration::from_secs(3);

/// How long the read waits for the end of the output after the shell ended. A program that the
/// profile started in the background can keep the pipe open.
const DRAIN: Duration = Duration::from_millis(300);

/// The directories that the fallback adds to `PATH`, in this order. `~` is the home directory.
pub const FALLBACK_DIRS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin", "~/.local/bin"];

/// Variables that belong to the shell that printed them, not to a pane.
const SHELL_OWN: &[&str] = &["_", "SHLVL", "PWD", "OLDPWD"];

/// Variables that a harness gives to the programs that it starts. An app that a harness
/// started (`open` in the Bash tool of Claude Code) has them, and the login shell keeps them. Then
/// every pane had them, and `CLAUDECODE=1` made the matcher find Claude Code in a pane of Codex.
/// Claude Code 2.1.301 sets the `CLAUDE` names and `AI_AGENT` for its Bash tool. Codex sets the
/// `CODEX` names for its shell commands.
const HARNESS_OWN: &[&str] = &[
    "AI_AGENT",
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_EFFORT",
    "CLAUDE_PID",
    "CODEX_SANDBOX",
    "CODEX_SANDBOX_NETWORK_DISABLED",
    "CODEX_THREAD_ID",
];

/// Variables that Claude Code sets for its Bash tool and that a person can set too. They are left
/// out only when `CLAUDECODE=1` shows that Claude Code started the app. `GIT_EDITOR=true` made a
/// `git commit` in a pane skip the editor.
const CLAUDE_TOOL: &[&str] = &[
    "COREPACK_ENABLE_AUTO_PIN",
    "GIT_EDITOR",
    "NoDefaultCurrentDirectoryInExePath",
];

/// The variables of a pane of neohar and of a pane of a task. An app that a pane
/// started has them. `PORT` is left out only with `NEOHAR_PORTS`, which a pane of a task has too.
const PANE_OWN: &[&str] = &[
    "NEOHAR",
    "NEOHAR_BIN",
    "NEOHAR_PANE_ID",
    "NEOHAR_PANE_TOKEN",
    "NEOHAR_PANE_ULID",
    "NEOHAR_PORTS",
    "NEOHAR_SESSION",
    "NEOHAR_SHIMS",
    "NEOHAR_SOCKET",
    "NEOHAR_TAB_ID",
    "NEOHAR_TASK_ID",
    "NEOHAR_WORKSPACE_ID",
    "NEOHAR_ZDOTDIR",
];

#[derive(Debug, thiserror::Error)]
pub enum LoginEnvError {
    #[error("cannot start {shell}: {reason}")]
    Start { shell: String, reason: String },
    #[error("{shell} did not print its environment within {millis} ms")]
    Timeout { shell: String, millis: u128 },
    #[error("the output of {0} has no environment after the sentinel")]
    NoEnvironment(String),
}

/// The login environment of the person, from `$SHELL`, else the fallback. The host calls it once
/// at its start, and it takes at most [`TIMEOUT`].
pub fn capture() -> Vec<(String, String)> {
    let base = without_inherited(std::env::vars().collect());
    let shell = shell_of(&base);
    match capture_with(&shell, &base, TIMEOUT) {
        Ok(env) => env,
        Err(error) => {
            tracing::warn!(%error, "the login environment is not read; panes get the environment of the app");
            fallback(&base)
        }
    }
}

/// The environment of the process without the variables that a harness or a pane gave to it:
/// [`HARNESS_OWN`], [`PANE_OWN`], [`CLAUDE_TOOL`] after Claude Code, and `PORT` after a
/// pane of a task. The login shell sets again what the profile of the person sets.
pub fn without_inherited(env: Vec<(String, String)>) -> Vec<(String, String)> {
    let by_claude = env
        .iter()
        .any(|(name, value)| name == "CLAUDECODE" && value == "1");
    let by_task = env.iter().any(|(name, _)| name == "NEOHAR_PORTS");
    env.into_iter()
        .filter(|(name, _)| {
            let name = name.as_str();
            !(HARNESS_OWN.contains(&name)
                || PANE_OWN.contains(&name)
                || (by_claude && CLAUDE_TOOL.contains(&name))
                || (by_task && name == "PORT"))
        })
        .collect()
}

/// The shell of the person: `$SHELL`, else `/bin/zsh` on macOS, else `/bin/sh`.
pub fn shell_of(env: &[(String, String)]) -> PathBuf {
    env.iter()
        .find(|(name, value)| name == "SHELL" && value.starts_with('/'))
        .map(|(_, value)| PathBuf::from(value))
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                PathBuf::from("/bin/zsh")
            } else {
                PathBuf::from("/bin/sh")
            }
        })
}

/// A sentinel that no output has by chance.
fn sentinel() -> String {
    let mut bytes = [0u8; 12];
    let _ = getrandom::fill(&mut bytes);
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("__NEOHAR_ENV_{hex}__")
}

/// The command for `-c`. It prints the sentinel in two parts, so that the command line itself does
/// not hold it: a shell can put its command line in a variable. `/usr/bin/env` is not an alias of
/// the interactive shell.
fn script(sentinel: &str) -> String {
    let (head, tail) = sentinel.split_at(sentinel.len() / 2);
    format!("printf '%s%s' '{head}' '{tail}'; exec /usr/bin/env -0")
}

/// Runs `shell -l -i -c` with `base` as its environment and reads the environment that it prints.
pub fn capture_with(
    shell: &Path,
    base: &[(String, String)],
    limit: Duration,
) -> Result<Vec<(String, String)>, LoginEnvError> {
    let name = shell.display().to_string();
    let sentinel = sentinel();
    let mut command = Command::new(shell);
    command
        .args(["-l", "-i", "-c", &script(&sentinel)])
        .env_clear()
        .envs(base.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(home) = home_of(base).filter(|home| home.is_dir()) {
        command.current_dir(home);
    }
    // SAFETY: `setsid` is one system call, which is safe after `fork`. It allocates nothing and
    // takes no lock.
    unsafe {
        command.pre_exec(|| rustix::process::setsid().map(drop).map_err(Into::into));
    }
    let mut child = command.spawn().map_err(|e| LoginEnvError::Start {
        shell: name.clone(),
        reason: e.to_string(),
    })?;
    // The output grows on a thread, so that a shell that fills the pipe does not wait for us, and
    // the part that came is there when the end of the output does not come.
    let output = Arc::new(Mutex::new(Vec::<u8>::new()));
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    if let Some(mut stdout) = child.stdout.take() {
        let output = Arc::clone(&output);
        thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            while let Ok(n) = stdout.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                output
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .extend_from_slice(&chunk[..n]);
            }
            let _ = done_tx.send(());
        });
    }
    let started = Instant::now();
    let group = rustix::process::Pid::from_raw(i32::try_from(child.id()).unwrap_or(0));
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() >= limit => {
                // The shell leads its own group, so the programs that it started end with it.
                if let Some(group) = group {
                    let _ =
                        rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                return Err(LoginEnvError::Timeout {
                    shell: name,
                    millis: limit.as_millis(),
                });
            }
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(error) => {
                return Err(LoginEnvError::Start {
                    shell: name,
                    reason: error.to_string(),
                });
            }
        }
    }
    let _ = done_rx.recv_timeout(DRAIN);
    let bytes = output
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    parse(&bytes, &sentinel).ok_or(LoginEnvError::NoEnvironment(name))
}

/// The environment that `env -0` printed after the sentinel. The text before the sentinel is the
/// noise of a profile, and a part after the last NUL is the noise of a logout script. `None` when
/// the output has no sentinel or no `PATH`.
pub fn parse(output: &[u8], sentinel: &str) -> Option<Vec<(String, String)>> {
    let at = find(output, sentinel.as_bytes())?;
    let rest = &output[at + sentinel.len()..];
    let end = rest.iter().rposition(|&b| b == 0)?;
    let vars: Vec<(String, String)> = rest[..end]
        .split(|&b| b == 0)
        .filter_map(|entry| {
            let entry = String::from_utf8_lossy(entry);
            let (name, value) = entry.split_once('=')?;
            let good = !name.is_empty() && !SHELL_OWN.contains(&name);
            good.then(|| (name.to_owned(), value.to_owned()))
        })
        .collect();
    vars.iter().any(|(name, _)| name == "PATH").then_some(vars)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn home_of(env: &[(String, String)]) -> Option<PathBuf> {
    env.iter()
        .find(|(name, _)| name == "HOME")
        .map(|(_, value)| PathBuf::from(value))
}

/// The environment of the process, with the directories of [`FALLBACK_DIRS`] at the end of its
/// `PATH` when it does not have them.
pub fn fallback(base: &[(String, String)]) -> Vec<(String, String)> {
    let home = home_of(base);
    let mut env = base.to_vec();
    let at = env.iter().position(|(name, _)| name == "PATH");
    let mut dirs: Vec<String> = at
        .map(|at| env[at].1.split(':').map(str::to_owned).collect())
        .unwrap_or_default();
    dirs.retain(|dir| !dir.is_empty());
    for dir in FALLBACK_DIRS {
        let dir = match (dir.strip_prefix("~/"), &home) {
            (Some(rest), Some(home)) => home.join(rest).display().to_string(),
            (Some(_), None) => continue,
            (None, _) => (*dir).to_owned(),
        };
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    let path = dirs.join(":");
    match at {
        Some(at) => env[at].1 = path,
        None => env.push(("PATH".to_owned(), path)),
    }
    env
}

#[cfg(test)]
mod tests;
