//! The connection to `hermes acp`: one child process, JSON-RPC 2.0 with one message on each line
//! of stdin and stdout. Stderr has the logs of Hermes; the core keeps its last lines for the error
//! of a process that ended.

use std::{
    collections::VecDeque,
    io,
    process::Stdio,
    sync::{Arc, Mutex, PoisonError},
};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::mpsc,
};

/// The lines of stderr that the core keeps.
const STDERR_LINES: usize = 20;

/// What the reader of a connection gives the worker. `generation` tells the connections apart, so
/// the end of an old one does not stop a new one.
#[derive(Debug)]
pub(crate) struct Incoming {
    pub generation: u64,
    pub kind: IncomingKind,
}

#[derive(Debug)]
pub(crate) enum IncomingKind {
    Message(Value),
    /// The process closed stdout. The text is the last line of stderr.
    Closed(String),
}

pub(crate) struct Conn {
    _child: Child,
    stdin: ChildStdin,
    next_id: u64,
}

impl Conn {
    /// Starts the program with the environment `env` (empty: the environment of the core).
    pub fn spawn(
        program: &str,
        args: &[String],
        env: &[(String, String)],
        cwd: &std::path::Path,
        generation: u64,
        out: mpsc::UnboundedSender<Incoming>,
    ) -> io::Result<Self> {
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if !env.is_empty() {
            command.env_clear().envs(env.iter().map(|(k, v)| (k, v)));
        }
        let mut child = command.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("no stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("no stdout"))?;
        let stderr = child.stderr.take();
        let tail = Arc::new(Mutex::new(VecDeque::new()));
        if let Some(stderr) = stderr {
            let tail = Arc::clone(&tail);
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let mut tail = tail.lock().unwrap_or_else(PoisonError::into_inner);
                    if tail.len() == STDERR_LINES {
                        tail.pop_front();
                    }
                    tail.push_back(line);
                }
            });
        }
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<Value>(&line) {
                    Ok(message) => {
                        let sent = out.send(Incoming {
                            generation,
                            kind: IncomingKind::Message(message),
                        });
                        if sent.is_err() {
                            return;
                        }
                    }
                    Err(error) => {
                        tracing::debug!(%error, "hermes acp wrote a line that is not JSON");
                    }
                }
            }
            // The stderr task may still read the last lines.
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let last = tail
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .iter()
                .rev()
                .find(|line| !line.trim().is_empty())
                .cloned()
                .unwrap_or_default();
            let _ = out.send(Incoming {
                generation,
                kind: IncomingKind::Closed(last),
            });
        });
        Ok(Self {
            _child: child,
            stdin,
            next_id: 1,
        })
    }

    async fn write(&mut self, message: &Value) -> io::Result<()> {
        let mut line = serde_json::to_vec(message).map_err(io::Error::other)?;
        line.push(b'\n');
        self.stdin.write_all(&line).await?;
        self.stdin.flush().await
    }

    /// Sends a request and returns its id.
    pub async fn request(&mut self, method: &str, params: Value) -> io::Result<u64> {
        let id = self.next_id;
        self.next_id += 1;
        self.write(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await?;
        Ok(id)
    }

    pub async fn notify(&mut self, method: &str, params: Value) -> io::Result<()> {
        self.write(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
            .await
    }

    pub async fn respond(&mut self, id: Value, result: Value) -> io::Result<()> {
        self.write(&json!({"jsonrpc": "2.0", "id": id, "result": result}))
            .await
    }

    pub async fn respond_error(&mut self, id: Value, code: i64, message: &str) -> io::Result<()> {
        self.write(
            &json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}),
        )
        .await
    }
}
