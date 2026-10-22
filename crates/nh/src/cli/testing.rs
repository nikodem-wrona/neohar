//! A fake core for the unit tests of the CLI: a Unix socket that answers from a script.

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

use serde_json::{Value, json};

/// What the fake core does when a request arrives.
#[derive(Clone)]
pub enum Reply {
    /// `{"result": …}`.
    Result(Value),
    /// An error reply: the integer, the string code and the message.
    Error(i64, &'static str, &'static str),
    /// A notification line first, then the reply.
    After(Value, Box<Reply>),
    /// Nothing for this long, then the connection closes.
    Silent(Duration),
    /// A line that is no JSON.
    Garbage,
    /// The connection closes.
    Close,
}

pub struct FakeCore {
    pub socket: PathBuf,
    /// The requests that arrived, as JSON.
    pub requests: Arc<Mutex<Vec<Value>>>,
    thread: Option<JoinHandle<()>>,
}

impl FakeCore {
    /// Listens on a new socket and answers one connection with `script`, a reply for each request.
    pub fn start(script: Vec<Reply>) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        // A short path: a Unix socket path holds about 100 bytes.
        let socket = std::env::temp_dir().join(format!(
            "nhcli{}-{}.sock",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).expect("the fake core binds");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        let thread = std::thread::spawn(move || {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let mut writer = stream.try_clone().expect("a clone");
            let mut reader = BufReader::new(stream);
            for reply in script {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                let request: Value = serde_json::from_str(&line).expect("a JSON request");
                let id = request["id"].clone();
                seen.lock().expect("a lock").push(request);
                if !send(&mut writer, &id, reply) {
                    return;
                }
            }
        });
        Self {
            socket,
            requests,
            thread: Some(thread),
        }
    }

    pub fn requests(&self) -> Vec<Value> {
        self.requests.lock().expect("a lock").clone()
    }
}

/// Writes one JSON value as a line.
fn line(writer: &mut impl Write, value: &Value) -> bool {
    writeln!(writer, "{value}").is_ok()
}

/// Sends a reply. Returns false when the connection should end.
fn send(writer: &mut impl Write, id: &Value, reply: Reply) -> bool {
    match reply {
        Reply::Result(result) => line(
            writer,
            &json!({"jsonrpc": "2.0", "id": id, "result": result}),
        ),
        Reply::Error(code, string, message) => line(
            writer,
            &json!({
                "jsonrpc": "2.0", "id": id,
                "error": {"code": code, "message": message, "data": {"code": string}},
            }),
        ),
        Reply::After(note, next) => line(writer, &note) && send(writer, id, *next),
        Reply::Silent(time) => {
            std::thread::sleep(time);
            false
        }
        Reply::Garbage => writeln!(writer, "this is not json").is_ok(),
        Reply::Close => false,
    }
}

impl Drop for FakeCore {
    fn drop(&mut self) {
        // A client that never connected leaves the accept waiting. A connection ends it.
        let _ = std::os::unix::net::UnixStream::connect(&self.socket);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.socket);
    }
}
