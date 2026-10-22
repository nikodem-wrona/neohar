//! The hook shim: `nh hook <harness> <event>`.
//!
//! A harness calls this process from its hook config, with the hook payload on stdin. The shim
//! has a budget of 5 ms, so this file reads the environment and stdin and nothing else: no
//! async runtime, no argument parser, no `tracing`, no config file.
//!
//! The shim watches. It never decides, so **every path ends in exit code 0 with nothing on
//! stdout or stderr**: outside neohar, with a missing socket, with a reply that does not parse,
//! and after a panic. A crash can then never be read as a decision.
//!
//! The shim sends one JSON-RPC line, `adapter.signal {token, harness, event, payload}`,
//! and reads one line back. It does not map the payload: the adapter in the core does.
//!
//! **The gate.** The event `PermissionRequest` goes to `adapter.gate` instead, and the shim holds
//! the hook until the core answers. The only text that the shim ever prints is
//! the `output` that the core sends with a decision, and it prints it as it is. The shim sends
//! `adapter.delivery_ack` just before. If the connection drops while the shim waits, it connects
//! again for a time (`NEOHAR_GATE_RESUME_MS`, 15 s) and sends `adapter.gate_resume` once. In every
//! other case, and after `NEOHAR_GATE_TIMEOUT_MS` (30 min), it prints nothing, and the harness
//! shows its own dialog.

use std::{
    fs::{self, OpenOptions},
    io::{self, IsTerminal, Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    process::ExitCode,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use rustix::{
    io::{Errno, ioctl_fionbio},
    net::{AddressFamily, SocketAddrUnix, SocketType, connect, socket},
};
use serde::Serialize;
use serde_json::Value;

/// The most that the shim reads from stdin.
pub const MAX_STDIN: usize = 4 * 1024 * 1024;
/// How long the shim waits for a connection to the core.
const CONNECT_TIMEOUT: Duration = Duration::from_millis(50);
/// How long the shim waits for the reply, and for each write to the socket.
const IO_TIMEOUT: Duration = Duration::from_millis(100);
/// The most that the shim reads of a reply. The reply is only a receipt.
const MAX_REPLY: usize = 64 * 1024;

/// What the shim got on stdin.
struct Stdin {
    bytes: Vec<u8>,
    /// The payload was longer than [`MAX_STDIN`]; `bytes` is the start of it.
    truncated: bool,
}

/// The line that the shim sends. `jsonrpc`, `id` and `method` are the JSON-RPC envelope.
#[derive(Serialize)]
struct Request<'a> {
    jsonrpc: &'static str,
    id: u32,
    method: &'static str,
    params: Params<'a>,
}

#[derive(Serialize)]
struct Params<'a> {
    token: &'a str,
    harness: &'a str,
    event: &'a str,
    payload: &'a Value,
    #[serde(skip_serializing_if = "is_false")]
    truncated: bool,
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "the signature serde wants"
)]
fn is_false(value: &bool) -> bool {
    !value
}

/// A line of the tee file (`NEOHAR_HOOK_TEE`).
#[derive(Serialize)]
struct TeeLine<'a> {
    /// Unix time in milliseconds.
    at: u64,
    harness: &'a str,
    event: &'a str,
    payload: &'a Value,
}

/// Runs the shim for `nh hook <args>` and returns the exit code, which is always success.
pub fn main(args: Vec<String>) -> ExitCode {
    // A panic must print nothing. The shim then ends like every other path.
    std::panic::set_hook(Box::new(|_| {}));
    let _ = std::panic::catch_unwind(|| run(&args));
    ExitCode::SUCCESS
}

fn run(args: &[String]) {
    if env("NEOHAR").is_none() {
        return;
    }
    let (Some(harness), Some(event)) = (args.first(), args.get(1)) else {
        return;
    };
    let stdin = read_stdin();
    let payload = payload(&stdin.bytes);
    if let Some(dir) = env("NEOHAR_HOOK_TEE") {
        tee(Path::new(&dir), harness, event, &payload);
    }
    let (Some(socket), Some(token)) = (env("NEOHAR_SOCKET"), env("NEOHAR_PANE_TOKEN")) else {
        return;
    };
    if GATE_EVENTS.contains(&event.as_str()) {
        gate(Path::new(&socket), &token, harness, event, &payload);
        return;
    }
    let request = request_line(&token, harness, event, &payload, stdin.truncated);
    // The shim does not care why an exchange failed.
    let _ = exchange(Path::new(&socket), &request);
}

/// The events that the shim holds until the core answers.
const GATE_EVENTS: &[&str] = &["PermissionRequest"];
/// How long the shim waits for a decision.
const DEFAULT_GATE_TIMEOUT_MS: u64 = 30 * 60 * 1000;
/// How long the shim tries to connect again after the core went away.
const DEFAULT_RESUME_WINDOW_MS: u64 = 15_000;
/// The time between two tries to connect again.
const RESUME_STEP: Duration = Duration::from_millis(100);

/// A number from the environment, in milliseconds.
fn env_ms(name: &str, default: u64) -> Duration {
    Duration::from_millis(env(name).and_then(|v| v.parse().ok()).unwrap_or(default))
}

/// What a connection to the core said about a gate.
#[derive(Debug, PartialEq, Eq)]
enum GateEnd {
    /// A decision: print `output`, after the ack with `key`.
    Decision {
        interaction: String,
        output: String,
        key: String,
    },
    /// The core has nothing to print.
    NoDecision,
    /// The connection ended. `interaction` is the id if the core had given one.
    Dropped { interaction: Option<String> },
}

/// Holds a hook for a decision of the core.
fn gate(socket: &Path, token: &str, harness: &str, event: &str, payload: &Value) {
    let deadline = Instant::now() + env_ms("NEOHAR_GATE_TIMEOUT_MS", DEFAULT_GATE_TIMEOUT_MS);
    let first = gate_line(token, harness, event, payload);
    let Ok(stream) = connect_with_timeout(socket, CONNECT_TIMEOUT) else {
        return;
    };
    let mut end = read_gate(stream, &first, deadline);
    let mut resumed = false;
    loop {
        match end {
            (
                GateEnd::Decision {
                    interaction,
                    output,
                    key,
                },
                Some(stream),
            ) => {
                ack(stream, token, &interaction, &key);
                print_output(&output);
                return;
            }
            (GateEnd::Decision { output, .. }, None) => {
                print_output(&output);
                return;
            }
            (GateEnd::NoDecision, _) => return,
            (
                GateEnd::Dropped {
                    interaction: Some(interaction),
                },
                _,
            ) if !resumed => {
                // The core went away, or restarted. Connect again, once.
                resumed = true;
                let window =
                    Instant::now() + env_ms("NEOHAR_GATE_RESUME_MS", DEFAULT_RESUME_WINDOW_MS);
                let Some(stream) = reconnect(socket, window.min(deadline)) else {
                    return;
                };
                end = read_gate(stream, &resume_line(token, &interaction), deadline);
            }
            (GateEnd::Dropped { .. }, _) => return,
        }
    }
}

/// Connects until it works or `until` passes.
fn reconnect(socket: &Path, until: Instant) -> Option<UnixStream> {
    loop {
        if let Ok(stream) = connect_with_timeout(socket, CONNECT_TIMEOUT) {
            return Some(stream);
        }
        if Instant::now() >= until {
            return None;
        }
        std::thread::sleep(RESUME_STEP);
    }
}

/// Sends a request and reads lines until the core ends the gate. The stream comes back when it is
/// still usable, so that the ack can use it.
fn read_gate(
    mut stream: UnixStream,
    request: &[u8],
    deadline: Instant,
) -> (GateEnd, Option<UnixStream>) {
    if stream.set_write_timeout(Some(IO_TIMEOUT)).is_err() || stream.write_all(request).is_err() {
        return (GateEnd::Dropped { interaction: None }, None);
    }
    let mut interaction: Option<String> = None;
    let mut pending = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        while let Some(at) = pending.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = pending.drain(..=at).collect();
            if let Some(end) = gate_line_end(&line, &mut interaction) {
                return (end, Some(stream));
            }
        }
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            return (GateEnd::NoDecision, None);
        };
        if stream
            .set_read_timeout(Some(left.max(Duration::from_millis(1))))
            .is_err()
        {
            return (GateEnd::Dropped { interaction }, None);
        }
        match stream.read(&mut chunk) {
            Ok(0) => return (GateEnd::Dropped { interaction }, None),
            Ok(n) => pending.extend_from_slice(&chunk[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return (GateEnd::Dropped { interaction }, None),
        }
    }
}

/// Reads one line from the core. Returns the end of the gate if the line is one.
fn gate_line_end(line: &[u8], interaction: &mut Option<String>) -> Option<GateEnd> {
    let value: Value = serde_json::from_slice(line).ok()?;
    // A reply to our request has `result` or `error`. A push of the core is a notification.
    let body = if value.get("method") == Some(&Value::String("adapter.gate_result".into())) {
        value.get("params")?.clone()
    } else if value.get("error").is_some() {
        return Some(GateEnd::NoDecision);
    } else {
        value.get("result")?.clone()
    };
    if let Some(id) = body.get("interaction").and_then(Value::as_str) {
        *interaction = Some(id.to_owned());
    }
    if body.get("no_decision") == Some(&Value::Bool(true)) {
        return Some(GateEnd::NoDecision);
    }
    let output = body.get("output").and_then(Value::as_str);
    let key = body.get("idempotency_key").and_then(Value::as_str);
    if let (Some(output), Some(key)) = (output, key) {
        return Some(GateEnd::Decision {
            interaction: interaction.clone().unwrap_or_default(),
            output: output.to_owned(),
            key: key.to_owned(),
        });
    }
    // `{wait: true, …}`: go on reading.
    None
}

fn gate_line(token: &str, harness: &str, event: &str, payload: &Value) -> Vec<u8> {
    let mut line = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "adapter.gate",
        "params": {"token": token, "harness": harness, "event": event, "payload": payload},
    }))
    .unwrap_or_default();
    line.push(b'\n');
    line
}

fn resume_line(token: &str, interaction: &str) -> Vec<u8> {
    let mut line = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "adapter.gate_resume",
        "params": {"token": token, "interaction": interaction},
    }))
    .unwrap_or_default();
    line.push(b'\n');
    line
}

/// Tells the core that the decision is applied. It happens just before the shim prints.
fn ack(mut stream: UnixStream, token: &str, interaction: &str, key: &str) {
    let mut line = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "adapter.delivery_ack",
        "params": {"token": token, "interaction": interaction, "idempotency_key": key, "applied": true},
    }))
    .unwrap_or_default();
    line.push(b'\n');
    if stream.set_write_timeout(Some(IO_TIMEOUT)).is_err() || stream.write_all(&line).is_err() {
        return;
    }
    // The core answers with one line. The shim waits for it a short time, so that the ack is in the
    // log of the core before the harness goes on.
    let deadline = Instant::now() + IO_TIMEOUT;
    let mut chunk = [0u8; 512];
    let mut reply = Vec::new();
    while !reply.contains(&b'\n') {
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            return;
        };
        if stream
            .set_read_timeout(Some(left.max(Duration::from_millis(1))))
            .is_err()
        {
            return;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => reply.extend_from_slice(&chunk[..n]),
        }
    }
}

/// Prints the decision of the core. It is the only text that the shim writes.
fn print_output(output: &str) {
    let mut stdout = io::stdout().lock();
    let _ = stdout.write_all(output.as_bytes());
    let _ = stdout.write_all(b"\n");
    let _ = stdout.flush();
}

/// The value of an environment variable, or `None` when it is unset or empty.
fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// Reads stdin up to [`MAX_STDIN`] bytes. A terminal has no payload, and reading it would block.
fn read_stdin() -> Stdin {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return Stdin {
            bytes: Vec::new(),
            truncated: false,
        };
    }
    let mut bytes = Vec::new();
    // A read error keeps what came before it.
    let _ = stdin
        .lock()
        .take(MAX_STDIN as u64 + 1)
        .read_to_end(&mut bytes);
    let truncated = bytes.len() > MAX_STDIN;
    bytes.truncate(MAX_STDIN);
    Stdin { bytes, truncated }
}

/// The payload as a JSON value: `null` for empty input, the parsed value for JSON, else the text
/// as a string. The shim parses it only to put it on one line. It does not look inside.
fn payload(bytes: &[u8]) -> Value {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Value::Null;
    }
    serde_json::from_slice(bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(bytes).into_owned()))
}

/// The JSON-RPC request line, with its newline. `serde_json` escapes every newline inside a
/// string and writes no other, so the result is exactly one line.
fn request_line(
    token: &str,
    harness: &str,
    event: &str,
    payload: &Value,
    truncated: bool,
) -> Vec<u8> {
    let request = Request {
        jsonrpc: "2.0",
        id: 1,
        method: "adapter.signal",
        params: Params {
            token,
            harness,
            event,
            payload,
            truncated,
        },
    };
    // A `Value` and strings always serialize.
    let mut line = serde_json::to_vec(&request).unwrap_or_default();
    line.push(b'\n');
    line
}

/// Appends the event to `<dir>/signals.jsonl`. The recorder of `tools/nh-golden` reads this file.
fn tee(dir: &Path, harness: &str, event: &str, payload: &Value) {
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    let Ok(mut line) = serde_json::to_vec(&TeeLine {
        at,
        harness,
        event,
        payload,
    }) else {
        return;
    };
    line.push(b'\n');
    let _ = fs::create_dir_all(dir);
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("signals.jsonl"))
    {
        // One `write` call, so that parallel hooks do not mix their lines.
        let _ = file.write_all(&line);
    }
}

/// Sends the request and waits for one line of reply, which it discards.
fn exchange(socket: &Path, request: &[u8]) -> io::Result<()> {
    let mut stream = connect_with_timeout(socket, CONNECT_TIMEOUT)?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    stream.write_all(request)?;
    let deadline = Instant::now() + IO_TIMEOUT;
    let mut reply = Vec::new();
    let mut chunk = [0u8; 512];
    while reply.len() < MAX_REPLY && !reply.contains(&b'\n') {
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            break;
        };
        stream.set_read_timeout(Some(left.max(Duration::from_millis(1))))?;
        match stream.read(&mut chunk)? {
            0 => break,
            n => reply.extend_from_slice(&chunk[..n]),
        }
    }
    Ok(())
}

/// `connect` to a Unix socket with a time limit. `UnixStream::connect` cannot stop: with a full
/// backlog it waits on Linux. A socket in non-blocking mode answers `EAGAIN` at once, and the
/// shim tries again until the limit.
fn connect_with_timeout(path: &Path, timeout: Duration) -> io::Result<UnixStream> {
    let address = SocketAddrUnix::new(path)?;
    let fd = socket(AddressFamily::UNIX, SocketType::STREAM, None)?;
    ioctl_fionbio(&fd, true)?;
    let deadline = Instant::now() + timeout;
    loop {
        match connect(&fd, &address) {
            Ok(()) => break,
            Err(Errno::AGAIN | Errno::INTR) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => return Err(error.into()),
        }
    }
    ioctl_fionbio(&fd, false)?;
    Ok(UnixStream::from(fd))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn empty_input_is_a_null_payload() {
        assert_eq!(payload(b""), Value::Null);
        assert_eq!(payload(b" \n\t"), Value::Null);
    }

    #[test]
    fn json_input_is_forwarded_as_a_value() {
        assert_eq!(
            payload(br#"{"tool_name":"Bash","tool_input":{"command":"ls"}}"#),
            json!({"tool_name": "Bash", "tool_input": {"command": "ls"}})
        );
    }

    #[test]
    fn text_that_is_not_json_is_forwarded_as_a_string() {
        assert_eq!(payload(b"not json {"), json!("not json {"));
        assert_eq!(payload(b"bad \xff byte"), json!("bad \u{fffd} byte"));
    }

    #[test]
    fn the_request_is_one_line_even_for_a_payload_with_newlines() {
        let payload = payload(b"{\n  \"a\": \"line one\\nline two\",\n  \"b\": [1,\n 2]\n}\n");
        let line = request_line("tok", "claude", "Stop", &payload, false);
        assert_eq!(line.iter().filter(|&&b| b == b'\n').count(), 1);
        assert_eq!(line.last(), Some(&b'\n'));
        let parsed: Value = serde_json::from_slice(&line).expect("valid JSON");
        assert_eq!(
            parsed,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "adapter.signal",
                "params": {
                    "token": "tok",
                    "harness": "claude",
                    "event": "Stop",
                    "payload": {"a": "line one\nline two", "b": [1, 2]},
                }
            })
        );
    }

    fn end(line: &str, interaction: &mut Option<String>) -> Option<GateEnd> {
        gate_line_end(line.as_bytes(), interaction)
    }

    #[test]
    fn the_first_reply_of_the_gate_is_a_wait_and_the_shim_keeps_the_id() {
        let mut id = None;
        assert_eq!(
            end(
                r#"{"jsonrpc":"2.0","id":1,"result":{"wait":true,"interaction":"i1","lease":"7"}}"#,
                &mut id
            ),
            None
        );
        assert_eq!(id.as_deref(), Some("i1"));
    }

    #[test]
    fn a_pushed_decision_ends_the_gate_with_the_output_as_it_is() {
        let mut id = Some("i1".to_owned());
        let line = r#"{"jsonrpc":"2.0","method":"adapter.gate_result","params":{"interaction":"i1","output":"{\"x\":1}","idempotency_key":"k"}}"#;
        assert_eq!(
            end(line, &mut id),
            Some(GateEnd::Decision {
                interaction: "i1".into(),
                output: "{\"x\":1}".into(),
                key: "k".into()
            })
        );
    }

    #[test]
    fn no_decision_and_an_error_end_the_gate_with_nothing_to_print() {
        let mut id = None;
        assert_eq!(
            end(
                r#"{"jsonrpc":"2.0","method":"adapter.gate_result","params":{"interaction":"i1","no_decision":true,"reason":"deadline"}}"#,
                &mut id
            ),
            Some(GateEnd::NoDecision)
        );
        assert_eq!(
            end(
                r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"x"}}"#,
                &mut id
            ),
            Some(GateEnd::NoDecision)
        );
        assert_eq!(end("not json", &mut id), None);
    }

    #[test]
    fn a_resume_reply_with_a_decision_ends_the_gate_at_once() {
        let mut id = None;
        let line = r#"{"jsonrpc":"2.0","id":2,"result":{"decision":true,"interaction":"i9","output":"o","idempotency_key":"k"}}"#;
        assert_eq!(
            end(line, &mut id),
            Some(GateEnd::Decision {
                interaction: "i9".into(),
                output: "o".into(),
                key: "k".into()
            })
        );
    }

    #[test]
    fn the_gate_lines_are_one_json_rpc_line_each() {
        let line = gate_line(
            "t",
            "claude",
            "PermissionRequest",
            &json!({"tool_name": "Bash"}),
        );
        assert_eq!(line.iter().filter(|&&b| b == b'\n').count(), 1);
        let parsed: Value = serde_json::from_slice(&line).expect("JSON");
        assert_eq!(parsed["method"], "adapter.gate");
        assert_eq!(parsed["params"]["payload"]["tool_name"], "Bash");
        let parsed: Value = serde_json::from_slice(&resume_line("t", "i1")).expect("JSON");
        assert_eq!(parsed["method"], "adapter.gate_resume");
        assert_eq!(parsed["params"]["interaction"], "i1");
        assert_eq!(
            parsed["params"]["token"], "t",
            "every adapter call carries the token"
        );
    }

    #[test]
    fn a_cut_payload_says_so() {
        let line = request_line("t", "codex", "stop", &json!("{\"a\":"), true);
        let parsed: Value = serde_json::from_slice(&line).expect("valid JSON");
        assert_eq!(parsed["params"]["truncated"], json!(true));
        let line = request_line("t", "codex", "stop", &Value::Null, false);
        let parsed: Value = serde_json::from_slice(&line).expect("valid JSON");
        assert!(parsed["params"].get("truncated").is_none());
    }
}
