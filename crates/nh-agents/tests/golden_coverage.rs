//! The golden corpus decides what a harness may do.
//!
//! `golden-coverage.toml` says which capabilities the recordings of a range of versions grant.
//! This test makes the rule "no grant without golden cases" true: every capability that the file
//! grants names the scenarios that must exist in `tests/harness-golden/` for each recorded version,
//! and each of them must be a whole recording.

use std::{fs, path::PathBuf};

use nh_agents::capabilities::{Coverage, Covered};
use semver::{Version, VersionReq};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The scenarios that a capability needs, for a harness. A grant with no entry here fails: a new
/// capability must say which recordings show it before the file can grant it.
fn required(harness: &str, capability: &str) -> Option<&'static [&'static str]> {
    Some(match (harness, capability) {
        ("claude", "observe") => &["startup", "permission-allow"],
        ("claude", "gate") => &[
            "pretool-deny",
            "pretool-ask",
            "pretool-allow-skips-dialog",
            "permission-allow",
            "permission-deny",
            "delivery-released-on-focus",
        ],
        ("claude", "native:approval") => &[
            "delivery-core-killed",
            "delivery-shim-killed",
            "delivery-two-answers",
            "delivery-answered-in-tui",
            "delivery-deadline",
        ],
        ("claude", "native:plan_review") => &["plan-allow", "plan-deny"],
        // The delivery cases of `AskUserQuestion` are not recorded yet (`golden-coverage.toml`), so
        // the file cannot grant this until they are.
        ("claude", "native:question") => &[
            "question-native",
            "question-hook",
            "delivery-question-core-killed",
            "delivery-question-two-answers",
        ],
        ("claude", "keystroke") => &["permission-allow", "delivery-dialog-while-held"],
        ("claude", "resume") => &["resume"],
        ("claude", "survive_disconnect") => &["delivery-core-killed"],
        ("codex", "observe") => &["startup", "approval-native"],
        ("codex", "gate") => &[
            "pretool-allow",
            "pretool-deny",
            "approval-hook-allow",
            "approval-hook-deny",
            "delivery-held",
            "delivery-release",
        ],
        ("codex", "native:approval") => &[
            "approval-hook-allow",
            "approval-hook-deny",
            "delivery-held",
            "delivery-release",
        ],
        ("codex", "keystroke") => &["approval-native"],
        ("codex", "resume") => &["resume"],
        ("codex", "survive_disconnect") => &["embedded"],
        _ => return None,
    })
}

/// The capabilities that a row grants, in the order of the table.
fn granted(row: &Covered) -> Vec<String> {
    let mut out = Vec::new();
    if row.observe {
        out.push("observe".to_owned());
    }
    if row.gate {
        out.push("gate".to_owned());
    }
    for kind in ["approval", "question", "plan_review"] {
        if row.answer_native.iter().any(|k| k == kind) {
            out.push(format!("native:{kind}"));
        }
    }
    if row.answer_keystroke {
        out.push("keystroke".to_owned());
    }
    if row.resume {
        out.push("resume".to_owned());
    }
    if row.survive_disconnect {
        out.push("survive_disconnect".to_owned());
    }
    out
}

/// What is wrong with the recordings behind a row. An empty list means the row is whole.
fn problems(row: &Covered) -> Vec<String> {
    let mut found = Vec::new();
    let range = match VersionReq::parse(&row.versions) {
        Ok(range) => range,
        Err(e) => {
            return vec![format!(
                "{}: the range {:?}: {e}",
                row.harness, row.versions
            )];
        }
    };
    if row.recorded.is_empty() {
        found.push(format!(
            "{} {}: a row with no recorded version grants nothing",
            row.harness, row.versions
        ));
    }
    for recorded in &row.recorded {
        match Version::parse(recorded) {
            Ok(version) if range.matches(&version) => {}
            Ok(_) => {
                found.push(format!(
                    "{} {recorded} is outside the range {}",
                    row.harness, row.versions
                ));
                continue;
            }
            Err(e) => {
                found.push(format!("{}: version {recorded:?}: {e}", row.harness));
                continue;
            }
        }
        for capability in granted(row) {
            let Some(scenarios) = required(&row.harness, &capability) else {
                found.push(format!(
                    "{} grants {capability}, and no golden cases are named for it",
                    row.harness
                ));
                continue;
            };
            for scenario in scenarios {
                let dir = root()
                    .join("tests/harness-golden")
                    .join(&row.harness)
                    .join(recorded)
                    .join(scenario);
                let shown = format!("{} {recorded} {scenario} (for {capability})", row.harness);
                if !dir.is_dir() {
                    found.push(format!("the recording is missing: {shown}"));
                    continue;
                }
                for file in ["meta.toml", "scenario.toml"] {
                    if !dir.join(file).is_file() {
                        found.push(format!("{file} is missing: {shown}"));
                    }
                }
                if !(dir.join("signals.jsonl").is_file()
                    || dir.join("session.cast").is_file()
                    || dir.join("session.jsonl").is_file())
                {
                    found.push(format!("no output of the harness is recorded: {shown}"));
                }
                // The hooks of a recording have a reviewed result (`golden_replay`).
                if dir.join("signals.jsonl").is_file() && !dir.join("expected.jsonl").is_file() {
                    found.push(format!("expected.jsonl is missing: {shown}"));
                }
                let version = fs::read_to_string(dir.join("meta.toml"))
                    .ok()
                    .and_then(|text| toml::from_str::<toml::Table>(&text).ok())
                    .and_then(|meta| meta.get("harness_version")?.as_str().map(str::to_owned));
                if version.as_deref() != Some(recorded.as_str()) {
                    found.push(format!(
                        "the recording was made with another version: {shown}"
                    ));
                }
            }
        }
    }
    found
}

#[test]
fn every_grant_names_recordings_that_exist_and_are_whole() {
    let coverage = Coverage::builtin();
    assert!(!coverage.rows().is_empty(), "the coverage has no rows");
    for row in coverage.rows() {
        let found = problems(row);
        assert!(found.is_empty(), "{}", found.join("\n"));
    }
}

fn parsed(text: &str) -> Covered {
    Coverage::parse(text)
        .expect("a coverage")
        .rows()
        .first()
        .cloned()
        .expect("a row")
}

#[test]
fn a_grant_without_its_recordings_is_refused() {
    // The answer to a question needs delivery cases that nobody recorded.
    let question = parsed(
        "[[covered]]\nharness = \"claude\"\nversions = \">=2.1.0, <2.2.0\"\nrecorded = [\"2.1.292\"]\nanswer_native = [\"question\"]\n",
    );
    let found = problems(&question).join("\n");
    assert!(found.contains("delivery-question-core-killed"), "{found}");
    // A version that nobody recorded.
    let unrecorded = parsed(
        "[[covered]]\nharness = \"claude\"\nversions = \">=2.2.0, <2.3.0\"\nrecorded = [\"2.2.0\"]\nobserve = true\n",
    );
    assert!(
        problems(&unrecorded)
            .join("\n")
            .contains("the recording is missing")
    );
    // A recorded version outside the range, and a row with no recorded version.
    let outside = parsed(
        "[[covered]]\nharness = \"codex\"\nversions = \">=0.157.0, <0.158.0\"\nrecorded = [\"0.156.1\"]\nobserve = true\n",
    );
    assert!(problems(&outside).join("\n").contains("outside the range"));
    let none = parsed(
        "[[covered]]\nharness = \"codex\"\nversions = \">=0.156.0, <0.157.0\"\nobserve = true\n",
    );
    assert!(problems(&none).join("\n").contains("no recorded version"));
}
