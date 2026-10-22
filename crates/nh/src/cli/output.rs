//! What the CLI prints, and the exit codes.
//!
//! Results go to stdout, errors to stderr. When stdout is a terminal the CLI prints for people:
//! aligned tables and indented JSON. Otherwise, or with `--json`, it prints JSON, one document on
//! one line.

use std::io::{self, Write};

use nh_proto::api::envelope::ErrorCode;
use serde_json::{Value, json};

use super::client::{CallError, Failure};

/// The exit codes of the CLI.
pub mod exit {
    pub const OK: u8 = 0;
    pub const ERROR: u8 = 1;
    pub const USAGE: u8 = 2;
    pub const NOT_FOUND: u8 = 3;
    pub const ALREADY_ANSWERED: u8 = 4;
}

/// Why a command failed.
#[derive(Debug)]
pub enum CliError {
    /// The arguments are wrong.
    Usage(String),
    /// A call failed.
    Call(CallError),
    /// Writing the output failed.
    Output(io::Error),
    /// The command ran, and what it checked is wrong.
    Failed(String),
}

impl From<CallError> for CliError {
    fn from(error: CallError) -> Self {
        Self::Call(error)
    }
}

impl From<io::Error> for CliError {
    fn from(error: io::Error) -> Self {
        Self::Output(error)
    }
}

/// The exit code for the string code of an error of the core. A code that this version does not
/// know (a core of another version can send one) gives the general error.
pub fn code_for(code: &str) -> u8 {
    match ErrorCode::parse(code) {
        Some(ErrorCode::NotFound) => exit::NOT_FOUND,
        Some(ErrorCode::AlreadyAnswered) => exit::ALREADY_ANSWERED,
        _ => exit::ERROR,
    }
}

impl CliError {
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Usage(_) => exit::USAGE,
            Self::Call(CallError::Rpc(failure)) => code_for(&failure.code),
            Self::Call(_) | Self::Output(_) | Self::Failed(_) => exit::ERROR,
        }
    }

    /// The string code and the message of the error, with the details of an error of the core.
    fn parts(&self) -> (String, String, Value) {
        match self {
            Self::Usage(message) => ("usage".into(), message.clone(), Value::Null),
            Self::Failed(message) => ("failed".into(), message.clone(), Value::Null),
            Self::Output(e) => (
                "io".into(),
                format!("cannot write the output: {e}"),
                Value::Null,
            ),
            Self::Call(CallError::Rpc(Failure {
                code,
                message,
                details,
                ..
            })) => (code.clone(), message.clone(), details.clone()),
            Self::Call(CallError::NotRunning(path)) => (
                "not_running".into(),
                format!(
                    "neohar is not running: nothing listens at {}\n  start neohar, or choose another session with --session or --socket",
                    path.display()
                ),
                Value::Null,
            ),
            Self::Call(CallError::Timeout(time)) => (
                "timeout".into(),
                format!("the core did not answer within {} s", time.as_secs()),
                Value::Null,
            ),
            Self::Call(CallError::Io(e)) => (
                "io".into(),
                format!("the connection failed: {e}"),
                Value::Null,
            ),
            Self::Call(CallError::Protocol(message)) => {
                ("protocol".into(), message.clone(), Value::Null)
            }
        }
    }
}

/// Where the output goes, and in which form.
pub struct Output<'a> {
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
    /// Print JSON.
    pub json: bool,
    /// Stdout is a terminal, so JSON is indented.
    pub pretty: bool,
}

impl Output<'_> {
    /// Prints a JSON value.
    pub fn value(&mut self, value: &Value) -> io::Result<()> {
        let text = if self.pretty {
            serde_json::to_string_pretty(value)
        } else {
            serde_json::to_string(value)
        }
        .map_err(io::Error::other)?;
        writeln!(self.out, "{text}")
    }

    /// Prints rows of `label  value`, with the labels aligned.
    pub fn pairs(&mut self, rows: &[(&str, String)]) -> io::Result<()> {
        let width = rows
            .iter()
            .map(|(label, _)| label.chars().count())
            .max()
            .unwrap_or(0);
        for (label, value) in rows {
            writeln!(self.out, "{label:<width$}  {value}")?;
        }
        Ok(())
    }

    /// Prints a table with a header row. A column is as wide as its widest cell.
    pub fn table(&mut self, headers: &[&str], rows: &[Vec<String>]) -> io::Result<()> {
        let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
        for row in rows {
            for (column, cell) in row.iter().enumerate() {
                if let Some(width) = widths.get_mut(column) {
                    *width = (*width).max(cell.chars().count());
                }
            }
        }
        let line = |cells: &[&str]| -> String {
            let mut text = String::new();
            for (column, cell) in cells.iter().enumerate() {
                if column + 1 == cells.len() {
                    text.push_str(cell);
                } else {
                    let width = widths.get(column).copied().unwrap_or(0);
                    text.push_str(&format!("{cell:<width$}  "));
                }
            }
            text
        };
        writeln!(self.out, "{}", line(headers))?;
        for row in rows {
            let cells: Vec<&str> = row.iter().map(String::as_str).collect();
            writeln!(self.out, "{}", line(&cells))?;
        }
        Ok(())
    }

    /// Prints an error on stderr: a line for people, or JSON in the JSON mode.
    pub fn error(&mut self, error: &CliError) {
        let (code, message, details) = error.parts();
        let _ = if self.json {
            let mut body = json!({ "code": code, "message": message });
            if !details.is_null() {
                body["details"] = details;
            }
            writeln!(self.err, "{}", json!({ "error": body }))
        } else if code == "usage" {
            writeln!(self.err, "nh: {message}")
        } else if matches!(error, CliError::Call(CallError::Rpc(_))) {
            writeln!(self.err, "nh: {message} ({code})")
        } else {
            writeln!(self.err, "nh: {message}")
        };
    }
}

#[cfg(test)]
mod tests;
