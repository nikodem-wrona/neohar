//! A minimal stand-in for the API socket of the core.
//!
//! The hook shim sends one `adapter.signal` or `adapter.gate` line and reads one line back.
//! This socket accepts that exchange, keeps what arrived, and answers
//! `{"result":{"ok":true}}`. A recording then shows that the whole path of the shim works with a real harness, and the driver checks
//! that every hook which wrote a tee line also reached the socket.

use std::{
    io::{self, BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::Path,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use serde_json::Value;

/// One `adapter.signal` or `adapter.gate` call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiCall {
    pub token: String,
    pub harness: String,
    pub event: String,
}

/// A running socket.
pub struct ApiSocket {
    calls: Arc<Mutex<Vec<ApiCall>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ApiSocket {
    /// Starts to listen on `path`.
    pub fn start(path: &Path) -> io::Result<Self> {
        let listener = UnixListener::bind(path)?;
        listener.set_nonblocking(true)?;
        let calls = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (calls, stop) = (Arc::clone(&calls), Arc::clone(&stop));
            thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let calls = Arc::clone(&calls);
                            thread::spawn(move || serve(stream, &calls));
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => break,
                    }
                }
            })
        };
        Ok(Self {
            calls,
            stop,
            thread: Some(thread),
        })
    }

    /// The calls so far.
    pub fn calls(&self) -> Vec<ApiCall> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Stops listening and returns the calls.
    pub fn stop(mut self) -> Vec<ApiCall> {
        self.shut_down();
        self.calls()
    }

    fn shut_down(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for ApiSocket {
    fn drop(&mut self) {
        self.shut_down();
    }
}

fn serve(stream: UnixStream, calls: &Mutex<Vec<ApiCall>>) {
    // The stream of an accepted socket may inherit the non-blocking mode of the listener.
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut line = String::new();
    let mut reader = BufReader::new(&stream);
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let Ok(request) = serde_json::from_str::<Value>(&line) else {
        return;
    };
    let text = |key: &str| {
        request["params"][key]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    // A hook that the shim may hold (`adapter.gate`) is a call too. This socket has no core behind
    // it, so it answers with a receipt, and the shim prints nothing.
    if request["method"] == "adapter.signal" || request["method"] == "adapter.gate" {
        calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(ApiCall {
                token: text("token"),
                harness: text("harness"),
                event: text("event"),
            });
    }
    let reply = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":{},\"result\":{{\"ok\":true}}}}\n",
        request["id"]
    );
    let mut stream = &stream;
    let _ = stream.write_all(reply.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn send(path: &Path, line: &str) -> String {
        let mut stream = UnixStream::connect(path).expect("connect");
        stream.write_all(line.as_bytes()).expect("write");
        let mut reply = String::new();
        BufReader::new(&mut stream)
            .read_line(&mut reply)
            .expect("reply");
        reply
    }

    #[test]
    fn a_signal_is_kept_and_answered() {
        let dir = std::env::temp_dir().join(format!("nhg-api-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("a.sock");
        let socket = ApiSocket::start(&path).expect("start");
        let reply = send(
            &path,
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"adapter.signal\",\"params\":{\"token\":\"t\",\"harness\":\"claude\",\"event\":\"Stop\",\"payload\":null}}\n",
        );
        assert!(reply.contains("\"ok\":true"), "{reply}");
        // Another method gets an answer and is not counted.
        send(
            &path,
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"other\",\"params\":{}}\n",
        );
        let calls = socket.stop();
        assert_eq!(
            calls,
            [ApiCall {
                token: "t".into(),
                harness: "claude".into(),
                event: "Stop".into()
            }]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
