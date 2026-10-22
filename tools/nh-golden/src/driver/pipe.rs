//! The pipe mode of a recording: a harness that talks JSON lines on its standard input and output,
//! with other programs and connections next to it.
//!
//! Claude Code does this with `-p --input-format stream-json --output-format stream-json`. The
//! recorder starts the program under the sandbox with three pipes, writes the lines of the
//! `send_json` and `respond` steps, and reads the lines of the program.
//!
//! A scenario can add **peers** (`[[peer]]`): more programs of the same binary, and WebSocket
//! connections to a Unix socket. The Codex app-server listens on such a socket, and the observer
//! scenarios connect two clients to it. Every line of every peer goes to `session.jsonl`
//! with the time at which it went or came and the name of the peer.

use std::{
    collections::HashSet,
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant},
};

use futures_util::{SinkExt, StreamExt};
use nh_proto::holder::ExitStatus;
use nh_record::holder::Scratch;
use regex::Regex;
use rustix::process::{Pid, Signal, kill_process_group};
use serde_json::{Value, json};
use tokio::{runtime::Handle, sync::mpsc::UnboundedSender};
use tokio_tungstenite::tungstenite::Message;

use super::{
    FailureKeeper, Outcome, POLL, QUIET, Recording, RunContext, SETTLE_MAX, Services, finish,
};
use crate::{
    GoldenError,
    recipe::expand,
    scenario::{Action, PeerKind, Step},
};

/// One line that went to a peer or came from it.
struct Line {
    at: Duration,
    /// The index in `PipeSession::peers`. 0 is the harness.
    peer: usize,
    /// `in`, `out` or `err`.
    dir: &'static str,
    text: String,
}

/// A program, and the guard that ends its process group on every way out.
struct ChildGuard {
    child: Child,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(pid) = i32::try_from(self.child.id()).ok().and_then(Pid::from_raw) {
            // The program is the leader of its process group (see `spawn`).
            let _ = kill_process_group(pid, Signal::KILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

enum Io {
    /// A peer that a `start_peer` step has not started yet.
    NotStarted,
    Process {
        child: ChildGuard,
        stdin: Option<ChildStdin>,
    },
    Ws {
        tx: UnboundedSender<String>,
    },
}

struct PipePeer {
    name: String,
    io: Io,
    exit: Option<ExitStatus>,
}

struct PipeSession {
    peers: Vec<PipePeer>,
    tx: Sender<Line>,
    rx: Receiver<Line>,
    lines: Vec<Line>,
    begin: Instant,
    last_output: Instant,
    /// For each peer, where `wait_line` looks next: after the last line that a step took.
    line_cursor: Vec<usize>,
    /// Requests that a step answered, by peer and id.
    answered: HashSet<(usize, String)>,
    tee_file: PathBuf,
    signal_cursor: usize,
    proc_roots: Vec<String>,
    proc_replacements: Vec<(String, String)>,
    processes: Vec<(String, String)>,
    handle: Handle,
}

fn spawn_reader<R: std::io::Read + Send + 'static>(
    stream: R,
    peer: usize,
    dir: &'static str,
    begin: Instant,
    tx: Sender<Line>,
) {
    thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            let Ok(text) = line else { break };
            let at = begin.elapsed();
            if tx
                .send(Line {
                    at,
                    peer,
                    dir,
                    text,
                })
                .is_err()
            {
                break;
            }
        }
    });
}

/// Starts a program under the sandbox with three pipes.
fn spawn(
    argv: &[String],
    ctx: &RunContext<'_>,
    index: usize,
    begin: Instant,
    tx: &Sender<Line>,
) -> Result<(ChildGuard, Option<ChildStdin>), GoldenError> {
    let (program, args) = argv
        .split_first()
        .ok_or_else(|| GoldenError::Setup("there is no program to run".into()))?;
    let mut child = Command::new(program)
        .args(args)
        .current_dir(&ctx.work)
        .env_clear()
        .envs(ctx.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // A group of its own, so that the guard can end the program and what it started.
        .process_group(0)
        .spawn()
        .map_err(GoldenError::io(format!("start {program}")))?;
    if let Some(stdout) = child.stdout.take() {
        spawn_reader(stdout, index, "out", begin, tx.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_reader(stderr, index, "err", begin, tx.clone());
    }
    let stdin = child.stdin.take();
    Ok((ChildGuard { child }, stdin))
}

/// Connects to a Unix socket, speaks WebSocket on it, and bridges the messages to the channels of
/// the session. A text message that arrives is a line. A string sent to the returned channel goes
/// out as a text message.
fn connect_ws(
    handle: &Handle,
    path: &Path,
    index: usize,
    begin: Instant,
    tx: Sender<Line>,
) -> Result<UnboundedSender<String>, GoldenError> {
    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let stream = handle
        .block_on(async {
            let unix = tokio::net::UnixStream::connect(path).await?;
            tokio_tungstenite::client_async("ws://localhost/", unix)
                .await
                .map(|(ws, _response)| ws)
                .map_err(|e| std::io::Error::other(e.to_string()))
        })
        .map_err(GoldenError::io(format!("connect to {}", path.display())))?;
    handle.spawn(async move {
        let (mut sink, mut source) = stream.split();
        loop {
            tokio::select! {
                incoming = source.next() => match incoming {
                    Some(Ok(Message::Text(text))) => {
                        let line = Line { at: begin.elapsed(), peer: index, dir: "out", text: text.to_string() };
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                    Some(Ok(_)) => {}
                },
                outgoing = out_rx.recv() => match outgoing {
                    Some(text) => {
                        if sink.send(Message::text(text)).await.is_err() {
                            break;
                        }
                    }
                    None => {
                        let _ = sink.close().await;
                        break;
                    }
                },
            }
        }
    });
    Ok(out_tx)
}

impl PipeSession {
    fn start(ctx: &RunContext<'_>, scratch: &Scratch, handle: Handle) -> Result<Self, GoldenError> {
        let begin = Instant::now();
        let (tx, rx) = mpsc::channel();
        let (child, stdin) = spawn(&ctx.sandboxed, ctx, 0, begin, &tx)?;
        let mut peers = vec![PipePeer {
            name: "main".to_owned(),
            io: Io::Process { child, stdin },
            exit: None,
        }];
        for peer in &ctx.request.scenario.peers {
            peers.push(PipePeer {
                name: peer.name.clone(),
                io: Io::NotStarted,
                exit: None,
            });
        }
        let count = peers.len();
        let mut session = Self {
            peers,
            tx,
            rx,
            lines: Vec::new(),
            begin,
            last_output: Instant::now(),
            line_cursor: vec![0; count],
            answered: HashSet::new(),
            tee_file: ctx.tee_dir.join("signals.jsonl"),
            signal_cursor: 0,
            proc_roots: ctx.proc_roots.clone(),
            proc_replacements: ctx.proc_replacements.clone(),
            processes: Vec::new(),
            handle,
        };
        for (offset, peer) in ctx.request.scenario.peers.iter().enumerate() {
            if peer.at_begin {
                session.start_peer(ctx, scratch, offset + 1)?;
            }
        }
        Ok(session)
    }

    /// Starts the peer with this index. A program waits for its `ready_path`.
    fn start_peer(
        &mut self,
        ctx: &RunContext<'_>,
        scratch: &Scratch,
        index: usize,
    ) -> Result<(), GoldenError> {
        let peer = &ctx.request.scenario.peers[index - 1];
        let io = match &peer.kind {
            PeerKind::Process { args, ready_path } => {
                let mut argv = ctx.sandboxed_base.clone();
                argv.extend(args.iter().map(|arg| expand(scratch, arg)));
                let (child, stdin) = spawn(&argv, ctx, index, self.begin, &self.tx)?;
                if let Some(path) = ready_path {
                    wait_for_path(Path::new(&expand(scratch, path)), Duration::from_secs(15))?;
                }
                Io::Process { child, stdin }
            }
            PeerKind::Ws { path } => {
                let path = PathBuf::from(expand(scratch, path));
                wait_for_path(&path, Duration::from_secs(15))?;
                let tx = connect_ws(&self.handle, &path, index, self.begin, self.tx.clone())?;
                Io::Ws { tx }
            }
        };
        self.peers[index].io = io;
        Ok(())
    }

    /// The index of the peer that a step is for.
    fn target(&self, step: &Step) -> usize {
        step.peer
            .as_deref()
            .and_then(|name| self.peers.iter().position(|p| p.name == name))
            .unwrap_or(0)
    }

    /// Reads at most one line, waiting at most `within`, and looks at the programs.
    fn pump(&mut self, within: Duration) -> Result<(), GoldenError> {
        if nh_record::signals::interrupted() {
            return Err(nh_record::RecordError::Interrupted.into());
        }
        match self.rx.recv_timeout(within) {
            Ok(line) => {
                if line.dir == "out" && line.peer == 0 {
                    self.last_output = Instant::now();
                }
                self.lines.push(line);
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {}
        }
        for index in 0..self.peers.len() {
            let peer = &mut self.peers[index];
            let ended = match &mut peer.io {
                Io::Process { child, .. } if peer.exit.is_none() => {
                    child.child.try_wait().ok().flatten()
                }
                _ => None,
            };
            if let Some(status) = ended {
                self.peers[index].exit = Some(exit_status(status));
                // The last lines are still in the pipes.
                let deadline = Instant::now() + Duration::from_millis(200);
                while let Some(left) = deadline.checked_duration_since(Instant::now()) {
                    match self.rx.recv_timeout(left) {
                        Ok(line) => self.lines.push(line),
                        Err(_) => break,
                    }
                }
            }
        }
        Ok(())
    }

    fn pump_for(&mut self, time: Duration) -> Result<(), GoldenError> {
        let deadline = Instant::now() + time;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            self.pump(left.min(POLL))?;
        }
        Ok(())
    }

    /// Replaces the `{{find:REGEX}}` markers in the strings of `value` with the first match in the
    /// lines that the programs wrote so far (capture group 1 when the expression has one). A
    /// scenario uses it to send an id that a response gave, such as the id of a thread.
    fn expand_finds(&self, value: &mut Value) -> Result<(), GoldenError> {
        match value {
            Value::String(text) if text.contains("{{find:") => {
                let seen: String = self
                    .lines
                    .iter()
                    .filter(|l| l.dir == "out")
                    .map(|l| l.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                let mut out = String::new();
                let mut rest = text.as_str();
                while let Some(start) = rest.find("{{find:") {
                    out.push_str(&rest[..start]);
                    let after = &rest[start + "{{find:".len()..];
                    let end = after.find("}}").ok_or_else(|| {
                        GoldenError::Step(format!("an open {{{{find: in {text:?}"))
                    })?;
                    let pattern = &after[..end];
                    let regex = Regex::new(pattern)
                        .map_err(|e| GoldenError::Step(format!("find {pattern:?}: {e}")))?;
                    let found = regex.captures(&seen).ok_or_else(|| {
                        GoldenError::Step(format!("find {pattern:?}: no match in the lines so far"))
                    })?;
                    out.push_str(
                        found
                            .get(1)
                            .or_else(|| found.get(0))
                            .map_or("", |m| m.as_str()),
                    );
                    rest = &after[end + 2..];
                }
                out.push_str(rest);
                *text = out;
            }
            Value::Array(items) => {
                for item in items {
                    self.expand_finds(item)?;
                }
            }
            Value::Object(map) => {
                for item in map.values_mut() {
                    self.expand_finds(item)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn write_line(&mut self, peer: usize, value: &Value) -> Result<(), GoldenError> {
        let mut value = value.clone();
        self.expand_finds(&mut value)?;
        let text = value.to_string();
        let name = self.peers[peer].name.clone();
        match &mut self.peers[peer].io {
            Io::Process {
                stdin: Some(stdin), ..
            } => writeln!(stdin, "{text}")
                .and_then(|()| stdin.flush())
                .map_err(GoldenError::io(format!("write to {name}")))?,
            Io::Ws { tx } => tx
                .send(text.clone())
                .map_err(|_| GoldenError::Step(format!("the connection of {name} is closed")))?,
            _ => {
                return Err(GoldenError::Step(format!(
                    "{name} cannot take a line: its input is closed or it is not started"
                )));
            }
        }
        self.lines.push(Line {
            at: self.begin.elapsed(),
            peer,
            dir: "in",
            text,
        });
        Ok(())
    }

    /// The `out` lines of a peer as JSON, with their position in `lines`.
    fn out_json(&self, peer: usize) -> impl Iterator<Item = (usize, Value)> + '_ {
        self.lines.iter().enumerate().filter_map(move |(at, line)| {
            (line.dir == "out" && line.peer == peer)
                .then(|| serde_json::from_str::<Value>(&line.text).ok())
                .flatten()
                .map(|value| (at, value))
        })
    }

    fn signals(&self) -> Vec<Value> {
        fs::read_to_string(&self.tee_file)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    fn failure(&self, number: usize, action: &str, why: &str) -> GoldenError {
        let tail: Vec<String> = self
            .lines
            .iter()
            .rev()
            .take(12)
            .rev()
            .map(|l| {
                format!(
                    "{} {} {}",
                    self.peers[l.peer].name,
                    l.dir,
                    l.text.chars().take(240).collect::<String>()
                )
            })
            .collect();
        GoldenError::Step(format!(
            "step {number} ({action}) failed: {why}.\nThe last lines:\n{}",
            tail.join("\n")
        ))
    }

    /// Waits for an `out` line of a peer that holds a JSON object `matches` accepts and that no
    /// step answered, and returns it.
    fn await_request(
        &mut self,
        number: usize,
        peer: usize,
        timeout: Duration,
        what: &str,
        matches: impl Fn(&Value) -> Option<String>,
    ) -> Result<(String, Value), GoldenError> {
        let deadline = Instant::now() + timeout;
        loop {
            let found = self.out_json(peer).find_map(|(_, value)| {
                matches(&value)
                    .filter(|id| !self.answered.contains(&(peer, id.clone())))
                    .map(|id| (id, value))
            });
            if let Some(found) = found {
                return Ok(found);
            }
            if self.peers[peer].exit.is_some() || Instant::now() >= deadline {
                return Err(self.failure(number, what, "no request is waiting for an answer"));
            }
            self.pump(POLL)?;
        }
    }

    fn run_step(
        &mut self,
        ctx: &RunContext<'_>,
        scratch: &Scratch,
        number: usize,
        step: &Step,
    ) -> Result<(), GoldenError> {
        let peer = self.target(step);
        match &step.action {
            Action::SendJson(value) => self.write_line(peer, value),
            Action::StartPeer(name) => {
                let index = self
                    .peers
                    .iter()
                    .position(|p| &p.name == name)
                    .ok_or_else(|| GoldenError::Step(format!("step {number}: no peer {name}")))?;
                self.start_peer(ctx, scratch, index)
            }
            Action::CloseStdin => {
                match &mut self.peers[peer].io {
                    Io::Process { stdin, .. } => *stdin = None,
                    Io::Ws { .. } => {
                        // Dropping the sender closes the connection.
                        self.peers[peer].io = Io::NotStarted;
                    }
                    Io::NotStarted => {}
                }
                Ok(())
            }
            Action::Sleep(time) => self.pump_for(*time),
            Action::Processes(name) => {
                self.pump_for(Duration::from_millis(100))?;
                let root = match &self.peers[peer].io {
                    Io::Process { child, .. } => i32::try_from(child.child.id()).unwrap_or(0),
                    _ => 0,
                };
                let text = crate::procs::tree_text(
                    root,
                    &self.proc_roots,
                    &self.proc_replacements,
                    &crate::procs::list(),
                );
                self.processes.push((name.clone(), text));
                Ok(())
            }
            Action::WaitLine(pattern) => {
                let regex = Regex::new(pattern).map_err(|e| GoldenError::Step(e.to_string()))?;
                let deadline = Instant::now() + step.timeout;
                loop {
                    let from = self.line_cursor[peer];
                    let found = self.lines[from..]
                        .iter()
                        .position(|l| l.peer == peer && l.dir == "out" && regex.is_match(&l.text));
                    if let Some(at) = found {
                        self.line_cursor[peer] = from + at + 1;
                        return Ok(());
                    }
                    if self.peers[peer].exit.is_some() {
                        return Err(self.failure(
                            number,
                            &format!("wait_line {pattern:?}"),
                            "the program exited first",
                        ));
                    }
                    if Instant::now() >= deadline {
                        return Err(self.failure(
                            number,
                            &format!("wait_line {pattern:?}"),
                            &format!("no line matched within {:?}", step.timeout),
                        ));
                    }
                    self.pump(POLL)?;
                }
            }
            Action::Respond(response) => {
                let (id, request) =
                    self.await_request(number, peer, step.timeout, "respond", |v| {
                        (v["type"] == "control_request")
                            .then(|| v["request_id"].as_str().map(str::to_owned))
                            .flatten()
                    })?;
                let mut body = response.clone();
                put_input(&mut body, &request["request"]["input"]);
                self.answered.insert((peer, id.clone()));
                self.write_line(
                    peer,
                    &json!({
                        "type": "control_response",
                        "response": { "subtype": "success", "request_id": id, "response": body },
                    }),
                )
            }
            Action::AnswerRequest { method, result } => {
                let wanted = method.clone();
                let (id, _) = self.await_request(
                    number,
                    peer,
                    step.timeout,
                    &format!("answer_request {method}"),
                    move |v| {
                        (v["method"] == wanted.as_str() && !v["id"].is_null())
                            .then(|| v["id"].to_string())
                    },
                )?;
                let id_value: Value = serde_json::from_str(&id).unwrap_or(Value::Null);
                self.answered.insert((peer, id));
                self.write_line(peer, &json!({ "id": id_value, "result": result }))
            }
            Action::WaitSignal(event) => {
                let deadline = Instant::now() + step.timeout;
                loop {
                    let signals = self.signals();
                    if let Some(at) = signals
                        .iter()
                        .enumerate()
                        .skip(self.signal_cursor)
                        .find(|(_, s)| s["event"] == *event)
                        .map(|(at, _)| at)
                    {
                        self.signal_cursor = at + 1;
                        return Ok(());
                    }
                    if Instant::now() >= deadline {
                        return Err(self.failure(
                            number,
                            &format!("wait_signal {event}"),
                            &format!("no such hook within {:?}", step.timeout),
                        ));
                    }
                    self.pump(POLL)?;
                }
            }
            Action::ExpectSignal(event) => {
                self.pump_for(Duration::from_millis(50))?;
                let signals = self.signals();
                if let Some(at) = signals
                    .iter()
                    .enumerate()
                    .skip(self.signal_cursor)
                    .find(|(_, s)| s["event"] == *event)
                    .map(|(at, _)| at)
                {
                    self.signal_cursor = at + 1;
                    Ok(())
                } else {
                    Err(self.failure(
                        number,
                        &format!("expect_signal {event}"),
                        "no such hook yet",
                    ))
                }
            }
            Action::ExpectExit => {
                let deadline = Instant::now() + step.timeout;
                while self.peers[peer].exit.is_none() {
                    if Instant::now() >= deadline {
                        return Err(self.failure(
                            number,
                            "expect_exit",
                            &format!("the program is still running after {:?}", step.timeout),
                        ));
                    }
                    self.pump(POLL)?;
                }
                Ok(())
            }
            other => Err(GoldenError::Step(format!(
                "step {number}: the action {other:?} does not work with pipes"
            ))),
        }
    }

    /// Reads until the output of the harness is quiet. Then it closes the inputs and waits for the
    /// harness to end. A harness that is a server (it does not read its input) stays, and the
    /// guard stops it.
    fn settle(&mut self) -> Result<(), GoldenError> {
        let deadline = Instant::now() + SETTLE_MAX;
        while self.peers[0].exit.is_none()
            && self.last_output.elapsed() < QUIET
            && Instant::now() < deadline
        {
            self.pump(POLL)?;
        }
        for peer in &mut self.peers {
            if let Io::Process { stdin, .. } = &mut peer.io {
                *stdin = None;
            }
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.peers[0].exit.is_none() && Instant::now() < deadline {
            self.pump(POLL)?;
        }
        Ok(())
    }

    /// The bytes of `session.jsonl`.
    fn session_file(&self) -> Vec<u8> {
        let mut out = String::new();
        for line in &self.lines {
            let t = line.at.as_micros() as f64 / 1_000_000.0;
            let mut record = match serde_json::from_str::<Value>(&line.text) {
                Ok(value) => json!({ "t": t, "dir": line.dir, "json": value }),
                Err(_) => json!({ "t": t, "dir": line.dir, "text": line.text }),
            };
            if line.peer != 0 {
                record["peer"] = json!(self.peers[line.peer].name);
            }
            out.push_str(&record.to_string());
            out.push('\n');
        }
        out.into_bytes()
    }
}

/// Waits until a path exists. A symbolic link counts when its target exists.
fn wait_for_path(path: &Path, within: Duration) -> Result<(), GoldenError> {
    let deadline = Instant::now() + within;
    while !path.exists() {
        if Instant::now() >= deadline {
            return Err(GoldenError::Step(format!(
                "{} did not appear within {within:?}",
                path.display()
            )));
        }
        thread::sleep(POLL);
    }
    Ok(())
}

/// Replaces every string `"$input"` in `value` with `input`.
fn put_input(value: &mut Value, input: &Value) {
    match value {
        Value::String(text) if text == "$input" => *value = input.clone(),
        Value::Array(items) => items.iter_mut().for_each(|v| put_input(v, input)),
        Value::Object(map) => map.values_mut().for_each(|v| put_input(v, input)),
        _ => {}
    }
}

fn exit_status(status: std::process::ExitStatus) -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    match (status.code(), status.signal()) {
        (Some(code), _) => ExitStatus::Code(code),
        (None, Some(signal)) => ExitStatus::Signal(signal),
        (None, None) => ExitStatus::Code(-1),
    }
}

/// Removes the real socket files that a program made behind a link in the scratch tree. The
/// Codex app-server puts the socket in `/tmp/codex-daemon-<uid>/` and links to it. Only files in
/// the writable places of the scenario go.
struct SocketCleanup {
    links: Vec<PathBuf>,
    writable: Vec<PathBuf>,
}

impl Drop for SocketCleanup {
    fn drop(&mut self) {
        for link in &self.links {
            let Ok(target) = fs::read_link(link) else {
                continue;
            };
            let inside = self.writable.iter().any(|w| target.starts_with(w));
            if inside {
                let _ = fs::remove_file(&target);
                let mut lock = target.into_os_string();
                lock.push(".lock");
                let _ = fs::remove_file(PathBuf::from(lock));
            }
        }
    }
}

/// Runs the harness with pipes.
pub(super) fn run_pipe(
    ctx: &RunContext<'_>,
    scratch: Scratch,
    services: Services,
) -> Result<Recording, GoldenError> {
    // Declared after the scratch tree (a parameter), so that they run first when the recording
    // fails: the tree is still there to copy.
    let mut keeper = FailureKeeper::new(&ctx.canonical_root);
    let uid = rustix::process::getuid().as_raw().to_string();
    let writable: Vec<PathBuf> = ctx
        .request
        .scenario
        .writable
        .iter()
        .map(|path| PathBuf::from(path.replace("{uid}", &uid)))
        .collect();
    let links: Vec<PathBuf> = ctx
        .request
        .scenario
        .peers
        .iter()
        .filter_map(|peer| match &peer.kind {
            PeerKind::Ws { path } => Some(PathBuf::from(expand(&scratch, path))),
            PeerKind::Process { ready_path, .. } => ready_path
                .as_ref()
                .map(|p| PathBuf::from(expand(&scratch, p))),
        })
        .collect();
    let _cleanup = SocketCleanup { links, writable };
    let mut session = PipeSession::start(ctx, &scratch, services.runtime.handle().clone())?;
    let outcome = ctx
        .request
        .scenario
        .steps
        .iter()
        .enumerate()
        .try_for_each(|(index, step)| session.run_step(ctx, &scratch, index + 1, step))
        .and_then(|()| session.settle());
    if let Err(error) = outcome {
        let _ = fs::write(
            ctx.canonical_root.join("failed-session.jsonl"),
            session.session_file(),
        );
        return Err(error);
    }
    let exit = session.peers[0].exit;
    let bytes = session.session_file();
    let processes = std::mem::take(&mut session.processes);
    drop(session);
    let recording = finish(
        ctx,
        services,
        Outcome {
            session: bytes,
            session_file: "session.jsonl",
            screens: Vec::new(),
            processes,
            exit,
            replies: 0,
        },
    )?;
    keeper.succeeded();
    Ok(recording)
}
