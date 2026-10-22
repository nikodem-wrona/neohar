//! `meta.toml`: what a recording is, and on what machine it was made.

use std::{fs, path::Path, process::Command};

use nh_proto::holder::ExitStatus;
use serde::{Deserialize, Serialize};

use crate::GoldenError;

fn default_mode() -> String {
    "pty".to_owned()
}

/// The file `meta.toml` of a recording.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub harness: String,
    pub harness_version: String,
    pub scenario: String,
    /// The BLAKE3 hash of the bytes of the scenario file.
    pub scenario_blake3: String,
    /// `pty` (a terminal) or `pipe` (JSON lines).
    #[serde(default = "default_mode")]
    pub mode: String,
    /// The operating system, such as `macos 26.6.2`.
    pub os: String,
    pub arch: String,
    pub cols: u16,
    pub rows: u16,
    /// How long the recording ran, in milliseconds. It differs from run to run.
    pub duration_ms: u64,
    /// How the harness ended: `code 0`, `signal 9`, or `still running` when the scenario ended
    /// first and the recorder stopped it.
    pub exit: String,
    /// The requests that the mock model got, and how many of them offered tools.
    pub llm_requests: u64,
    pub llm_main_requests: u64,
    pub turns_used: usize,
    pub turns_total: usize,
    /// Main requests that no turn fitted. The scenario should have none.
    pub unmatched_main: u64,
    /// Hooks that wrote a line to `signals.jsonl`, and hooks that reached the API socket.
    pub hook_calls: usize,
    pub hook_calls_api: usize,
    /// The replies that the recorder wrote to terminal queries of the harness.
    pub terminal_replies: usize,
    /// The names of the screens in `screens/`, in the order of the scenario.
    pub screens: Vec<String>,
    /// The names of the process lists in `processes/`, in the order of the scenario.
    #[serde(default)]
    pub processes: Vec<String>,
}

/// The BLAKE3 hash of `bytes` as text.
pub fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// `macos 26.6.2` or the plain name of the operating system.
pub fn os_description() -> String {
    let name = std::env::consts::OS;
    let version = Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty());
    match version {
        Some(version) => format!("{name} {version}"),
        None => name.to_owned(),
    }
}

/// How an exit is written in `meta.toml`.
pub fn describe_exit(exit: Option<ExitStatus>) -> String {
    match exit {
        None => "still running".to_owned(),
        Some(ExitStatus::Code(code)) => format!("code {code}"),
        Some(ExitStatus::Signal(signal)) => format!("signal {signal}"),
        Some(ExitStatus::ExecFailed(errno)) => format!("exec failed {errno}"),
    }
}

impl Meta {
    pub fn to_toml(&self) -> Result<String, GoldenError> {
        toml::to_string_pretty(self).map_err(|e| GoldenError::Setup(e.to_string()))
    }

    pub fn read(dir: &Path) -> Result<Self, GoldenError> {
        let path = dir.join("meta.toml");
        let text = fs::read_to_string(&path)
            .map_err(GoldenError::io(format!("read {}", path.display())))?;
        toml::from_str(&text)
            .map_err(|e| GoldenError::Setup(format!("{} is not valid: {e}", path.display())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn sample() -> Meta {
        Meta {
            harness: "claude".into(),
            harness_version: "2.1.292".into(),
            scenario: "startup".into(),
            scenario_blake3: blake3_hex(b"x"),
            mode: "pty".into(),
            os: "macos 26.6.2".into(),
            arch: "aarch64".into(),
            cols: 120,
            rows: 40,
            duration_ms: 9000,
            exit: describe_exit(Some(ExitStatus::Code(0))),
            llm_requests: 2,
            llm_main_requests: 1,
            turns_used: 1,
            turns_total: 1,
            unmatched_main: 0,
            hook_calls: 3,
            hook_calls_api: 3,
            terminal_replies: 4,
            screens: vec!["prompt".into(), "final".into()],
            processes: vec!["idle".into()],
        }
    }

    #[test]
    fn meta_round_trips_through_toml() {
        let meta = sample();
        let text = meta.to_toml().expect("toml");
        assert!(text.contains("harness_version = \"2.1.292\""), "{text}");
        let back: Meta = toml::from_str(&text).expect("parse");
        assert_eq!(back, meta);
    }

    #[test]
    fn exits_are_described_in_words() {
        assert_eq!(describe_exit(None), "still running");
        assert_eq!(describe_exit(Some(ExitStatus::Code(1))), "code 1");
        assert_eq!(describe_exit(Some(ExitStatus::Signal(9))), "signal 9");
    }

    #[test]
    fn a_hash_is_64_hex_digits() {
        let hash = blake3_hex(b"abc");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
