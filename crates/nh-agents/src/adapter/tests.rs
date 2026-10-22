//! The adapter layer on table cases and on the hooks of the golden recordings.

use std::path::PathBuf;

use nh_proto::api::{
    common::{Harness, Source},
    ids::RunId,
    model::{Execution, Liveness},
    time::Timestamp,
};
use serde_json::{Value, json};

use super::{
    effects::{Effect, effects},
    hooks::ManifestAdapter,
    *,
};
use crate::{
    arbiter::{Arbiter, Input},
    harness::name,
    manifest::loader::builtin,
};

fn run_id() -> RunId {
    "01M4F1W7EM2GN3EV6NHXZJEGGA".parse().expect("an id")
}

/// An adapter that started for a harness, and the receiver of what it says.
async fn started(harness: Harness) -> (ManifestAdapter, mpsc::Receiver<AdapterSignal>) {
    let loaded = builtin();
    let manifest = loaded.get(name(harness)).expect("a manifest").clone();
    let (emitter, receiver) = Emitter::channel();
    let mut adapter = ManifestAdapter::new(&manifest);
    adapter
        .start(AdapterCtx {
            run: run_id(),
            pane: None,
            harness,
            manifest: Arc::new(manifest),
            emitter,
        })
        .await
        .expect("starts");
    (adapter, receiver)
}

fn raw(event: &str, payload: Value) -> AdapterSignal {
    AdapterSignal::Raw {
        harness_event: event.to_owned(),
        payload,
    }
}

fn drain(receiver: &mpsc::Receiver<AdapterSignal>) -> Vec<AdapterSignal> {
    receiver.try_iter().collect()
}

fn kinds(signals: &[AdapterSignal]) -> Vec<&'static str> {
    signals
        .iter()
        .map(|signal| match signal {
            AdapterSignal::SessionStarted { .. } => "session_started",
            AdapterSignal::TurnStarted { .. } => "turn_started",
            AdapterSignal::TurnEnded { .. } => "turn_ended",
            AdapterSignal::Error { .. } => "error",
            AdapterSignal::SessionEnded { .. } => "session_ended",
            AdapterSignal::Raw { .. } => "raw",
            _ => "other",
        })
        .collect()
}

// --- effects --------------------------------------------------------------------------------

fn observe(effects: &[Effect]) -> Vec<&Observation> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Observe(observation) => Some(observation),
            Effect::Identify { .. } | Effect::PermissionMode { .. } => None,
        })
        .collect()
}

use crate::arbiter::Observation;

#[test]
fn a_session_that_starts_names_the_run_and_makes_it_idle() {
    let signal = AdapterSignal::SessionStarted {
        session_id: "s1".to_owned(),
        transcript_path: Some(PathBuf::from("/t/s1.jsonl")),
        source: StartSource::Startup,
        model: Some("opus".to_owned()),
    };
    let got = effects(&signal);
    assert_eq!(
        got[0],
        Effect::Identify {
            session_id: "s1".to_owned(),
            transcript_path: Some("/t/s1.jsonl".to_owned()),
            model: Some("opus".to_owned()),
        }
    );
    assert_eq!(observe(&got), [&Observation::Execution(Execution::Idle)]);
}

#[test]
fn a_session_after_a_compaction_says_nothing_about_the_state() {
    for source in [StartSource::Compact, StartSource::Other("x".to_owned())] {
        let got = effects(&AdapterSignal::SessionStarted {
            session_id: "s1".to_owned(),
            transcript_path: None,
            source,
            model: None,
        });
        assert_eq!(got.len(), 1, "only the identity");
    }
    for source in [StartSource::Resume, StartSource::Clear] {
        let got = effects(&AdapterSignal::SessionStarted {
            session_id: "s1".to_owned(),
            transcript_path: None,
            source,
            model: None,
        });
        assert_eq!(observe(&got).len(), 1);
    }
}

#[test]
fn the_other_signals_map_to_the_states_of_the_spec() {
    let working = [
        AdapterSignal::TurnStarted {
            turn_ref: None,
            prompt_preview: None,
        },
        AdapterSignal::ToolStarted {
            call_id: "c".to_owned(),
            tool: "Bash".to_owned(),
            input: json!({}),
        },
        AdapterSignal::SubagentStarted {
            native_id: "a".to_owned(),
            kind: None,
        },
        AdapterSignal::Compacting {
            phase: Phase::Start,
        },
    ];
    for signal in working {
        assert_eq!(
            observe(&effects(&signal)),
            [&Observation::Execution(Execution::Working)],
            "{signal:?}"
        );
    }
    assert_eq!(
        observe(&effects(&AdapterSignal::TurnEnded {
            turn_ref: None,
            stop_reason: None,
            last_message_preview: None
        })),
        [&Observation::TurnEnded]
    );
    assert_eq!(
        observe(&effects(&AdapterSignal::Error {
            kind: "x".to_owned(),
            message: "m".to_owned(),
            retrying: false
        })),
        [&Observation::Execution(Execution::Error)]
    );
    assert_eq!(
        observe(&effects(&AdapterSignal::RateLimited {
            resets_at: Some(Timestamp(5)),
            scope: None
        })),
        [&Observation::Execution(Execution::RateLimited {
            resets_at: Some(Timestamp(5))
        })]
    );
    assert_eq!(
        observe(&effects(&AdapterSignal::SessionEnded {
            reason: "logout".to_owned()
        })),
        [&Observation::SessionEnded]
    );
}

#[test]
fn signals_that_change_no_state_have_no_effect() {
    for signal in [
        AdapterSignal::Error {
            kind: "x".to_owned(),
            message: "retry".to_owned(),
            retrying: true,
        },
        AdapterSignal::SessionEnded {
            reason: "clear".to_owned(),
        },
        AdapterSignal::Compacting { phase: Phase::End },
        AdapterSignal::ToolEnded {
            call_id: "c".to_owned(),
            tool: "Bash".to_owned(),
            ok: true,
            summary: None,
        },
        AdapterSignal::SubagentEnded {
            native_id: "a".to_owned(),
        },
        AdapterSignal::Notice {
            kind: NoticeKind::Info,
            message: "n".to_owned(),
        },
        raw("PostToolUse", json!({})),
    ] {
        assert!(effects(&signal).is_empty(), "{signal:?}");
    }
}

// --- the manifest adapter ---------------------------------------------------------------------

#[tokio::test]
async fn the_manifest_adapter_reports_a_session_once_and_the_states_of_the_table() {
    let (mut adapter, receiver) = started(Harness::Claude).await;
    let session = json!({"session_id": "s1", "transcript_path": "/t/s1.jsonl", "model": "opus", "source": "startup"});
    adapter
        .on_signal(raw("SessionStart", session))
        .await
        .expect("ok");
    assert_eq!(kinds(&drain(&receiver)), ["session_started", "raw"]);
    adapter
        .on_signal(raw(
            "UserPromptSubmit",
            json!({"session_id": "s1", "prompt_id": "p1"}),
        ))
        .await
        .expect("ok");
    assert_eq!(kinds(&drain(&receiver)), ["turn_started", "raw"]);
    adapter
        .on_signal(raw("Stop", json!({"session_id": "s1"})))
        .await
        .expect("ok");
    assert_eq!(kinds(&drain(&receiver)), ["turn_ended", "raw"]);
    // Another session id in the same adapter is `/clear` or `/resume`.
    adapter
        .on_signal(raw(
            "SessionStart",
            json!({"session_id": "s2", "source": "clear"}),
        ))
        .await
        .expect("ok");
    let again = drain(&receiver);
    assert_eq!(kinds(&again), ["session_started", "raw"]);
    assert!(
        matches!(&again[0], AdapterSignal::SessionStarted { session_id, source: StartSource::Clear, .. } if session_id == "s2")
    );
}

#[tokio::test]
async fn an_event_that_the_table_calls_an_interaction_adds_no_state_and_gets_no_reply() {
    let (mut adapter, receiver) = started(Harness::Claude).await;
    let reply = adapter
        .on_signal(raw(
            "PermissionRequest",
            json!({"session_id": "s1", "tool_name": "Bash"}),
        ))
        .await
        .expect("ok");
    assert_eq!(reply, SignalReply::Continue);
    // The session and the raw event; no state.
    assert_eq!(kinds(&drain(&receiver)), ["session_started", "raw"]);
}

#[tokio::test]
async fn a_failure_of_the_harness_and_the_end_of_the_session_are_reported() {
    let (mut adapter, receiver) = started(Harness::Claude).await;
    adapter
        .on_signal(raw(
            "StopFailure",
            json!({"session_id": "s1", "error": "rate_limit"}),
        ))
        .await
        .expect("ok");
    let got = drain(&receiver);
    assert!(got.iter().any(|s| matches!(s, AdapterSignal::Error { message, retrying: false, .. } if message == "rate_limit")));
    adapter
        .on_signal(raw(
            "SessionEnd",
            json!({"session_id": "s1", "reason": "logout"}),
        ))
        .await
        .expect("ok");
    let got = drain(&receiver);
    assert!(
        got.iter()
            .any(|s| matches!(s, AdapterSignal::SessionEnded { reason } if reason == "logout"))
    );
}

#[tokio::test]
async fn an_event_that_is_not_raw_is_ignored() {
    let (mut adapter, receiver) = started(Harness::Codex).await;
    let reply = adapter
        .on_signal(AdapterSignal::Notice {
            kind: NoticeKind::Info,
            message: "n".to_owned(),
        })
        .await
        .expect("ok");
    assert_eq!(reply, SignalReply::Continue);
    assert!(drain(&receiver).is_empty());
}

use crate::testing::recorded;

/// Plays a recording through the adapter and the arbiter, and gives the states in order.
async fn play(harness: Harness, signals: Vec<(String, Value)>) -> (Vec<String>, Arbiter) {
    let (mut adapter, receiver) = started(harness).await;
    let mut arbiter = Arbiter::new(Timestamp(0), Source::Process);
    let mut states = Vec::new();
    let mut at = 1;
    for (event, payload) in signals {
        adapter.on_signal(raw(&event, payload)).await.expect("ok");
        for signal in drain(&receiver) {
            for effect in effects(&signal) {
                if let Effect::Observe(observation) = effect {
                    at += 1;
                    arbiter.apply(Input::new(Source::Structured, Timestamp(at), observation));
                    let state = serde_json::to_value(&arbiter.execution().value).expect("json");
                    states.push(state["state"].as_str().unwrap_or("?").to_owned());
                }
            }
        }
    }
    (states, arbiter)
}

#[tokio::test]
async fn a_recorded_session_of_claude_code_goes_through_the_states() {
    let signals = recorded("claude", "2.1.292", "permission-allow");
    assert!(signals.len() >= 4, "{signals:?}");
    let (states, arbiter) = play(Harness::Claude, signals).await;
    // The session starts idle, a prompt and a tool make it work, `Stop` makes it idle, and
    // `SessionEnd` ends the session. The process lives: only the holder says that it ended.
    assert_eq!(states, ["idle", "working", "working", "idle", "exited"]);
    assert_eq!(arbiter.liveness().value, Liveness::Alive);
    assert_eq!(arbiter.execution().provenance.source, Source::Structured);
}

#[tokio::test]
async fn a_recorded_session_of_codex_goes_through_the_states() {
    let signals = recorded("codex", "0.156.1", "approval-hook-allow");
    let (states, arbiter) = play(Harness::Codex, signals).await;
    assert_eq!(states, ["idle", "working", "working", "idle", "exited"]);
    assert_eq!(arbiter.liveness().value, Liveness::Alive);
}
