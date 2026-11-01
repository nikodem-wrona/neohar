//! The chat model, with updates in the shape that Hermes 0.22.5 sent in the spike of 2026-10-31.

use nh_proto::api::model::{HermesAnswerState, HermesItemBody, HermesPlanStatus, HermesToolStatus};
use serde_json::{Value, json};

use super::{
    approvals_mode,
    thread::{AnswerError, Kind, Thread},
};

fn chunk(kind: &str, text: &str, message: Option<&str>) -> Value {
    let mut update = json!({"sessionUpdate": kind, "content": {"type": "text", "text": text}});
    if let Some(message) = message {
        update["messageId"] = json!(message);
    }
    update
}

fn texts(thread: &Thread) -> Vec<String> {
    thread
        .entries()
        .iter()
        .map(|entry| match &entry.kind {
            Kind::Prompt { text } => format!("prompt:{text}"),
            Kind::Assistant { text, .. } => format!("assistant:{text}"),
            Kind::Thought { text } => format!("thought:{text}"),
            Kind::Tool { title, status, .. } => format!("tool:{title}:{status:?}"),
            Kind::Plan { entries } => format!("plan:{}", entries.len()),
            Kind::Approval { title, state, .. } => format!("approval:{title}:{state:?}"),
            Kind::Notice { text } => format!("notice:{text}"),
        })
        .collect()
}

#[test]
fn chunks_of_one_message_make_one_item() {
    let mut thread = Thread::default();
    thread.prompt("Say hi", 1);
    thread.update(&chunk("agent_message_chunk", "\n\nDone", Some("m1")), 2);
    let outcome = thread.update(&chunk("agent_message_chunk", ".", Some("m1")), 3);
    assert!(outcome.agent_text);
    assert_eq!(texts(&thread), ["prompt:Say hi", "assistant:\n\nDone."]);
}

#[test]
fn a_chunk_with_the_full_text_replaces_it() {
    // A client that did not send the prompt gets the deltas, then the full text once more.
    let mut thread = Thread::default();
    for part in ["Could", "n’t", " create", " gone.txt."] {
        thread.update(&chunk("agent_message_chunk", part, Some("m1")), 1);
    }
    thread.update(
        &chunk(
            "agent_message_chunk",
            "Couldn’t create gone.txt.\n\nThe edit was denied.",
            Some("m1"),
        ),
        2,
    );
    assert_eq!(
        texts(&thread),
        ["assistant:Couldn’t create gone.txt.\n\nThe edit was denied."]
    );
}

#[test]
fn a_new_message_id_starts_a_new_item_and_a_replay_has_none() {
    let mut thread = Thread::default();
    thread.update(&chunk("user_message_chunk", "Reply with one", None), 1);
    thread.update(&chunk("agent_message_chunk", "one", None), 2);
    thread.prompt("Reply with two", 3);
    thread.update(&chunk("agent_message_chunk", "two", Some("a")), 4);
    thread.update(&chunk("agent_message_chunk", "three", Some("b")), 5);
    assert_eq!(
        texts(&thread),
        [
            "prompt:Reply with one",
            "assistant:one",
            "prompt:Reply with two",
            "assistant:two",
            "assistant:three"
        ]
    );
}

#[test]
fn a_tool_call_starts_and_its_update_finishes_it_with_the_output() {
    let mut thread = Thread::default();
    thread.update(
        &json!({
            "sessionUpdate": "tool_call", "toolCallId": "call_1", "kind": "execute",
            "title": "terminal: cat hello.txt",
            "content": [{"type": "content", "content": {"type": "text", "text": "$ cat hello.txt"}}],
        }),
        1,
    );
    assert_eq!(texts(&thread), ["tool:terminal: cat hello.txt:Running"]);
    let outcome = thread.update(
        &json!({
            "sessionUpdate": "tool_call_update", "toolCallId": "call_1", "status": "completed",
            "content": [{"type": "content", "content": {"type": "text",
                "text": "terminal result\n- **output:** hello\n- **exit_code:** 0"}}],
        }),
        2,
    );
    assert!(outcome.tool);
    assert_eq!(texts(&thread), ["tool:terminal: cat hello.txt:Done"]);
    let changes = thread.take_changes(false);
    let HermesItemBody::Tool { output, status, .. } = &changes.items[0].body else {
        panic!("a tool item");
    };
    assert_eq!(*status, HermesToolStatus::Done);
    let html = &output.as_ref().expect("an output").html;
    assert!(html.contains("<strong>output:</strong>"), "{html}");
}

#[test]
fn the_tool_call_of_an_approval_makes_no_item() {
    let mut thread = Thread::default();
    let outcome = thread.update(
        &json!({"sessionUpdate": "tool_call_update", "toolCallId": "edit-approval-1",
                "status": "completed"}),
        1,
    );
    assert!(!outcome.tool);
    assert!(thread.entries().is_empty());
}

fn permission(id: &str) -> Value {
    json!({
        "options": [
            {"kind": "allow_once", "name": "Allow once", "optionId": "allow_once"},
            {"kind": "reject_once", "name": "Deny", "optionId": "deny"},
        ],
        "sessionId": "local-1",
        "toolCall": {
            "content": [{"type": "diff", "path": "/work/hello.txt", "oldText": "hi\n",
                         "newText": "hello\n"}],
            "kind": "edit", "status": "pending", "title": "Approve edit: /work/hello.txt",
            "toolCallId": id,
        },
    })
}

#[test]
fn an_approval_is_answered_once_with_one_of_its_options() {
    let mut thread = Thread::default();
    let request = thread.approval(json!(0), &permission("edit-approval-1"), 1, 301);
    assert_eq!(request, "edit-approval-1");
    assert_eq!(thread.open_approvals(), 1);
    assert_eq!(
        thread.answer(&request, "always"),
        Err(AnswerError::NoSuchOption)
    );
    let (rpc, option) = thread.answer(&request, "deny").expect("answered");
    assert_eq!((rpc, option.as_str()), (json!(0), "deny"));
    assert_eq!(
        thread.answer(&request, "deny"),
        Err(AnswerError::NotOpen(HermesAnswerState::Answered))
    );
    assert_eq!(thread.open_approvals(), 0);
    let changes = thread.take_changes(false);
    let HermesItemBody::Approval {
        options,
        diff,
        answer,
        ..
    } = &changes.items[0].body
    else {
        panic!("an approval item");
    };
    assert_eq!(answer.as_deref(), Some("Deny"));
    assert!(options[0].allow && !options[1].allow);
    assert_eq!(diff.as_ref().expect("a diff").files.len(), 1);
}

#[test]
fn a_repeated_request_takes_the_new_rpc_id_and_the_deadline_expires_it() {
    let mut thread = Thread::default();
    thread.approval(json!(0), &permission("edit-approval-1"), 1, 301);
    // A new connection repeats the request under another JSON-RPC id.
    thread.approval(json!(7), &permission("edit-approval-1"), 2, 302);
    assert_eq!(thread.entries().len(), 1);
    assert!(!thread.expire(301));
    assert!(thread.expire(302));
    assert_eq!(
        thread.answer("edit-approval-1", "allow_once"),
        Err(AnswerError::NotOpen(HermesAnswerState::Expired))
    );
}

#[test]
fn the_plan_of_a_turn_is_one_item_that_changes() {
    let mut thread = Thread::default();
    thread.prompt("Plan it", 1);
    let plan = |status: &str| {
        json!({"sessionUpdate": "plan", "entries": [
            {"content": "Create hello.txt", "priority": "medium", "status": status},
            {"content": "Run cat", "priority": "medium", "status": "pending"},
        ]})
    };
    thread.update(&plan("pending"), 2);
    thread.update(&plan("completed"), 3);
    assert_eq!(texts(&thread), ["prompt:Plan it", "plan:2"]);
    let Kind::Plan { entries } = &thread.entries()[1].kind else {
        panic!("a plan");
    };
    assert_eq!(entries[0].status, HermesPlanStatus::Completed);
    thread.prompt("Again", 4);
    thread.update(&plan("pending"), 5);
    assert_eq!(texts(&thread).len(), 4, "a new turn has its own plan");
}

#[test]
fn a_reset_removes_every_item_and_new_keys_follow() {
    let mut thread = Thread::default();
    thread.prompt("one", 1);
    thread.update(&chunk("agent_message_chunk", "two", None), 2);
    let first = thread.take_changes(false);
    assert_eq!(first.items.len(), 2);
    assert_eq!(
        first.order.as_deref(),
        Some(&["1".to_owned(), "2".to_owned()][..])
    );
    thread.reset();
    thread.update(&chunk("user_message_chunk", "one", None), 3);
    let second = thread.take_changes(false);
    assert_eq!(second.removed, ["1", "2"]);
    assert_eq!(second.order.as_deref(), Some(&["3".to_owned()][..]));
}

#[test]
fn a_history_that_ends_in_a_prompt_or_a_running_tool_is_an_open_turn() {
    let mut thread = Thread::default();
    assert!(!thread.ends_open());
    thread.update(&chunk("user_message_chunk", "Run sleep", None), 1);
    assert!(thread.ends_open());
    thread.update(
        &json!({"sessionUpdate": "tool_call", "toolCallId": "c", "title": "terminal: sleep 25"}),
        2,
    );
    thread.update(&chunk("agent_message_chunk", "Waiting", None), 3);
    assert!(thread.ends_open(), "the tool runs");
    thread.end_turn();
    assert!(!thread.ends_open());
    assert_eq!(texts(&thread)[1], "tool:terminal: sleep 25:Failed");
}

#[test]
fn the_streaming_flag_is_on_the_last_item_only() {
    let mut thread = Thread::default();
    thread.prompt("Hi", 1);
    thread.update(&chunk("agent_message_chunk", "Hello", None), 2);
    let changes = thread.take_changes(true);
    let streaming: Vec<bool> = changes
        .items
        .iter()
        .map(|item| {
            matches!(
                item.body,
                HermesItemBody::Assistant {
                    streaming: true,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(streaming, [false, true]);
}

#[test]
fn the_approvals_mode_comes_from_the_top_level_block() {
    let text = "model:\n  default: x\n  mode: fast\napprovals:\n  # a comment\n  timeout: 60\n  \
                mode: manual  # ask\nterminal:\n  mode: y\n";
    assert_eq!(approvals_mode(text).as_deref(), Some("manual"));
    assert_eq!(approvals_mode("model:\n  mode: fast\n"), None);
    assert_eq!(
        approvals_mode("approvals:\n  mode: \"off\"\n").as_deref(),
        Some("off")
    );
}
