//! Starting a holder.
//!
//! The core starts `nh hold --pane <ulid> --config-fd 3` and writes one `HoldConfig` to it. The
//! descriptor 3 is a pipe that carries the config; the holder detaches (double fork, `setsid`),
//! writes `READY <pid>` to its stdout, and lives on after the core.
//!
//! A shell makes the two redirections (the config pipe becomes descriptor 3, stdin becomes
//! `/dev/null`), so that this crate needs no `unsafe` code. The command is a plain
//! `tokio::process::Command` on the path of the `nh` binary, never `tauri-plugin-shell`, which
//! kills its children when the app exits.

use std::{path::Path, process::Stdio, time::Duration};

use nh_proto::{framing, holder::HoldConfig};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    time::timeout,
};
use ulid::Ulid;

/// How long a holder may take to report `READY`.
const READY_TIMEOUT: Duration = Duration::from_secs(5);

/// `$0` is the `nh` binary and `$1` is the pane id. `exec` leaves no shell behind.
const HOLD_SCRIPT: &str = r#"exec "$0" hold --pane "$1" --config-fd 3 3<&0 </dev/null"#;

/// An error from starting a holder.
#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    #[error("the sidecar {0} is not a file")]
    NoSidecar(std::path::PathBuf),
    #[error("cannot start the holder: {0}")]
    Start(std::io::Error),
    #[error("cannot send the config to the holder: {0}")]
    Config(String),
    #[error("the holder did not report READY within {} s", READY_TIMEOUT.as_secs())]
    NotReady,
    #[error("the holder stopped before it reported READY")]
    Exited,
    #[error("the holder reported an error: {0}")]
    Reported(String),
    #[error("the holder said {0:?} where it should say READY")]
    BadReady(String),
}

/// Starts a holder for `config` with the `nh` binary at `nh`. Returns the pid of the holder.
///
/// The holder is a daemon: it does not belong to this process, and it keeps running when this
/// process ends.
pub async fn spawn_holder(nh: &Path, config: &HoldConfig) -> Result<i32, SpawnError> {
    if !nh.is_file() {
        return Err(SpawnError::NoSidecar(nh.to_owned()));
    }
    let mut message = Vec::new();
    framing::write_msg(&mut message, config).map_err(|e| SpawnError::Config(e.to_string()))?;

    let mut child = Command::new("/bin/sh")
        .args(["-c", HOLD_SCRIPT])
        .arg(nh)
        .arg(Ulid::from_bytes(config.pane).to_string())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(SpawnError::Start)?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| SpawnError::Config("no pipe".into()))?;
    let stdout = child.stdout.take().ok_or(SpawnError::Exited)?;
    stdin
        .write_all(&message)
        .await
        .map_err(|e| SpawnError::Config(e.to_string()))?;
    drop(stdin);

    let mut line = String::new();
    let read = timeout(READY_TIMEOUT, BufReader::new(stdout).read_line(&mut line)).await;
    let result = match read {
        Err(_) => Err(SpawnError::NotReady),
        Ok(Err(e)) => Err(SpawnError::Start(e)),
        Ok(Ok(0)) => Err(SpawnError::Exited),
        Ok(Ok(_)) => parse_ready(&line),
    };
    // The first process leaves once the holder detached. Collect it, so that it is no zombie.
    tokio::spawn(async move {
        let _ = timeout(READY_TIMEOUT, child.wait()).await;
    });
    result
}

fn parse_ready(line: &str) -> Result<i32, SpawnError> {
    let line = line.trim();
    if let Some(message) = line.strip_prefix("ERROR ") {
        return Err(SpawnError::Reported(message.to_owned()));
    }
    line.strip_prefix("READY ")
        .and_then(|pid| pid.parse().ok())
        .ok_or_else(|| SpawnError::BadReady(line.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_carries_the_pid() {
        assert_eq!(parse_ready("READY 4242\n").expect("a pid"), 4242);
    }

    #[test]
    fn an_error_line_keeps_its_message() {
        assert!(matches!(
            parse_ready("ERROR bind: address in use\n"),
            Err(SpawnError::Reported(m)) if m == "bind: address in use"
        ));
    }

    #[test]
    fn anything_else_is_refused() {
        assert!(matches!(
            parse_ready("hello\n"),
            Err(SpawnError::BadReady(_))
        ));
        assert!(matches!(
            parse_ready("READY x\n"),
            Err(SpawnError::BadReady(_))
        ));
    }
}
