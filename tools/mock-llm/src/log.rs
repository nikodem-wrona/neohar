//! The request log: one JSON value for each request, in the order of arrival.
//!
//! The harness recordings use it to learn what each harness calls, so the log keeps every
//! request, also one to an endpoint that the mock does not know. It never keeps a secret: the value
//! of a header that looks like one is replaced.

use std::{
    fs::File,
    io::{self, Write},
    path::Path,
    sync::{Arc, Mutex, PoisonError},
};

use serde_json::Value;

/// Where the records go.
#[derive(Clone)]
pub enum LogSink {
    Discard,
    /// A list in memory, for tests.
    Memory(Arc<Mutex<Vec<Value>>>),
    /// A file with one JSON value on each line.
    File(Arc<Mutex<File>>),
}

impl LogSink {
    /// A sink that writes JSON lines to `path`. The file is made, or emptied.
    pub fn file(path: &Path) -> io::Result<Self> {
        Ok(Self::File(Arc::new(Mutex::new(File::create(path)?))))
    }

    /// A sink in memory, and the list that it fills.
    pub fn memory() -> (Self, Arc<Mutex<Vec<Value>>>) {
        let list = Arc::new(Mutex::new(Vec::new()));
        (Self::Memory(Arc::clone(&list)), list)
    }

    pub(crate) fn write(&self, record: &Value) {
        match self {
            Self::Discard => {}
            Self::Memory(list) => list
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(record.clone()),
            Self::File(file) => {
                let mut file = file.lock().unwrap_or_else(PoisonError::into_inner);
                // A failing log must not fail the request.
                let _ = writeln!(file, "{record}").and_then(|()| file.flush());
            }
        }
    }
}

/// Whether the value of a header must not be logged.
pub(crate) fn is_secret(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "authorization",
        "api-key",
        "apikey",
        "cookie",
        "token",
        "secret",
    ]
    .iter()
    .any(|word| name.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_that_look_like_secrets_are_found_by_name() {
        for name in [
            "Authorization",
            "x-api-key",
            "X-Api-Key",
            "cookie",
            "x-session-token",
        ] {
            assert!(is_secret(name), "{name}");
        }
        for name in [
            "user-agent",
            "content-type",
            "anthropic-version",
            "session_id",
        ] {
            assert!(!is_secret(name), "{name}");
        }
    }

    #[test]
    fn a_memory_sink_keeps_the_records_in_order_and_a_file_sink_writes_lines() {
        let (sink, list) = LogSink::memory();
        sink.write(&serde_json::json!({ "n": 1 }));
        sink.write(&serde_json::json!({ "n": 2 }));
        assert_eq!(list.lock().expect("lock").len(), 2);
        let path = std::env::temp_dir().join(format!("mock-llm-log-{}.jsonl", std::process::id()));
        let file = LogSink::file(&path).expect("a file");
        file.write(&serde_json::json!({ "a": 1 }));
        file.write(&serde_json::json!({ "b": 2 }));
        let text = std::fs::read_to_string(&path).expect("the file");
        assert_eq!(text, "{\"a\":1}\n{\"b\":2}\n");
        let _ = std::fs::remove_file(path);
    }
}
