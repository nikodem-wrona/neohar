use serde_json::json;

use super::*;
use crate::cli::testing::{FakeCore, Reply};

const WAIT: Duration = Duration::from_secs(5);

#[test]
fn a_call_sends_one_json_rpc_line_and_returns_the_result() {
    let core = FakeCore::start(vec![Reply::Result(json!({"name": "default"}))]);
    let mut client = Client::connect(&core.socket, WAIT).expect("connects");
    let result = client.call("session.info", json!({})).expect("a result");
    assert_eq!(result, json!({"name": "default"}));
    assert_eq!(
        core.requests(),
        [json!({"jsonrpc": "2.0", "id": 1, "method": "session.info", "params": {}})]
    );
}

#[test]
fn ids_grow_with_each_call_on_one_connection() {
    let core = FakeCore::start(vec![Reply::Result(json!(1)), Reply::Result(json!(2))]);
    let mut client = Client::connect(&core.socket, WAIT).expect("connects");
    assert_eq!(client.call("a.b", json!({})).expect("1"), json!(1));
    assert_eq!(client.call("c.d", json!({"x": 1})).expect("2"), json!(2));
    let ids: Vec<_> = core.requests().iter().map(|r| r["id"].clone()).collect();
    assert_eq!(ids, [json!(1), json!(2)]);
}

#[test]
fn notifications_on_the_way_are_skipped() {
    let note = json!({"jsonrpc": "2.0", "method": "events.event", "params": {}});
    let other = json!({"jsonrpc": "2.0", "id": 99, "result": "not mine"});
    let core = FakeCore::start(vec![Reply::After(
        note,
        Box::new(Reply::After(other, Box::new(Reply::Result(json!("mine"))))),
    )]);
    let mut client = Client::connect(&core.socket, WAIT).expect("connects");
    assert_eq!(
        client.call("x.y", json!({})).expect("a result"),
        json!("mine")
    );
}

#[test]
fn an_error_reply_has_its_codes_message_and_details() {
    let core = FakeCore::start(vec![Reply::Error(-32000, "not_found", "no pane x")]);
    let mut client = Client::connect(&core.socket, WAIT).expect("connects");
    let Err(CallError::Rpc(failure)) = client.call("pane.read", json!({})) else {
        panic!("an error reply");
    };
    assert_eq!(failure.rpc_code, -32000);
    assert_eq!(failure.code, "not_found");
    assert_eq!(failure.message, "no pane x");
    assert_eq!(failure.details, Value::Null);
}

#[test]
fn an_error_without_data_still_has_a_code() {
    let error = json!({"code": -32601, "message": "unknown method"});
    let failure = failure(&error);
    assert_eq!(
        (failure.rpc_code, failure.code.as_str()),
        (-32601, "internal")
    );
    let failure = super::failure(&json!({}));
    assert_eq!(failure.rpc_code, -32603);
    assert!(!failure.message.is_empty());
}

#[test]
fn a_closed_connection_and_a_line_that_is_no_json_are_protocol_errors() {
    let core = FakeCore::start(vec![Reply::Close]);
    let mut client = Client::connect(&core.socket, WAIT).expect("connects");
    assert!(matches!(
        client.call("x.y", json!({})),
        Err(CallError::Protocol(message)) if message.contains("closed")
    ));
    drop(core);
    let core = FakeCore::start(vec![Reply::Garbage]);
    let mut client = Client::connect(&core.socket, WAIT).expect("connects");
    assert!(matches!(
        client.call("x.y", json!({})),
        Err(CallError::Protocol(message)) if message.contains("no JSON")
    ));
}

#[test]
fn a_core_that_does_not_answer_is_a_timeout() {
    let core = FakeCore::start(vec![Reply::Silent(Duration::from_millis(800))]);
    let mut client = Client::connect(&core.socket, Duration::from_millis(200)).expect("connects");
    let started = std::time::Instant::now();
    assert!(matches!(
        client.call("x.y", json!({})),
        Err(CallError::Timeout(time)) if time == Duration::from_millis(200)
    ));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn nobody_at_the_socket_is_not_running() {
    let missing = std::env::temp_dir().join("nhcli-missing.sock");
    let _ = std::fs::remove_file(&missing);
    assert!(matches!(
        Client::connect(&missing, WAIT),
        Err(CallError::NotRunning(path)) if path == missing
    ));
    // A socket file that nobody listens on.
    let stale = std::env::temp_dir().join(format!("nhcli-stale{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&stale);
    drop(std::os::unix::net::UnixListener::bind(&stale).expect("a socket"));
    assert!(matches!(
        Client::connect(&stale, WAIT),
        Err(CallError::NotRunning(_))
    ));
    let _ = std::fs::remove_file(&stale);
}

#[test]
fn a_socket_in_a_directory_that_others_can_write_is_refused() {
    use std::os::unix::fs::PermissionsExt;

    let dir = std::env::temp_dir().join(format!("nhcli-open{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir(&dir).expect("a directory");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).expect("the mode");
    let socket = dir.join("api.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&socket).expect("a socket");
    let refused = Client::connect(&socket, WAIT);
    let _ = std::fs::remove_dir_all(&dir);
    match refused {
        Err(CallError::Io(error)) => {
            assert_eq!(
                error.kind(),
                std::io::ErrorKind::PermissionDenied,
                "{error}"
            );
        }
        Err(other) => panic!("another error: {other:?}"),
        Ok(_) => panic!("the CLI connected to a socket that another user could have made"),
    }
}
