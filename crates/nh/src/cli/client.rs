//! A blocking client of the API socket: one connection, one request at a time.

use std::{
    io::{self, BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::Duration,
};

use serde_json::{Value, json};

/// An error reply of the core: the integer of JSON-RPC, and `{code, message, details}`.
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    pub rpc_code: i64,
    pub code: String,
    pub message: String,
    pub details: Value,
}

/// Why a call did not give a result.
#[derive(Debug)]
pub enum CallError {
    /// Nobody listens at the socket.
    NotRunning(PathBuf),
    /// The core did not answer in time.
    Timeout(Duration),
    /// The connection failed.
    Io(io::Error),
    /// The core said something that is no JSON-RPC reply.
    Protocol(String),
    /// The core answered with an error.
    Rpc(Failure),
}

pub struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    timeout: Duration,
    next_id: i64,
}

impl Client {
    /// Connects to the socket. `timeout` is how long a call waits for its reply.
    pub fn connect(path: &Path, timeout: Duration) -> Result<Self, CallError> {
        let stream = UnixStream::connect(path).map_err(|source| match source.kind() {
            io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused => {
                CallError::NotRunning(path.to_owned())
            }
            _ => CallError::Io(source),
        })?;
        let writer = stream.try_clone().map_err(CallError::Io)?;
        writer
            .set_write_timeout(Some(timeout))
            .map_err(CallError::Io)?;
        stream
            .set_read_timeout(Some(timeout))
            .map_err(CallError::Io)?;
        Ok(Self {
            reader: BufReader::new(stream),
            writer,
            timeout,
            next_id: 0,
        })
    }

    /// Sends a request and waits for the reply with its id. Notifications of the core on the way
    /// are skipped: the CLI does not subscribe.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, CallError> {
        self.next_id += 1;
        let id = self.next_id;
        let mut line = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .map_err(|e| CallError::Protocol(e.to_string()))?;
        line.push(b'\n');
        self.writer.write_all(&line).map_err(|e| self.io(e))?;
        loop {
            let mut reply = String::new();
            let read = self.reader.read_line(&mut reply).map_err(|e| self.io(e))?;
            if read == 0 {
                return Err(CallError::Protocol(
                    "the core closed the connection".to_owned(),
                ));
            }
            let value: Value = serde_json::from_str(&reply)
                .map_err(|e| CallError::Protocol(format!("the core sent no JSON: {e}")))?;
            if value.get("id") != Some(&json!(id)) {
                continue;
            }
            return match value.get("error") {
                None => Ok(value.get("result").cloned().unwrap_or(Value::Null)),
                Some(error) => Err(CallError::Rpc(failure(error))),
            };
        }
    }

    /// Reads the next line of the core, whatever it is. `None` when the core closed the connection.
    pub fn read_message(&mut self) -> Result<Option<Value>, CallError> {
        let mut line = String::new();
        let read = self.reader.read_line(&mut line).map_err(|e| self.io(e))?;
        if read == 0 {
            return Ok(None);
        }
        serde_json::from_str(&line)
            .map(Some)
            .map_err(|e| CallError::Protocol(format!("the core sent no JSON: {e}")))
    }

    /// Stops the time limit of the reads: a command that follows the log waits as long as it takes.
    pub fn wait_without_limit(&mut self) -> Result<(), CallError> {
        self.reader
            .get_ref()
            .set_read_timeout(None)
            .map_err(CallError::Io)
    }

    fn io(&self, error: io::Error) -> CallError {
        match error.kind() {
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => CallError::Timeout(self.timeout),
            _ => CallError::Io(error),
        }
    }
}

/// Reads the `error` member of a reply.
fn failure(error: &Value) -> Failure {
    let data = error.get("data");
    Failure {
        rpc_code: error.get("code").and_then(Value::as_i64).unwrap_or(-32603),
        code: data
            .and_then(|d| d.get("code"))
            .and_then(Value::as_str)
            .unwrap_or("internal")
            .to_owned(),
        message: error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("the core gave no message")
            .to_owned(),
        details: data
            .and_then(|d| d.get("details"))
            .cloned()
            .unwrap_or(Value::Null),
    }
}

#[cfg(test)]
mod tests;
