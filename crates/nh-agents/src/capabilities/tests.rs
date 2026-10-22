//! The resolver on the built-in manifests and on a manifest that asks for too much.

use super::*;
use crate::manifest::loader::builtin;

fn version(text: &str) -> Version {
    Version::parse(text).expect("a version")
}

struct Case {
    harness: Harness,
    version: Option<Version>,
    hooks: bool,
    bound: bool,
    screen_validates: bool,
}

impl Case {
    fn new(harness: Harness, text: &str) -> Self {
        Self {
            harness,
            version: Some(version(text)),
            hooks: true,
            bound: true,
            screen_validates: false,
        }
    }

    fn resolve(&self) -> Resolved {
        let loaded = builtin();
        let manifest = loaded.get(name(self.harness)).expect("a built-in manifest");
        resolve(
            &Context {
                manifest,
                harness: self.harness,
                version: self.version.as_ref(),
                mode: TUI,
                transports: Transports { hooks: self.hooks },
                bound: self.bound,
                screen_validates: self.screen_validates,
            },
            &Coverage::builtin(),
        )
    }
}

#[test]
fn the_coverage_that_is_compiled_in_has_a_row_for_each_manifest_range() {
    let coverage = Coverage::builtin();
    assert_eq!(coverage.rows().len(), 2);
    let loaded = builtin();
    for harness in ["claude", "codex"] {
        let manifest = loaded.get(harness).expect("a manifest");
        for range in &manifest.capabilities {
            assert!(
                coverage
                    .rows()
                    .iter()
                    .any(|row| row.harness == harness && row.versions == range.versions),
                "{harness} {}",
                range.versions
            );
        }
    }
}

#[test]
fn a_recorded_version_gets_the_capabilities_of_its_range() {
    let resolved = Case::new(Harness::Claude, "2.1.292").resolve();
    assert_eq!(resolved.unvalidated, None);
    let caps = resolved.capabilities;
    assert!(caps.observe && caps.gate && caps.answer_keystroke && caps.resume);
    assert!(caps.survive_disconnect);
    assert_eq!(
        caps.answer_native,
        vec![InteractionKind::Approval, InteractionKind::PlanReview]
    );
    assert!(!caps.reconcile && !caps.steer);
    let codex = Case::new(Harness::Codex, "0.156.1").resolve();
    assert_eq!(
        codex.capabilities.answer_native,
        vec![InteractionKind::Approval]
    );
    // The manifest says that a Codex pane does not survive; the intersection keeps that.
    assert!(!codex.capabilities.survive_disconnect);
}

#[test]
fn a_newer_version_gets_observe_only() {
    // Codex 0.162.0 is newer than the recordings, which are of 0.156.1.
    let resolved = Case::new(Harness::Codex, "0.162.0").resolve();
    assert_eq!(resolved.unvalidated, Some(Unvalidated::OutsideRanges));
    let caps = resolved.capabilities;
    assert!(caps.observe);
    assert!(!caps.gate && !caps.answer_keystroke && !caps.resume && !caps.survive_disconnect);
    assert!(caps.answer_native.is_empty());
}

#[test]
fn a_newer_version_keeps_the_keys_while_the_screen_manifest_validates() {
    let mut case = Case::new(Harness::Claude, "2.2.0");
    case.screen_validates = true;
    let caps = case.resolve().capabilities;
    assert!(caps.observe && caps.answer_keystroke);
    assert!(!caps.gate && caps.answer_native.is_empty());
}

#[test]
fn an_unknown_version_and_an_old_version_are_unvalidated() {
    let mut case = Case::new(Harness::Claude, "2.1.0");
    case.version = None;
    assert_eq!(case.resolve().unvalidated, Some(Unvalidated::NoVersion));
    let old = Case::new(Harness::Claude, "2.0.9").resolve();
    assert_eq!(old.unvalidated, Some(Unvalidated::BelowMinimum));
    assert!(!old.capabilities.gate);
}

#[test]
fn without_the_hooks_there_is_no_gate_and_no_native_answer() {
    let mut case = Case::new(Harness::Claude, "2.1.292");
    case.hooks = false;
    let caps = case.resolve().capabilities;
    assert!(caps.observe && caps.answer_keystroke && caps.resume);
    assert!(!caps.gate);
    assert!(caps.answer_native.is_empty());
}

#[test]
fn a_run_that_is_not_bound_is_never_answered() {
    let mut case = Case::new(Harness::Claude, "2.1.292");
    case.bound = false;
    let resolved = case.resolve();
    assert_eq!(resolved.unvalidated, None);
    let caps = resolved.capabilities;
    assert!(caps.observe && caps.resume);
    assert!(!caps.gate && !caps.answer_keystroke && !caps.steer);
    assert!(caps.answer_native.is_empty());
    // And it holds on an unvalidated version with a validating screen.
    let mut case = Case::new(Harness::Claude, "2.2.0");
    case.bound = false;
    case.screen_validates = true;
    assert!(!case.resolve().capabilities.answer_keystroke);
}

#[test]
fn a_manifest_of_the_user_cannot_grant_what_the_recordings_do_not_cover() {
    let dir = std::env::temp_dir().join(format!("nh-caps-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    std::fs::write(
        dir.join("greedy.toml"),
        r#"
schema = 1
id = "claude"
[[capabilities]]
versions = ">=2.1.0, <2.3.0"
mode = "tui"
observe = true
gate = true
answer_native = ["approval", "question", "plan_review"]
answer_keystroke = true
resume = true
survive_disconnect = true
"#,
    )
    .expect("a file");
    let loaded = crate::manifest::loader::load(Some(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let manifest = loaded.get("claude").expect("claude");
    let cover = |text: &str| {
        let version = version(text);
        resolve(
            &Context {
                manifest,
                harness: Harness::Claude,
                version: Some(&version),
                mode: TUI,
                transports: Transports { hooks: true },
                bound: true,
                screen_validates: false,
            },
            &Coverage::builtin(),
        )
    };
    // The merge replaces the array, so the user's range is the one in force. A recorded version
    // gets the kinds that the recordings cover, not the three that the user wrote.
    assert_eq!(
        cover("2.1.292").capabilities.answer_native,
        vec![InteractionKind::Approval, InteractionKind::PlanReview]
    );
    // A version in the range of the user, but not in any recording, is not validated.
    let later = cover("2.2.5");
    assert_eq!(later.unvalidated, Some(Unvalidated::NotCovered));
    assert!(!later.capabilities.gate);
}

#[test]
fn a_coverage_file_that_is_not_right_is_an_error() {
    assert!(Coverage::parse("covered = 3").is_err());
    let coverage = Coverage::parse("").expect("an empty file");
    assert!(coverage.rows().is_empty());
}
