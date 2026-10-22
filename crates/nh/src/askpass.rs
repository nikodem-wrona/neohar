//! `nh askpass "<question>"`: the answer to a question of ssh.
//!
//! The app starts ssh for a link with `SSH_ASKPASS` set to this binary and `SSH_ASKPASS_REQUIRE=force`.
//! ssh runs `nh askpass "<question>"` when it needs a host key, a passphrase or a password, and it
//! reads the answer from our standard output. The command calls `askpass.ask` on the `api.sock` of
//! the app that started the link. The environment of the ssh child says which one: `NEOHAR_SOCKET`,
//! or `NEOHAR_SESSION`. `NEOHAR_MACHINE` gives the label of the machine for the sheet.
//!
//! The answer goes to standard output and nowhere else: not to a file, not to a log, not to the
//! error text. A cancel or an error exits with status 1 and writes nothing to standard output.

use std::{io::Write, path::PathBuf, process::ExitCode, time::Duration};

use nh_proto::paths::Env;
use serde_json::{Value, json};

use crate::cli::client::{CallError, Client};

/// How long a call waits for the person. A person who walks away leaves ssh waiting; ssh has its
/// own limit for the connection.
const WAIT: Duration = Duration::from_secs(600);

pub fn main(args: &[String]) -> ExitCode {
    let prompt = args.first().cloned().unwrap_or_default();
    match ask(&prompt) {
        Ok(Some(answer)) => {
            let mut out = std::io::stdout().lock();
            if writeln!(out, "{answer}")
                .and_then(|()| out.flush())
                .is_err()
            {
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::from(1),
        Err(message) => {
            eprintln!("nh askpass: {message}");
            ExitCode::from(1)
        }
    }
}

fn socket() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("NEOHAR_SOCKET").filter(|path| !path.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let session = std::env::var("NEOHAR_SESSION")
        .ok()
        .filter(|session| !session.is_empty())
        .unwrap_or_else(|| "default".to_owned());
    Env::from_process()
        .api_socket(&session)
        .map_err(|error| error.to_string())
}

/// Asks the app. `None` is a person who cancelled.
fn ask(prompt: &str) -> Result<Option<String>, String> {
    let socket = socket()?;
    let mut client = Client::connect(&socket, WAIT).map_err(describe)?;
    let mut params = json!({ "prompt": prompt });
    if let Some(machine) = std::env::var("NEOHAR_MACHINE")
        .ok()
        .filter(|machine| !machine.is_empty())
    {
        params["machine"] = Value::String(machine);
    }
    let result = client.call("askpass.ask", params).map_err(describe)?;
    Ok(result
        .get("answer")
        .and_then(Value::as_str)
        .map(str::to_owned))
}

/// The text of an error. It never holds the answer: the answer is only in a result.
fn describe(error: CallError) -> String {
    match error {
        CallError::NotRunning(path) => format!("no core serves {}", path.display()),
        CallError::Timeout(after) => format!("the core did not answer within {after:?}"),
        CallError::Io(error) => error.to_string(),
        CallError::Protocol(message) => message,
        CallError::Rpc(failure) => failure.message,
    }
}
