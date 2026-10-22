//! The API socket against a real core: the envelope, the error codes, the scope of
//! the caller, the order of the replies.

mod core_support;

use std::time::Duration;

use core_support::{
    Harness, TEST_PANE_TOKEN,
    client::{Client, error_of},
};
use nh_core::ErrorCode;
use serde_json::{Value, json};

async fn client(h: &Harness) -> Client {
    Client::connect(&h.api_socket()).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_method_of_the_contract_runs_on_the_state_actor() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let info = c.ok("session.info", json!({})).await;
    assert_eq!(info["name"], "test");
    assert_eq!(info["pid"], std::process::id());
    assert!(info["protocols"]["holder"].is_u64());
    // `params` may be left out, and `null` counts as an empty object.
    let reply = c.call("machine.info", Value::Null).await;
    assert_eq!(reply["result"]["label"], "local");
    let reply = c.call("config.get", json!({"section": "tasks"})).await;
    assert_eq!(
        reply["result"]["sections"]["tasks"]["entries"]["ports"]["value"],
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_line_that_is_not_json_is_a_parse_error_and_the_connection_goes_on() {
    let h = Harness::start();
    let mut c = client(&h).await;
    c.send_raw(b"{this is not json}\n").await;
    let reply = c.recv().await;
    assert_eq!(error_of(&reply), (-32700, "parse_error".into()));
    assert_eq!(reply["id"], Value::Null);
    assert_eq!(reply["jsonrpc"], "2.0");
    // The next line works.
    assert_eq!(c.ok("session.info", json!({})).await["name"], "test");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_envelope_is_checked() {
    let h = Harness::start();
    let mut c = client(&h).await;
    // A batch, a bare value, no version, a method that is no string, an id of a bad type.
    for line in [
        r#"[{"jsonrpc":"2.0","id":1,"method":"session.info"}]"#,
        r#"42"#,
        r#"{"id":1,"method":"session.info"}"#,
        r#"{"jsonrpc":"2.0","id":1,"method":7}"#,
        r#"{"jsonrpc":"2.0","id":[1],"method":"session.info"}"#,
    ] {
        c.send_raw(format!("{line}\n").as_bytes()).await;
        let reply = c.recv().await;
        assert_eq!(
            error_of(&reply),
            (-32600, "invalid_request".into()),
            "{line}"
        );
    }
    // The id comes back as it was sent: a string, a negative number, null.
    for id in [json!("req-7"), json!(-4), Value::Null] {
        c.send(&json!({"jsonrpc":"2.0","id":id,"method":"session.info"}))
            .await;
        let reply = c.recv().await;
        assert_eq!(reply["id"], id);
        assert!(reply["result"].is_object());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_methods_and_bad_params_have_their_codes() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let reply = c.call("nothing.here", json!({})).await;
    assert_eq!(error_of(&reply), (-32601, "method_not_found".into()));
    // A name with the right namespace and no method.
    let reply = c.call("pane.nothing", json!({})).await;
    assert_eq!(error_of(&reply), (-32601, "method_not_found".into()));
    // Params of the wrong type, and a missing field.
    let reply = c.call("config.get", json!({"section": 5})).await;
    assert_eq!(error_of(&reply), (-32602, "invalid_params".into()));
    let reply = c.call("config.set", json!({"key": "agents.shims"})).await;
    assert_eq!(error_of(&reply), (-32602, "invalid_params".into()));
    // Params that are not an object.
    let reply = c.call("config.get", json!([1, 2])).await;
    assert_eq!(error_of(&reply), (-32602, "invalid_params".into()));
    // An error of the application: -32000 with the string.
    let reply = c.call("config.get", json!({"section": "nothing"})).await;
    assert_eq!(error_of(&reply), (-32000, "not_found".into()));
    assert!(
        reply["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("nothing"))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_of_the_cli_cannot_call_a_method_of_the_ui() {
    let h = Harness::start();
    let mut c = client(&h).await;
    // `pane.input` is for the UI. `render.ack` and `window.focus_report` too.
    for method in ["pane.input", "render.ack", "window.focus_report"] {
        let reply = c.call(method, json!({})).await;
        assert_eq!(
            error_of(&reply),
            (-32000, "permission_denied".into()),
            "{method}"
        );
    }
    // The host calls the same method as the UI, and it works (the pane does not exist).
    let error = h
        .handle
        .rpc(
            "pane.input",
            json!({"pane": "01ARZ3NDEKTSV4RRFFQ69G5FAV", "input_id": 1, "event": {"type": "text", "text": "x"}}),
        )
        .await
        .expect_err("no such pane");
    assert_eq!(error.code, ErrorCode::NotFound);
    // And the UI cannot call a method for the CLI.
    let error = h
        .handle
        .rpc("pane.read", json!({"pane": "x"}))
        .await
        .expect_err("not for the UI");
    assert_eq!(error.code, ErrorCode::PermissionDenied);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_can_call_the_adapter_methods_and_cannot_answer() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let answer = json!({"interaction": "01ARZ3NDEKTSV4RRFFQ69G5FAV", "decision": {"kind": "approval", "behavior": "allow"}, "via": "cli"});

    // Without a token the caller is a client of the CLI: it may answer (the interaction does
    // not exist, but the door is open), and it may not use an adapter method.
    let reply = c.call("interaction.answer", answer.clone()).await;
    assert_ne!(error_of(&reply).1, "permission_denied", "{reply}");
    let reply = c
        .call(
            "adapter.signal",
            json!({"harness": "claude", "event": "Stop"}),
        )
        .await;
    assert_eq!(error_of(&reply), (-32000, "permission_denied".into()));

    // With the token of a pane in the envelope, the caller has pane scope.
    c.send(
        &json!({"jsonrpc": "2.0", "id": 100, "pane_token": TEST_PANE_TOKEN,
        "method": "interaction.answer", "params": answer}),
    )
    .await;
    let reply = c.recv().await;
    assert_eq!(
        error_of(&reply),
        (-32000, "permission_denied".into()),
        "{reply}"
    );
    assert!(
        reply["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("a pane"))
    );
    for method in ["session.info", "config.set", "policy.add"] {
        c.send(
            &json!({"jsonrpc": "2.0", "id": 101, "pane_token": TEST_PANE_TOKEN,
            "method": method, "params": {}}),
        )
        .await;
        assert_eq!(error_of(&c.recv().await).1, "permission_denied", "{method}");
    }

    // The adapter methods work with the token in the params, as the shim sends it.
    let reply = c
        .call(
            "adapter.signal",
            json!({"token": TEST_PANE_TOKEN, "harness": "claude", "event": "Stop"}),
        )
        .await;
    assert_eq!(reply["result"], json!({}));
    // A token that no pane has is refused, in the envelope and in the params.
    let reply = c
        .call(
            "adapter.signal",
            json!({"token": "nobody", "harness": "claude", "event": "Stop"}),
        )
        .await;
    assert_eq!(error_of(&reply), (-32000, "permission_denied".into()));
    c.send(&json!({"jsonrpc": "2.0", "id": 102, "pane_token": "nobody",
        "method": "session.info", "params": {}}))
        .await;
    assert_eq!(error_of(&c.recv().await).1, "permission_denied");
    assert_eq!(
        h.handle.recent_signals().len(),
        1,
        "only the good signal counts"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_notification_runs_and_gets_no_reply() {
    let h = Harness::start();
    let mut c = client(&h).await;
    c.send(&json!({"jsonrpc": "2.0", "method": "config.set",
        "params": {"key": "agents.shims", "value": false}}))
        .await;
    // The next request has a reply, and nothing came before it.
    let reply = c.call("config.get", json!({"section": "agents"})).await;
    let shims = &reply["result"]["sections"]["agents"]["entries"]["shims"];
    // The notification may still be running when the read comes. Wait until it has an effect.
    if shims["value"] != json!(false) {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let reply = c.call("config.get", json!({"section": "agents"})).await;
            if reply["result"]["sections"]["agents"]["entries"]["shims"]["value"] == json!(false) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the notification had no effect"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    assert!(
        c.try_recv(Duration::from_millis(100)).await.is_none(),
        "no stray reply"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_slow_call_does_not_hold_the_calls_after_it() {
    let h = Harness::start();
    let mut c = client(&h).await;
    // Starting a holder takes a while. The call after it must not wait for it.
    let slow = c
        .request(
            "debug.spawn_panes",
            json!({"count": 3, "cols": 40, "rows": 10, "argv": ["/bin/sh", "-c", "sleep 30"]}),
        )
        .await;
    let fast = c.request("session.info", json!({})).await;
    let first = c.recv().await;
    assert_eq!(
        first["id"],
        json!(fast),
        "the fast reply comes first: {first}"
    );
    let second = c.recv().await;
    assert_eq!(second["id"], json!(slow));
    assert_eq!(second["result"]["panes"].as_array().map(Vec::len), Some(3));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn many_connections_work_at_once() {
    let h = Harness::start();
    let socket = h.api_socket();
    let mut tasks = Vec::new();
    for n in 0..16 {
        let socket = socket.clone();
        tasks.push(tokio::spawn(async move {
            let mut c = Client::connect(&socket).await;
            for _ in 0..20 {
                let info = c.ok("session.info", json!({})).await;
                assert_eq!(info["name"], "test", "client {n}");
            }
        }));
    }
    for task in tasks {
        task.await.expect("a client finishes");
    }
}
