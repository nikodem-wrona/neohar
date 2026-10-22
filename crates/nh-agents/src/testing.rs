//! Helpers for the tests of the adapters: the hooks of the golden recordings.

use std::{fs, path::PathBuf};

use serde_json::Value;

/// The recorded hooks of a scenario: `(event, payload)` in the order of the recording.
pub fn recorded(harness: &str, version: &str, scenario: &str) -> Vec<(String, Value)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/harness-golden")
        .join(harness)
        .join(version)
        .join(scenario)
        .join("signals.jsonl");
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let row: Value = serde_json::from_str(line).expect("a JSON line");
            (
                row["event"].as_str().expect("an event").to_owned(),
                row["payload"].clone(),
            )
        })
        .collect()
}
