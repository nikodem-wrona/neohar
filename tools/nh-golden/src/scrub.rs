//! What a recording keeps of the requests and the transcripts of a harness.
//!
//! A harness sends its system prompt and the descriptions of its tools in each request, and
//! Claude Code also writes them into its transcript. That text belongs to the vendor of the
//! harness, and no test reads it, so a recording does not keep it:
//!
//! - `llm-requests.jsonl` keeps the fields that [`crate::compare`] reads. The body and the headers
//!   of each request go.
//! - In a transcript, an `attachment` line keeps only the type of its attachment and loses its
//!   `rendered` text. Claude Code puts the system prompt, the tools, the reminders and the lists
//!   of skills and agents there. The `api-request*` lines, which are copies of the request, go.
//!   The parsers of the adapters read neither.
//!
//! [`crate::driver::Recording::write`] applies these rules. `nh-golden scrub` applies them to the
//! recordings that are already in a directory.

use std::{fs, path::Path};

use serde_json::{Map, Value};

use crate::GoldenError;

/// The fields of a request that a recording does not keep.
const REQUEST_DROPS: [&str; 2] = ["body", "headers"];

/// The lines of `llm-requests.jsonl` without the fields in [`REQUEST_DROPS`]. A line that is not
/// a JSON object stays as it is.
pub fn requests(text: &str) -> String {
    rewrite(text, |mut line| {
        let mut changed = false;
        for key in REQUEST_DROPS {
            changed |= line.remove(key).is_some();
        }
        if changed {
            Line::Write(line)
        } else {
            Line::Keep
        }
    })
}

/// The lines of a transcript without the text of the harness: an `attachment` keeps only its
/// `type` and loses its `rendered` text, and the `api-request*` lines go. The other lines stay
/// byte for byte.
pub fn transcript(text: &str) -> String {
    rewrite(text, |mut line| {
        let kind = line.get("type").and_then(Value::as_str).unwrap_or_default();
        if kind.starts_with("api-request") {
            return Line::Drop;
        }
        if kind != "attachment" {
            return Line::Keep;
        }
        let mut changed = line.remove("rendered").is_some();
        if let Some(Value::Object(attachment)) = line.get_mut("attachment")
            && !(attachment.len() == 1 && attachment.contains_key("type"))
        {
            attachment.retain(|key, _| key == "type");
            changed = true;
        }
        if changed {
            Line::Write(line)
        } else {
            Line::Keep
        }
    })
}

/// What a rule does with one line.
enum Line {
    /// The line goes.
    Drop,
    /// The text of the line stays as it is.
    Keep,
    /// The changed line replaces it.
    Write(Map<String, Value>),
}

/// Applies `change` to each line that is a JSON object. The other lines stay.
fn rewrite(text: &str, change: impl Fn(Map<String, Value>) -> Line) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let kept = match serde_json::from_str::<Value>(line) {
            Ok(Value::Object(object)) => match change(object) {
                Line::Drop => continue,
                Line::Keep => line.to_owned(),
                Line::Write(object) => Value::Object(object).to_string(),
            },
            _ => line.to_owned(),
        };
        out.push_str(&kept);
        out.push('\n');
    }
    if !text.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Applies the rules to every recording under `root`: each directory that has a `meta.toml`.
/// Returns the number of files that changed.
pub fn tree(root: &Path) -> Result<usize, GoldenError> {
    let mut changed = 0;
    if root.join("meta.toml").is_file() {
        changed += file(&root.join("llm-requests.jsonl"), requests)?;
        let transcripts = root.join("transcripts");
        if transcripts.is_dir() {
            for entry in read_dir(&transcripts)? {
                if entry.extension().is_some_and(|e| e == "jsonl") {
                    changed += file(&entry, transcript)?;
                }
            }
        }
    }
    for entry in read_dir(root)? {
        if entry.is_dir() && entry.file_name().is_some_and(|n| n != "transcripts") {
            changed += tree(&entry)?;
        }
    }
    Ok(changed)
}

fn read_dir(dir: &Path) -> Result<Vec<std::path::PathBuf>, GoldenError> {
    let mut paths = fs::read_dir(dir)
        .map_err(GoldenError::io(format!("read {}", dir.display())))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

/// Rewrites one file with `rule`. A missing file is not an error. Returns 1 when the file changed.
fn file(path: &Path, rule: fn(&str) -> String) -> Result<usize, GoldenError> {
    let Ok(text) = fs::read_to_string(path) else {
        return Ok(0);
    };
    let scrubbed = rule(&text);
    if scrubbed == text {
        return Ok(0);
    }
    fs::write(path, scrubbed).map_err(GoldenError::io(format!("write {}", path.display())))?;
    Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_keep_what_compare_reads() {
        let text = concat!(
            r#"{"api":"anthropic","body":{"system":"secret"},"headers":{"a":"b"},"last_user":"hi","main":true,"method":"POST","model":"m","path":"/v1/messages","seq":1,"stream":true,"turn":0}"#,
            "\n",
            "not json\n",
        );
        let out = requests(text);
        assert!(!out.contains("secret"));
        assert!(!out.contains("headers"));
        let first: Value = serde_json::from_str(out.lines().next().expect("a line")).expect("JSON");
        for key in [
            "api",
            "last_user",
            "main",
            "method",
            "model",
            "path",
            "stream",
            "turn",
        ] {
            assert!(first.get(key).is_some(), "{key} is gone");
        }
        assert_eq!(out.lines().nth(1), Some("not json"));
        assert_eq!(requests(&out), out, "a second scrub changes nothing");
    }

    #[test]
    fn transcripts_lose_attachments_and_request_copies() {
        let kept = r#"{"type":"user","uuid":"u1","message":{"role":"user","content":"hi"}}"#;
        let text = [
            kept,
            r#"{"type":"attachment","uuid":"a1","parentUuid":"u1","attachment":{"type":"prompt_snapshot","systemPrompt":"secret","tools":["secret"]}}"#,
            r#"{"type":"attachment","uuid":"a2","parentUuid":"a1","attachment":{"type":"agent_listing_delta"},"rendered":[{"content":"secret"}]}"#,
            r#"{"type":"api-request-shape","shape":"secret"}"#,
            r#"{"type":"api-request-blob","message":"secret"}"#,
        ]
        .join("\n")
            + "\n";
        let out = transcript(&text);
        assert!(!out.contains("secret"));
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], kept, "other lines stay byte for byte");
        let stub: Value = serde_json::from_str(lines[1]).expect("JSON");
        assert_eq!(
            stub["attachment"],
            serde_json::json!({"type": "prompt_snapshot"})
        );
        assert_eq!(
            stub["parentUuid"], "u1",
            "the chain of the transcript stays"
        );
        assert_eq!(transcript(&out), out, "a second scrub changes nothing");
    }
}
