//! The hook shim, `nh hook <harness> <event>`, as a process.
//!
//! The first group runs the binary against a fake core: a Unix socket in a test thread. It checks
//! what the shim sends and that every failure ends in exit code 0 with nothing printed. The last
//! test runs it against the real core.

mod core_support;

use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU32, Ordering},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

const NH: &str = env!("CARGO_BIN_EXE_nh");
const MAX_STDIN: usize = 4 * 1024 * 1024;

/// A directory under `/tmp`, removed on drop. The name is short: a socket path holds 103 bytes.
struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let path = PathBuf::from(format!(
            "/tmp/nhk-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a directory");
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// What a run of the shim did.
struct Run {
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    elapsed: Duration,
}

impl Run {
    /// The shim always exits 0 and prints nothing.
    fn assert_silent_success(&self) {
        assert_eq!(self.code, Some(0), "exit code");
        assert!(self.stdout.is_empty(), "stdout: {:?}", self.stdout);
        assert!(self.stderr.is_empty(), "stderr: {:?}", self.stderr);
    }
}

/// Runs `nh hook <args>` with exactly `env` (nothing else is inherited) and `stdin`.
fn run_shim(args: &[&str], env: &[(&str, &Path)], stdin: &[u8]) -> Run {
    let mut command = Command::new(NH);
    command
        .arg("hook")
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in env {
        command.env(name, value);
    }
    let started = Instant::now();
    let mut child = command.spawn().expect("the shim starts");
    let mut pipe = child.stdin.take().expect("a stdin pipe");
    let payload = stdin.to_vec();
    // A large payload needs a writer apart from the reader of the output.
    let writer = std::thread::spawn(move || {
        let _ = pipe.write_all(&payload);
    });
    let output = child.wait_with_output().expect("the shim ends");
    let elapsed = started.elapsed();
    writer.join().expect("the writer ends");
    Run {
        code: output.status.code(),
        stdout: output.stdout,
        stderr: output.stderr,
        elapsed,
    }
}

/// The environment of a pane of neohar.
fn pane_env<'a>(socket: &'a Path, token: &'a Path) -> Vec<(&'a str, &'a Path)> {
    vec![
        ("NEOHAR", Path::new("1")),
        ("NEOHAR_SOCKET", socket),
        ("NEOHAR_PANE_TOKEN", token),
    ]
}

/// What a fake core does with a connection, after it has read the request line.
#[derive(Clone)]
enum Answer {
    /// Sends these bytes.
    Send(&'static [u8]),
    /// Sends nothing and keeps the connection open for this long.
    Silent(Duration),
    /// Closes the connection at once, without reading.
    CloseUnread,
}

/// A core that serves one connection.
struct FakeCore {
    socket: PathBuf,
    request: JoinHandle<Option<String>>,
}

impl FakeCore {
    fn start(dir: &Dir, answer: Answer) -> Self {
        let socket = dir.join("api.sock");
        let listener = UnixListener::bind(&socket).expect("a socket");
        let request = std::thread::spawn(move || {
            let (stream, _) = listener.accept().ok()?;
            if matches!(answer, Answer::CloseUnread) {
                return None;
            }
            let mut stream = stream;
            let mut line = String::new();
            BufReader::new(stream.try_clone().ok()?)
                .read_line(&mut line)
                .ok()?;
            match answer {
                Answer::Send(bytes) => {
                    let _ = stream.write_all(bytes);
                }
                Answer::Silent(time) => std::thread::sleep(time),
                Answer::CloseUnread => {}
            }
            Some(line)
        });
        Self { socket, request }
    }

    /// The request line that the shim sent.
    fn request(self) -> Value {
        let line = self
            .request
            .join()
            .expect("the fake core ends")
            .expect("a request line");
        assert!(line.ends_with('\n'), "a request is one line");
        serde_json::from_str(&line).expect("a JSON request")
    }
}

const OK: Answer = Answer::Send(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n");

#[test]
fn outside_neohar_the_shim_connects_to_nothing_and_prints_nothing() {
    let dir = Dir::new();
    let socket = dir.join("api.sock");
    let listener = UnixListener::bind(&socket).expect("a socket");
    listener.set_nonblocking(true).expect("non-blocking");
    let token = PathBuf::from("tok");
    // The variables of a pane are there, but `NEOHAR` is not.
    let env = [
        ("NEOHAR_SOCKET", socket.as_path()),
        ("NEOHAR_PANE_TOKEN", token.as_path()),
    ];
    let run = run_shim(&["claude", "Stop"], &env, br#"{"a":1}"#);
    run.assert_silent_success();
    // A connection that the shim had made would wait in the queue of the listener.
    assert_eq!(
        listener.accept().expect_err("no connection").kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn inside_neohar_the_shim_sends_one_json_rpc_line() {
    let dir = Dir::new();
    let core = FakeCore::start(&dir, OK);
    let token = PathBuf::from("pane-token-1");
    let env = pane_env(&core.socket, &token);
    let run = run_shim(
        &["claude", "PreToolUse"],
        &env,
        br#"{"tool_name":"Bash","tool_input":{"command":"ls"}}"#,
    );
    run.assert_silent_success();
    assert_eq!(
        core.request(),
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "adapter.signal",
            "params": {
                "token": "pane-token-1",
                "harness": "claude",
                "event": "PreToolUse",
                "payload": {"tool_name": "Bash", "tool_input": {"command": "ls"}},
            }
        })
    );
}

#[test]
fn a_pretty_printed_payload_still_makes_one_line() {
    let dir = Dir::new();
    let core = FakeCore::start(&dir, OK);
    let token = PathBuf::from("t");
    let env = pane_env(&core.socket, &token);
    let run = run_shim(
        &["codex", "stop"],
        &env,
        b"{\n  \"msg\": \"a\\nb\",\n  \"list\": [\n    1,\n    2\n  ]\n}\n",
    );
    run.assert_silent_success();
    let request = core.request();
    assert_eq!(
        request["params"]["payload"],
        json!({"msg": "a\nb", "list": [1, 2]})
    );
}

#[test]
fn a_payload_that_is_not_json_goes_as_a_string_and_no_payload_as_null() {
    let dir = Dir::new();
    let core = FakeCore::start(&dir, OK);
    let token = PathBuf::from("t");
    let env = pane_env(&core.socket, &token);
    run_shim(&["claude", "Stop"], &env, b"plain text\nwith two lines").assert_silent_success();
    assert_eq!(
        core.request()["params"]["payload"],
        json!("plain text\nwith two lines")
    );

    let dir = Dir::new();
    let core = FakeCore::start(&dir, OK);
    let env = pane_env(&core.socket, &token);
    run_shim(&["claude", "Stop"], &env, b"").assert_silent_success();
    assert_eq!(core.request()["params"]["payload"], Value::Null);
}

#[test]
fn a_payload_over_4_mib_is_cut_and_marked() {
    let dir = Dir::new();
    let core = FakeCore::start(&dir, OK);
    let token = PathBuf::from("t");
    let env = pane_env(&core.socket, &token);
    let mut payload = b"x".repeat(MAX_STDIN + 1000);
    payload[0] = b'q';
    let run = run_shim(&["claude", "PostToolUse"], &env, &payload);
    run.assert_silent_success();
    let request = core.request();
    assert_eq!(request["params"]["truncated"], json!(true));
    let text = request["params"]["payload"].as_str().expect("a string");
    assert_eq!(text.len(), MAX_STDIN);
    assert!(text.starts_with('q'));
}

#[test]
fn every_failure_of_the_exchange_is_exit_0_and_silent() {
    let token = PathBuf::from("t");

    // The socket does not exist.
    let dir = Dir::new();
    let missing = dir.join("missing.sock");
    let env = pane_env(&missing, &token);
    let run = run_shim(&["claude", "Stop"], &env, b"{}");
    run.assert_silent_success();
    assert!(run.elapsed < Duration::from_secs(2), "{:?}", run.elapsed);

    // The socket path is no socket.
    let file = dir.join("file");
    std::fs::write(&file, b"x").expect("a file");
    run_shim(&["claude", "Stop"], &pane_env(&file, &token), b"{}").assert_silent_success();

    // The core never answers: the shim gives up after its own time limit.
    let core = FakeCore::start(&dir, Answer::Silent(Duration::from_secs(5)));
    let run = run_shim(&["claude", "Stop"], &pane_env(&core.socket, &token), b"{}");
    run.assert_silent_success();
    assert!(
        run.elapsed < Duration::from_secs(2),
        "the shim waited {:?}",
        run.elapsed
    );
    drop(core);

    // The reply is not JSON, and it has no newline.
    let dir = Dir::new();
    let core = FakeCore::start(&dir, Answer::Send(b"\x00\xff garbage"));
    run_shim(&["claude", "Stop"], &pane_env(&core.socket, &token), b"{}").assert_silent_success();

    // The core closes at once.
    let dir = Dir::new();
    let core = FakeCore::start(&dir, Answer::CloseUnread);
    run_shim(
        &["claude", "Stop"],
        &pane_env(&core.socket, &token),
        &b"x".repeat(1 << 20),
    )
    .assert_silent_success();
}

/// Connects to `socket` without blocking until the queue of its listener is full, and keeps the
/// connections. On Linux the next connect answers `EAGAIN`; on macOS it answers `ECONNREFUSED`.
fn fill_queue(socket: &Path) -> Vec<std::os::fd::OwnedFd> {
    use rustix::{
        io::ioctl_fionbio,
        net::{AddressFamily, SocketAddrUnix, SocketType, connect, socket as new_socket},
    };

    let address = SocketAddrUnix::new(socket).expect("an address");
    let mut queue = Vec::new();
    for _ in 0..5000 {
        let fd = new_socket(AddressFamily::UNIX, SocketType::STREAM, None).expect("a socket");
        ioctl_fionbio(&fd, true).expect("non-blocking");
        if connect(&fd, &address).is_err() {
            return queue;
        }
        queue.push(fd);
    }
    panic!("the queue of the listener never filled");
}

#[test]
fn a_core_that_does_not_accept_costs_the_shim_its_time_limit_and_no_more() {
    let dir = Dir::new();
    let socket = dir.join("api.sock");
    let _listener = UnixListener::bind(&socket).expect("a socket");
    let _queue = fill_queue(&socket);
    let token = PathBuf::from("t");
    let run = run_shim(&["claude", "Stop"], &pane_env(&socket, &token), b"{}");
    run.assert_silent_success();
    assert!(run.elapsed < Duration::from_secs(2), "{:?}", run.elapsed);
}

#[test]
fn without_a_socket_or_a_token_there_is_nothing_to_send() {
    let dir = Dir::new();
    let core = FakeCore::start(&dir, OK);
    let token = PathBuf::from("t");
    let one = Path::new("1");
    // No token.
    run_shim(
        &["claude", "Stop"],
        &[("NEOHAR", one), ("NEOHAR_SOCKET", &core.socket)],
        b"{}",
    )
    .assert_silent_success();
    // No socket.
    run_shim(
        &["claude", "Stop"],
        &[("NEOHAR", one), ("NEOHAR_PANE_TOKEN", &token)],
        b"{}",
    )
    .assert_silent_success();
    // No harness and no event.
    run_shim(&[], &pane_env(&core.socket, &token), b"{}").assert_silent_success();
    run_shim(&["claude"], &pane_env(&core.socket, &token), b"{}").assert_silent_success();
    // The first connection to the fake core must be our own probe, which sends nothing. If the
    // shim had connected before, the fake core would have read a request line from it.
    drop(UnixStream::connect(&core.socket).expect("the fake core listens"));
    let first = core.request.join().expect("the fake core ends");
    assert_eq!(first.as_deref(), Some(""), "no shim connected");
}

#[test]
fn the_tee_file_records_each_event_even_without_a_core() {
    let dir = Dir::new();
    let tee = dir.join("tee");
    let token = PathBuf::from("t");
    let missing = dir.join("missing.sock");
    let mut env = pane_env(&missing, &token);
    env.push(("NEOHAR_HOOK_TEE", &tee));
    run_shim(&["claude", "Stop"], &env, br#"{"session_id":"s1"}"#).assert_silent_success();
    run_shim(&["codex", "session_start"], &env, b"not json").assert_silent_success();

    let text = std::fs::read_to_string(tee.join("signals.jsonl")).expect("a tee file");
    let lines: Vec<Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).expect("a JSON line"))
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["harness"], json!("claude"));
    assert_eq!(lines[0]["event"], json!("Stop"));
    assert_eq!(lines[0]["payload"], json!({"session_id": "s1"}));
    assert_eq!(lines[1]["harness"], json!("codex"));
    assert_eq!(lines[1]["payload"], json!("not json"));
    let at = lines[0]["at"].as_u64().expect("unix milliseconds");
    assert!(at > 1_700_000_000_000, "{at}");
    assert!(lines[1]["at"].as_u64().expect("unix ms") >= at);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_real_core_receives_the_signal() {
    let core = core_support::Harness::start();
    let socket = core.api_socket();
    assert!(socket.exists(), "the core binds api.sock when it starts");
    let token = PathBuf::from(core_support::TEST_PANE_TOKEN);
    let run = run_shim(
        &["claude", "UserPromptSubmit"],
        &pane_env(&socket, &token),
        br#"{"prompt":"hello"}"#,
    );
    run.assert_silent_success();

    // The shim waits for the reply, so the core has the signal when the shim ends.
    let signals = core.handle.recent_signals();
    assert_eq!(signals.len(), 1, "{signals:?}");
    assert_eq!(signals[0].harness, "claude");
    assert_eq!(signals[0].event, "UserPromptSubmit");
    assert!(!signals[0].truncated);

    // A client that speaks the protocol by hand gets a JSON-RPC reply.
    let mut stream = UnixStream::connect(&socket).expect("connect");
    stream
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"nope\"}\n")
        .expect("write");
    let mut reply = String::new();
    BufReader::new(&mut stream)
        .read_line(&mut reply)
        .expect("a reply");
    let reply: Value = serde_json::from_str(&reply).expect("JSON");
    assert_eq!(reply["id"], json!(9));
    assert_eq!(reply["error"]["code"], json!(-32601));
    assert_eq!(reply["error"]["data"]["code"], json!("method_not_found"));
    let mut rest = Vec::new();
    stream
        .set_read_timeout(Some(Duration::from_millis(50)))
        .expect("timeout");
    assert!(
        stream.read_to_end(&mut rest).is_err() || rest.is_empty(),
        "the connection stays open and quiet"
    );
}
