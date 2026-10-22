//! The adapter of Codex on the hooks recorded for 0.156.1.

use std::sync::{Arc, mpsc};

use nh_proto::api::{
    common::Harness,
    ids::RunId,
    model::{GateMode, InteractionKind},
};
use serde_json::{Value, json};

use super::*;
use crate::{
    adapter::{
        AdapterCtx, Emitter, StartSource,
        effects::{Effect, effects},
    },
    manifest::loader::builtin,
    testing::recorded,
};

fn run_id() -> RunId {
    "01M4F1W7EM2GN3EV6NHXZJEGGA".parse().expect("an id")
}

async fn started() -> (CodexHooks, mpsc::Receiver<AdapterSignal>) {
    let manifest = builtin().get("codex").expect("codex").clone();
    let (emitter, receiver) = Emitter::channel();
    let mut adapter = CodexHooks::new();
    adapter
        .start(AdapterCtx {
            run: run_id(),
            pane: None,
            harness: Harness::Codex,
            manifest: Arc::new(manifest),
            emitter,
        })
        .await
        .expect("starts");
    (adapter, receiver)
}

async fn fire(
    adapter: &mut CodexHooks,
    receiver: &mpsc::Receiver<AdapterSignal>,
    event: &str,
    payload: Value,
) -> Vec<AdapterSignal> {
    let reply = adapter
        .on_signal(AdapterSignal::Raw {
            harness_event: event.to_owned(),
            payload,
        })
        .await
        .expect("a reply");
    assert_eq!(reply, SignalReply::Continue, "{event}");
    receiver
        .try_iter()
        .filter(|signal| !matches!(signal, AdapterSignal::Raw { .. }))
        .collect()
}

async fn play(scenario: &str) -> Vec<(String, Vec<AdapterSignal>)> {
    let (mut adapter, receiver) = started().await;
    let mut out = Vec::new();
    for (event, payload) in recorded("codex", "0.156.1", scenario) {
        let signals = fire(&mut adapter, &receiver, &event, payload).await;
        out.push((event, signals));
    }
    out
}

fn of<'a>(played: &'a [(String, Vec<AdapterSignal>)], event: &str) -> &'a [AdapterSignal] {
    &played
        .iter()
        .find(|(name, _)| name == event)
        .unwrap_or_else(|| panic!("no {event}"))
        .1
}

fn kinds(signals: &[AdapterSignal]) -> Vec<&'static str> {
    signals
        .iter()
        .map(|signal| match signal {
            AdapterSignal::SessionStarted { .. } => "session_started",
            AdapterSignal::TurnStarted { .. } => "turn_started",
            AdapterSignal::TurnEnded { .. } => "turn_ended",
            AdapterSignal::ToolStarted { .. } => "tool_started",
            AdapterSignal::ToolEnded { .. } => "tool_ended",
            AdapterSignal::InteractionOpen { .. } => "interaction_open",
            AdapterSignal::InteractionResolved { .. } => "interaction_resolved",
            AdapterSignal::PermissionMode { .. } => "permission_mode",
            AdapterSignal::SessionEnded { .. } => "session_ended",
            AdapterSignal::Compacting { .. } => "compacting",
            AdapterSignal::SubagentStarted { .. } => "subagent_started",
            AdapterSignal::SubagentEnded { .. } => "subagent_ended",
            _ => "other",
        })
        .collect()
}

#[tokio::test]
async fn the_session_names_the_thread_with_its_rollout_and_model() {
    let played = play("approval-hook-allow").await;
    let start = of(&played, "SessionStart");
    assert_eq!(kinds(start), ["session_started", "permission_mode"]);
    let AdapterSignal::SessionStarted {
        session_id,
        transcript_path,
        source,
        model,
    } = &start[0]
    else {
        panic!("{start:?}")
    };
    assert!(session_id.len() >= 32);
    assert!(
        transcript_path
            .as_ref()
            .is_some_and(|p| p.to_string_lossy().contains("rollout-"))
    );
    assert_eq!(*source, StartSource::Startup);
    assert_eq!(model.as_deref(), Some("mock-model"));
}

#[tokio::test]
async fn a_prompt_a_command_and_the_end_of_the_turn() {
    let played = play("approval-hook-allow").await;
    let prompt = of(&played, "UserPromptSubmit");
    let AdapterSignal::TurnStarted {
        turn_ref,
        prompt_preview,
    } = &prompt[0]
    else {
        panic!("{prompt:?}")
    };
    assert_eq!(prompt_preview.as_deref(), Some("make a note"));
    assert!(turn_ref.as_deref().is_some_and(|id| id.len() > 10));
    let call = of(&played, "PreToolUse");
    let AdapterSignal::ToolStarted {
        call_id,
        tool,
        input,
    } = &call[0]
    else {
        panic!("{call:?}")
    };
    assert_eq!(tool, "Bash");
    assert_eq!(input["command"], "touch note.txt");
    assert!(call_id.starts_with("call_"), "{call_id}");
    let stop = of(&played, "Stop");
    assert_eq!(kinds(stop), ["turn_ended"]);
    assert_eq!(kinds(of(&played, "SessionEnd")), ["session_ended"]);
}

#[tokio::test]
async fn the_request_for_permission_takes_the_ref_of_the_command_before_it_and_the_end_resolves_it()
{
    let played = play("approval-hook-allow").await;
    let AdapterSignal::ToolStarted { call_id, .. } = &of(&played, "PreToolUse")[0] else {
        panic!()
    };
    let request = of(&played, "PermissionRequest");
    let AdapterSignal::InteractionOpen {
        kind,
        native_ref,
        payload,
        gate,
    } = &request[0]
    else {
        panic!("{request:?}")
    };
    assert_eq!(*kind, InteractionKind::Approval);
    assert_eq!(native_ref, call_id);
    assert_eq!(*gate, GateMode::Gate);
    assert_eq!(payload["command"], "touch note.txt");
    assert!(payload["description"].is_string());
    let done = of(&played, "PostToolUse");
    assert_eq!(kinds(done), ["tool_ended", "interaction_resolved"]);
    assert!(
        matches!(&done[1], AdapterSignal::InteractionResolved { resolution: Resolution::Allowed, native_ref: r } if r == call_id)
    );
}

#[tokio::test]
async fn a_denied_command_leaves_its_request_open_until_the_turn_ends() {
    // In the recordings, no `PostToolUse` follows a deny.
    let played = play("approval-hook-deny").await;
    assert_eq!(
        kinds(of(&played, "PermissionRequest")),
        ["interaction_open"]
    );
    assert!(!played.iter().any(|(event, _)| event == "PostToolUse"));
    assert_eq!(kinds(of(&played, "Stop")), ["turn_ended"]);
}

#[tokio::test]
async fn under_the_unrestricted_mode_the_run_is_yolo() {
    let played = play("yolo-hooks").await;
    assert!(!played.iter().any(|(event, _)| event == "PermissionRequest"));
    let mode = played
        .iter()
        .flat_map(|(_, signals)| signals)
        .find(|signal| matches!(signal, AdapterSignal::PermissionMode { .. }))
        .expect("a mode");
    assert!(matches!(mode, AdapterSignal::PermissionMode { mode } if mode == "bypassPermissions"));
    assert_eq!(effects(mode), [Effect::PermissionMode { yolo: true }]);
}

#[tokio::test]
async fn the_interrupt_ends_the_turn_and_closes_what_the_adapter_held() {
    let (mut adapter, receiver) = started().await;
    let input = json!({"command": "rm x"});
    fire(
        &mut adapter,
        &receiver,
        "PreToolUse",
        json!({"session_id": "s", "tool_name": "Bash", "tool_use_id": "c1", "tool_input": input}),
    )
    .await;
    fire(
        &mut adapter,
        &receiver,
        "PermissionRequest",
        json!({"session_id": "s", "tool_name": "Bash", "tool_input": input}),
    )
    .await;
    let got = fire(
        &mut adapter,
        &receiver,
        "Interrupt",
        json!({"session_id": "s", "turn_id": "t1"}),
    )
    .await;
    assert!(
        matches!(&got[0], AdapterSignal::TurnEnded { stop_reason: Some(reason), turn_ref: Some(turn) , .. } if reason == "interrupt" && turn == "t1")
    );
    // The adapter let go of the request, so a late end of the call resolves nothing.
    let late = fire(
        &mut adapter,
        &receiver,
        "PostToolUse",
        json!({"session_id": "s", "tool_name": "Bash", "tool_use_id": "c1", "tool_input": input}),
    )
    .await;
    assert_eq!(kinds(&late), ["tool_ended"]);
}

#[tokio::test]
async fn subagents_and_compaction_are_the_same_as_for_claude_code() {
    let (mut adapter, receiver) = started().await;
    let start = fire(
        &mut adapter,
        &receiver,
        "SubagentStart",
        json!({"session_id": "s", "agent_id": "a1", "agent_type": "worker"}),
    )
    .await;
    assert!(
        matches!(start.last(), Some(AdapterSignal::SubagentStarted { native_id, .. }) if native_id == "a1")
    );
    let stop = fire(
        &mut adapter,
        &receiver,
        "SubagentStop",
        json!({"session_id": "s", "agent_id": "a1"}),
    )
    .await;
    assert_eq!(kinds(&stop), ["subagent_ended"]);
    let pre = fire(
        &mut adapter,
        &receiver,
        "PreCompact",
        json!({"session_id": "s"}),
    )
    .await;
    assert!(matches!(
        pre[0],
        AdapterSignal::Compacting {
            phase: Phase::Start
        }
    ));
    let post = fire(
        &mut adapter,
        &receiver,
        "PostCompact",
        json!({"session_id": "s"}),
    )
    .await;
    assert!(matches!(
        post[0],
        AdapterSignal::Compacting { phase: Phase::End }
    ));
}

#[tokio::test]
async fn the_resume_handle_is_codex_resume_and_the_capabilities_have_no_plan_or_question() {
    let (mut adapter, receiver) = started().await;
    assert_eq!(adapter.resume_handle(), None);
    fire(
        &mut adapter,
        &receiver,
        "SessionStart",
        json!({"session_id": "th-1", "source": "startup"}),
    )
    .await;
    assert_eq!(
        adapter.resume_handle(),
        Some(vec![
            "codex".to_owned(),
            "resume".to_owned(),
            "th-1".to_owned()
        ])
    );
    let caps = adapter.capabilities();
    assert_eq!(caps.answer_native, [InteractionKind::Approval]);
    assert!(caps.gate && caps.observe);
}

#[tokio::test]
async fn the_end_of_an_old_thread_does_not_end_the_run() {
    // Codex 0.163: the interface moved to thread B, then Codex shut down the idle thread A while
    // the process ran.
    let (mut adapter, receiver) = started().await;
    fire(
        &mut adapter,
        &receiver,
        "SessionStart",
        json!({"session_id": "th-a", "source": "startup"}),
    )
    .await;
    let moved = fire(
        &mut adapter,
        &receiver,
        "UserPromptSubmit",
        json!({"session_id": "th-b", "turn_id": "t1", "prompt": "next"}),
    )
    .await;
    assert_eq!(kinds(&moved), ["session_started", "turn_started"]);
    adapter
        .on_signal(AdapterSignal::Raw {
            harness_event: "SessionEnd".to_owned(),
            payload: json!({"session_id": "th-a", "reason": "other"}),
        })
        .await
        .expect("a reply");
    let old: Vec<AdapterSignal> = receiver.try_iter().collect();
    assert!(
        matches!(old.as_slice(), [AdapterSignal::Raw { harness_event, .. }] if harness_event == "SessionEnd"),
        "only the raw signal: {old:?}"
    );
    assert_eq!(
        adapter.resume_handle(),
        Some(vec![
            "codex".to_owned(),
            "resume".to_owned(),
            "th-b".to_owned()
        ])
    );
    let end = fire(
        &mut adapter,
        &receiver,
        "SessionEnd",
        json!({"session_id": "th-b", "reason": "other"}),
    )
    .await;
    assert_eq!(kinds(&end), ["session_ended"]);
}

#[tokio::test]
async fn a_recorded_session_goes_through_the_states_of_the_arbiter() {
    use crate::arbiter::{Arbiter, Input};
    use nh_proto::api::{common::Source, time::Timestamp};
    let (mut adapter, receiver) = started().await;
    let mut arbiter = Arbiter::new(Timestamp(0), Source::Process);
    let mut states = Vec::new();
    let mut at = 1;
    for (event, payload) in recorded("codex", "0.156.1", "approval-hook-allow") {
        fire_into(&mut adapter, &receiver, &event, payload, &mut |signal| {
            for effect in effects(signal) {
                if let Effect::Observe(observation) = effect {
                    at += 1;
                    arbiter.apply(Input::new(Source::Structured, Timestamp(at), observation));
                    let state = serde_json::to_value(&arbiter.execution().value).expect("json");
                    states.push(state["state"].as_str().unwrap_or("?").to_owned());
                }
            }
        })
        .await;
    }
    assert_eq!(states, ["idle", "working", "working", "idle", "exited"]);
}

async fn fire_into(
    adapter: &mut CodexHooks,
    receiver: &mpsc::Receiver<AdapterSignal>,
    event: &str,
    payload: Value,
    sink: &mut dyn FnMut(&AdapterSignal),
) {
    for signal in fire(adapter, receiver, event, payload).await {
        sink(&signal);
    }
}
