//! The adapter of Claude Code on the hooks recorded for 2.1.292, and on the events that the
//! recordings do not have.

use std::sync::{Arc, mpsc};

use nh_proto::api::{
    common::Harness,
    ids::RunId,
    model::{GateMode, InteractionKind},
};
use serde_json::{Value, json};

use super::*;
use crate::adapter::{FileOp, StartSource};
use crate::{
    adapter::{
        AdapterCtx, Emitter,
        effects::{Effect, effects},
    },
    manifest::loader::builtin,
    testing::recorded,
};

fn run_id() -> RunId {
    "01M4F1W7EM2GN3EV6NHXZJEGGA".parse().expect("an id")
}

async fn started() -> (ClaudeHooks, mpsc::Receiver<AdapterSignal>) {
    let manifest = builtin().get("claude").expect("claude").clone();
    let (emitter, receiver) = Emitter::channel();
    let mut adapter = ClaudeHooks::new();
    adapter
        .start(AdapterCtx {
            run: run_id(),
            pane: None,
            harness: Harness::Claude,
            manifest: Arc::new(manifest),
            emitter,
        })
        .await
        .expect("starts");
    (adapter, receiver)
}

/// Fires one hook and gives the signals that the adapter reported, without the `Raw` copy.
async fn fire(
    adapter: &mut ClaudeHooks,
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

/// Plays a whole recording and gives the signals of every hook, with the event that made them.
async fn play(scenario: &str) -> Vec<(String, Vec<AdapterSignal>)> {
    let (mut adapter, receiver) = started().await;
    let mut out = Vec::new();
    for (event, payload) in recorded("claude", "2.1.292", scenario) {
        let signals = fire(&mut adapter, &receiver, &event, payload).await;
        out.push((event, signals));
    }
    out
}

fn name(signal: &AdapterSignal) -> &'static str {
    match signal {
        AdapterSignal::SessionStarted { .. } => "session_started",
        AdapterSignal::TurnStarted { .. } => "turn_started",
        AdapterSignal::TurnEnded { .. } => "turn_ended",
        AdapterSignal::ToolStarted { .. } => "tool_started",
        AdapterSignal::ToolEnded { .. } => "tool_ended",
        AdapterSignal::FileChanged { .. } => "file_changed",
        AdapterSignal::InteractionOpen { .. } => "interaction_open",
        AdapterSignal::InteractionResolved { .. } => "interaction_resolved",
        AdapterSignal::Notice { .. } => "notice",
        AdapterSignal::Usage { .. } => "usage",
        AdapterSignal::RateLimited { .. } => "rate_limited",
        AdapterSignal::Error { .. } => "error",
        AdapterSignal::SubagentStarted { .. } => "subagent_started",
        AdapterSignal::SubagentEnded { .. } => "subagent_ended",
        AdapterSignal::Compacting { .. } => "compacting",
        AdapterSignal::PermissionMode { .. } => "permission_mode",
        AdapterSignal::SessionEnded { .. } => "session_ended",
        AdapterSignal::Raw { .. } => "raw",
    }
}

fn names(signals: &[AdapterSignal]) -> Vec<&'static str> {
    signals.iter().map(name).collect()
}

fn of<'a>(played: &'a [(String, Vec<AdapterSignal>)], event: &str) -> &'a [AdapterSignal] {
    &played
        .iter()
        .find(|(name, _)| name == event)
        .unwrap_or_else(|| panic!("no {event}"))
        .1
}

// --- the rows of the table of hooks ---------------------------------------------------------------

#[tokio::test]
async fn session_start_names_the_session_with_its_transcript_and_model() {
    let played = play("permission-allow").await;
    let start = of(&played, "SessionStart");
    assert_eq!(names(start), ["session_started"]);
    let AdapterSignal::SessionStarted {
        session_id,
        transcript_path,
        source,
        model,
    } = &start[0]
    else {
        panic!("{start:?}")
    };
    assert_eq!(session_id.len(), 36, "{session_id}");
    assert!(
        transcript_path
            .as_ref()
            .is_some_and(|p| p.ends_with(format!("{session_id}.jsonl")))
    );
    assert_eq!(*source, StartSource::Startup);
    assert_eq!(model.as_deref(), Some("claude-opus-5-5"));
}

#[tokio::test]
async fn a_prompt_starts_a_turn_with_a_preview_and_the_mode_is_reported_once() {
    let played = play("permission-allow").await;
    let prompt = of(&played, "UserPromptSubmit");
    // The first hook with a mode reports it, and the next ones do not.
    assert_eq!(names(prompt), ["permission_mode", "turn_started"]);
    assert!(matches!(&prompt[0], AdapterSignal::PermissionMode { mode } if mode == "default"));
    let AdapterSignal::TurnStarted {
        turn_ref,
        prompt_preview,
    } = &prompt[1]
    else {
        panic!()
    };
    assert_eq!(prompt_preview.as_deref(), Some("make a note"));
    assert!(turn_ref.is_some());
    let tool = of(&played, "PreToolUse");
    assert!(!names(tool).contains(&"permission_mode"));
}

#[tokio::test]
async fn a_long_prompt_is_cut_for_the_preview() {
    let (mut adapter, receiver) = started().await;
    let long = "é".repeat(500);
    let got = fire(
        &mut adapter,
        &receiver,
        "UserPromptSubmit",
        json!({"session_id": "s", "prompt": long, "prompt_id": "p"}),
    )
    .await;
    let AdapterSignal::TurnStarted { prompt_preview, .. } = got.last().expect("a signal") else {
        panic!()
    };
    let preview = prompt_preview.as_deref().expect("a preview");
    assert_eq!(preview.chars().count(), 201);
    assert!(preview.ends_with('…'));
}

#[tokio::test]
async fn a_request_for_permission_has_the_native_ref_of_the_call_before_it() {
    let played = play("permission-allow").await;
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
    let request = of(&played, "PermissionRequest");
    assert_eq!(names(request), ["interaction_open"]);
    let AdapterSignal::InteractionOpen {
        kind,
        native_ref,
        payload,
        gate,
    } = &request[0]
    else {
        panic!()
    };
    assert_eq!(*kind, InteractionKind::Approval);
    assert_eq!(native_ref, call_id);
    assert_eq!(*gate, GateMode::Gate);
    assert_eq!(payload["tool"], "Bash");
    assert_eq!(payload["command"], "touch note.txt");
    assert_eq!(payload["permission_mode"], "default");
    assert!(payload["suggestions"].is_array());
}

#[tokio::test]
async fn the_end_of_the_tool_resolves_the_approval_and_the_turn_and_session_end() {
    let played = play("permission-allow").await;
    let done = of(&played, "PostToolUse");
    assert_eq!(names(done), ["tool_ended", "interaction_resolved"]);
    assert!(matches!(&done[0], AdapterSignal::ToolEnded { ok: true, tool, .. } if tool == "Bash"));
    assert!(matches!(
        &done[1],
        AdapterSignal::InteractionResolved {
            resolution: Resolution::Allowed,
            ..
        }
    ));
    let stop = of(&played, "Stop");
    let AdapterSignal::TurnEnded {
        last_message_preview,
        ..
    } = &stop[0]
    else {
        panic!()
    };
    assert!(last_message_preview.is_some());
    let end = of(&played, "SessionEnd");
    assert!(
        matches!(&end[0], AdapterSignal::SessionEnded { reason } if reason == "prompt_input_exit")
    );
}

#[tokio::test]
async fn a_denied_request_stays_open_until_the_turn_ends() {
    // In the recordings, `PostToolUse` does not fire for a denied tool.
    let played = play("permission-deny").await;
    assert_eq!(
        names(of(&played, "PermissionRequest")),
        ["interaction_open"]
    );
    assert!(
        of(&played, "Stop")
            .iter()
            .any(|s| matches!(s, AdapterSignal::TurnEnded { .. }))
    );
    assert!(!played.iter().any(|(event, _)| event == "PostToolUse"));
}

#[tokio::test]
async fn ask_user_question_opens_one_question_for_the_pair_of_hooks() {
    let played = play("headless-question").await;
    let pre = of(&played, "PreToolUse");
    assert_eq!(names(pre), ["tool_started", "interaction_open"]);
    let AdapterSignal::InteractionOpen {
        kind,
        native_ref,
        payload,
        ..
    } = &pre[1]
    else {
        panic!()
    };
    assert_eq!(*kind, InteractionKind::Question);
    assert_eq!(
        payload["questions"][0]["question"],
        "Which color do you want?"
    );
    assert_eq!(payload["questions"][0]["options"][1]["label"], "Blue");
    // The dialog fires `PermissionRequest` for the same call: no second interaction.
    assert!(of(&played, "PermissionRequest").is_empty());
    // The tool ends with the answers, and the question is answered.
    let done = of(&played, "PostToolUse");
    let AdapterSignal::InteractionResolved {
        native_ref: resolved,
        resolution,
    } = &done[1]
    else {
        panic!("{done:?}")
    };
    assert_eq!(resolved, native_ref);
    assert_eq!(*resolution, Resolution::Answered);
}

#[tokio::test]
async fn exit_plan_mode_opens_a_plan_review_with_the_plan() {
    let played = play("plan-allow").await;
    // The write of the plan file is a file change.
    let write = of(&played, "PostToolUse");
    assert!(write.iter().any(|s| matches!(s, AdapterSignal::FileChanged { op: FileOp::Create, path } if path.ends_with("plan-a-note-calm-dusk.md") || path.extension().is_some())));
    let request = of(&played, "PermissionRequest");
    let AdapterSignal::InteractionOpen {
        kind,
        payload,
        native_ref,
        ..
    } = &request[0]
    else {
        panic!("{request:?}")
    };
    assert_eq!(*kind, InteractionKind::PlanReview);
    assert!(
        payload["plan_md"]
            .as_str()
            .is_some_and(|plan| plan.starts_with("# Plan"))
    );
    assert!(payload["plan_file"].is_string());
    assert!(native_ref.starts_with("toolu_"), "{native_ref}");
}

#[tokio::test]
async fn an_elicitation_of_an_mcp_server_opens_a_question_that_the_tool_closes() {
    let played = play("elicitation-hook").await;
    let ask = of(&played, "Elicitation");
    let AdapterSignal::InteractionOpen {
        kind,
        payload,
        native_ref,
        ..
    } = &ask[0]
    else {
        panic!("{ask:?}")
    };
    assert_eq!(*kind, InteractionKind::Question);
    assert_eq!(payload["server"], "golden");
    assert_eq!(payload["message"], "Which color do you want?");
    assert_eq!(payload["schema"]["properties"]["color"]["enum"][0], "red");
    let done = of(&played, "PostToolUse");
    assert!(done.iter().any(|s| matches!(s,
        AdapterSignal::InteractionResolved { native_ref: r, resolution: Resolution::Answered } if r == native_ref)));
}

#[tokio::test]
async fn the_notification_about_an_answered_elicitation_closes_it() {
    let (mut adapter, receiver) = started().await;
    let base = json!({"session_id": "s", "prompt_id": "p", "mcp_server_name": "golden", "message": "m", "mode": "form"});
    let open = fire(&mut adapter, &receiver, "Elicitation", base).await;
    assert!(
        open.iter()
            .any(|s| matches!(s, AdapterSignal::InteractionOpen { .. }))
    );
    let closed = fire(
        &mut adapter,
        &receiver,
        "Notification",
        json!({"session_id": "s", "notification_type": "elicitation_response", "message": "accept"}),
    )
    .await;
    assert!(closed.iter().any(|s| matches!(
        s,
        AdapterSignal::InteractionResolved {
            resolution: Resolution::Answered,
            ..
        }
    )));
    assert!(
        closed
            .iter()
            .any(|s| matches!(s, AdapterSignal::Notice { .. }))
    );
}

// --- the hooks that the recordings do not have -------------------------------------------------

#[tokio::test]
async fn the_notifications_that_change_the_state() {
    let (mut adapter, receiver) = started().await;
    let idle = fire(
        &mut adapter,
        &receiver,
        "Notification",
        json!({"session_id": "s", "notification_type": "idle_prompt", "message": "Claude is waiting"}),
    )
    .await;
    assert!(idle.iter().any(
        |s| matches!(s, AdapterSignal::TurnEnded { stop_reason: Some(r), .. } if r == "idle_prompt")
    ));
    let quota = fire(
        &mut adapter,
        &receiver,
        "Notification",
        json!({"session_id": "s", "notification_type": "quota_auto_resume_scheduled", "message": "m"}),
    )
    .await;
    assert!(quota.iter().any(|s| matches!(s, AdapterSignal::RateLimited { scope: Some(scope), .. } if scope.starts_with("quota_auto_resume"))));
    let other = fire(
        &mut adapter,
        &receiver,
        "Notification",
        json!({"session_id": "s", "notification_type": "auth_success", "message": "signed in"}),
    )
    .await;
    assert_eq!(names(&other), ["notice"]);
}

#[tokio::test]
async fn a_failure_to_stop_is_a_rate_limit_or_an_error() {
    let (mut adapter, receiver) = started().await;
    let limited = fire(
        &mut adapter,
        &receiver,
        "StopFailure",
        json!({"session_id": "s", "error_type": "rate_limit"}),
    )
    .await;
    assert_eq!(names(&limited), ["session_started", "rate_limited"]);
    let broken = fire(
        &mut adapter,
        &receiver,
        "StopFailure",
        json!({"session_id": "s", "error_type": "server_error", "message": "boom"}),
    )
    .await;
    assert!(
        matches!(&broken[0], AdapterSignal::Error { kind, message, retrying: false } if kind == "server_error" && message == "boom")
    );
}

#[tokio::test]
async fn subagents_and_compaction() {
    let (mut adapter, receiver) = started().await;
    let start = fire(
        &mut adapter,
        &receiver,
        "SubagentStart",
        json!({"session_id": "s", "agent_id": "a1", "agent_type": "Explore"}),
    )
    .await;
    assert!(
        matches!(start.last().expect("a signal"), AdapterSignal::SubagentStarted { native_id, kind: Some(kind) } if native_id == "a1" && kind == "Explore")
    );
    let stop = fire(
        &mut adapter,
        &receiver,
        "SubagentStop",
        json!({"session_id": "s", "agent_id": "a1"}),
    )
    .await;
    assert!(matches!(&stop[0], AdapterSignal::SubagentEnded { native_id } if native_id == "a1"));
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
    // The directory changes and nothing else.
    assert!(
        fire(
            &mut adapter,
            &receiver,
            "CwdChanged",
            json!({"session_id": "s", "cwd": "/x"})
        )
        .await
        .is_empty()
    );
}

#[tokio::test]
async fn a_request_that_was_denied_by_the_mode_resolves_its_approval() {
    let (mut adapter, receiver) = started().await;
    let input = json!({"command": "rm x"});
    fire(
        &mut adapter,
        &receiver,
        "PreToolUse",
        json!({"session_id": "s", "tool_name": "Bash", "tool_use_id": "t1", "tool_input": input}),
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
        "PermissionDenied",
        json!({"session_id": "s", "tool_name": "Bash", "tool_use_id": "t1"}),
    )
    .await;
    assert!(
        matches!(&got[0], AdapterSignal::InteractionResolved { native_ref, resolution: Resolution::Denied } if native_ref == "t1")
    );
    // Nothing is open any more, so a second denial says nothing.
    assert!(
        fire(
            &mut adapter,
            &receiver,
            "PermissionDenied",
            json!({"session_id": "s", "tool_name": "Bash"})
        )
        .await
        .is_empty()
    );
}

#[tokio::test]
async fn a_tool_that_failed_ends_without_a_file_change() {
    let (mut adapter, receiver) = started().await;
    let input = json!({"file_path": "/work/a.rs", "old_string": "a", "new_string": "b"});
    fire(
        &mut adapter,
        &receiver,
        "PreToolUse",
        json!({"session_id": "s", "tool_name": "Edit", "tool_use_id": "t2", "tool_input": input}),
    )
    .await;
    let failed = fire(
        &mut adapter,
        &receiver,
        "PostToolUseFailure",
        json!({"session_id": "s", "tool_name": "Edit", "tool_use_id": "t2", "tool_input": input}),
    )
    .await;
    assert_eq!(names(&failed), ["tool_ended"]);
    assert!(matches!(
        &failed[0],
        AdapterSignal::ToolEnded { ok: false, .. }
    ));
    // An edit that worked changes the file.
    fire(
        &mut adapter,
        &receiver,
        "PreToolUse",
        json!({"session_id": "s", "tool_name": "Edit", "tool_use_id": "t3", "tool_input": input}),
    )
    .await;
    let done = fire(&mut adapter, &receiver, "PostToolUse", json!({"session_id": "s", "tool_name": "Edit", "tool_use_id": "t3", "tool_input": input, "tool_response": {}})).await;
    assert_eq!(names(&done), ["tool_ended", "file_changed"]);
    assert!(
        matches!(&done[1], AdapterSignal::FileChanged { op: FileOp::Edit, path } if path.ends_with("a.rs"))
    );
}

// --- more behaviours ----------------------------------------------------------------------------

#[tokio::test]
async fn a_request_with_no_call_before_it_has_a_ref_that_follows_what_it_says() {
    let (mut adapter, receiver) = started().await;
    let payload = json!({"session_id": "s", "prompt_id": "p1", "tool_name": "Bash", "tool_input": {"command": "ls"}});
    let first = fire(
        &mut adapter,
        &receiver,
        "PermissionRequest",
        payload.clone(),
    )
    .await;
    let AdapterSignal::InteractionOpen { native_ref: a, .. } = &first[first.len() - 1] else {
        panic!("{first:?}")
    };
    // The same request again while it is open is the same interaction.
    let again = fire(&mut adapter, &receiver, "PermissionRequest", payload).await;
    assert!(again.is_empty(), "{again:?}");
    let other = fire(
        &mut adapter,
        &receiver,
        "PermissionRequest",
        json!({"session_id": "s", "prompt_id": "p1", "tool_name": "Bash", "tool_input": {"command": "pwd"}}),
    )
    .await;
    let AdapterSignal::InteractionOpen { native_ref: b, .. } = &other[other.len() - 1] else {
        panic!()
    };
    assert_ne!(a, b);
}

#[tokio::test]
async fn two_calls_with_the_same_input_are_matched_in_order_from_the_latest() {
    let (mut adapter, receiver) = started().await;
    let input = json!({"command": "make"});
    for id in ["t1", "t2"] {
        fire(
            &mut adapter,
            &receiver,
            "PreToolUse",
            json!({"session_id": "s", "tool_name": "Bash", "tool_use_id": id, "tool_input": input}),
        )
        .await;
    }
    let got = fire(
        &mut adapter,
        &receiver,
        "PermissionRequest",
        json!({"session_id": "s", "tool_name": "Bash", "tool_input": input}),
    )
    .await;
    assert!(
        matches!(&got[0], AdapterSignal::InteractionOpen { native_ref, .. } if native_ref == "t2")
    );
}

#[tokio::test]
async fn an_interaction_in_a_subagent_names_the_subagent() {
    let (mut adapter, receiver) = started().await;
    let got = fire(
        &mut adapter,
        &receiver,
        "PermissionRequest",
        json!({"session_id": "s", "agent_id": "a1", "agent_type": "Explore", "tool_name": "Bash", "tool_input": {"command": "ls"}}),
    )
    .await;
    let AdapterSignal::InteractionOpen { payload, .. } = got.last().expect("a signal") else {
        panic!()
    };
    assert_eq!(payload["agent"], json!({"id": "a1", "type": "Explore"}));
}

#[tokio::test]
async fn the_mode_without_dialogs_makes_the_run_yolo() {
    let (mut adapter, receiver) = started().await;
    let got = fire(
        &mut adapter,
        &receiver,
        "UserPromptSubmit",
        json!({"session_id": "s", "permission_mode": "bypassPermissions", "prompt": "go"}),
    )
    .await;
    let mode = got
        .iter()
        .find(|s| matches!(s, AdapterSignal::PermissionMode { .. }))
        .expect("a mode");
    assert_eq!(effects(mode), [Effect::PermissionMode { yolo: true }]);
    let back = fire(
        &mut adapter,
        &receiver,
        "UserPromptSubmit",
        json!({"session_id": "s", "permission_mode": "acceptEdits", "prompt": "go"}),
    )
    .await;
    let mode = back
        .iter()
        .find(|s| matches!(s, AdapterSignal::PermissionMode { .. }))
        .expect("a mode");
    assert_eq!(effects(mode), [Effect::PermissionMode { yolo: false }]);
}

#[tokio::test]
async fn a_hook_in_the_middle_of_a_session_names_it_without_a_state() {
    let (mut adapter, receiver) = started().await;
    let got = fire(
        &mut adapter,
        &receiver,
        "PostToolUse",
        json!({"session_id": "s9", "hook_event_name": "PostToolUse", "tool_name": "Bash", "tool_use_id": "t", "tool_input": {}}),
    )
    .await;
    let AdapterSignal::SessionStarted { source, .. } = &got[0] else {
        panic!("{got:?}")
    };
    assert_eq!(*source, StartSource::Other("seen".to_owned()));
    // `effects` makes it an identity and no state.
    assert_eq!(effects(&got[0]).len(), 1);
}

#[tokio::test]
async fn the_end_of_a_session_that_is_not_the_one_of_the_run_does_not_end_it() {
    // The same rule as for Codex.
    let (mut adapter, receiver) = started().await;
    fire(
        &mut adapter,
        &receiver,
        "SessionStart",
        json!({"session_id": "s-a", "source": "startup", "hook_event_name": "SessionStart"}),
    )
    .await;
    let moved = fire(
        &mut adapter,
        &receiver,
        "UserPromptSubmit",
        json!({"session_id": "s-b", "prompt_id": "p1", "prompt": "next", "hook_event_name": "UserPromptSubmit"}),
    )
    .await;
    assert_eq!(names(&moved), ["session_started", "turn_started"]);
    adapter
        .on_signal(AdapterSignal::Raw {
            harness_event: "SessionEnd".to_owned(),
            payload: json!({"session_id": "s-a", "reason": "other", "hook_event_name": "SessionEnd"}),
        })
        .await
        .expect("a reply");
    let old: Vec<AdapterSignal> = receiver.try_iter().collect();
    assert_eq!(names(&old), ["raw"], "{old:?}");
    assert_eq!(
        adapter.resume_handle(),
        Some(vec![
            "claude".to_owned(),
            "--resume".to_owned(),
            "s-b".to_owned()
        ])
    );
    let end = fire(
        &mut adapter,
        &receiver,
        "SessionEnd",
        json!({"session_id": "s-b", "reason": "clear", "hook_event_name": "SessionEnd"}),
    )
    .await;
    assert!(
        matches!(end.as_slice(), [AdapterSignal::SessionEnded { reason }] if reason == "clear"),
        "{end:?}"
    );
}

#[tokio::test]
async fn the_resume_handle_follows_the_manifest() {
    let (mut adapter, receiver) = started().await;
    assert_eq!(adapter.resume_handle(), None, "no session yet");
    fire(
        &mut adapter,
        &receiver,
        "SessionStart",
        json!({"session_id": "abc", "source": "startup", "hook_event_name": "SessionStart"}),
    )
    .await;
    assert_eq!(
        adapter.resume_handle(),
        Some(vec![
            "claude".to_owned(),
            "--resume".to_owned(),
            "abc".to_owned()
        ])
    );
}

#[tokio::test]
async fn the_capabilities_of_the_adapter_are_those_of_the_matrix() {
    let (adapter, _) = started().await;
    let caps = adapter.capabilities();
    assert!(caps.observe && caps.gate && caps.answer_keystroke && caps.resume);
    assert_eq!(
        caps.answer_native,
        [InteractionKind::Approval, InteractionKind::PlanReview]
    );
    assert!(!caps.reconcile && !caps.steer);
}
