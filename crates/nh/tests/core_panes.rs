//! Panes of `nh-core` against real holders: output, input, resize, exit, queries and the `panes`
//! projection.

mod core_support;

use std::time::Duration;

use core_support::{Harness, Messages, has_row, wait_for};
use nh_core::{ErrorCode, pane::Liveness};
use nh_proto::holder::ExitStatus;
use rustix::process::{Pid, Signal, kill_process, test_kill_process};
use serde_json::json;

/// The window that resizes the panes. No window with this id is open. The first window that sends
/// `pane.resize` sets the size of the pane, open or not.
const WINDOW: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_shows_what_its_program_prints() {
    let h = Harness::start();
    let pane = h
        .spawn(
            &["/bin/sh", "-c", "printf 'hello\\r\\nworld'; sleep 60"],
            40,
            10,
        )
        .await;
    let text = wait_for(&pane, "hello and world", |t| {
        has_row(t, "hello") && has_row(t, "world")
    })
    .await;
    assert_eq!(text.len(), 10);
    assert_eq!(pane.size(), (40, 10));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_health_lists_the_protocol_that_the_holders_speak() {
    let h = Harness::start();
    assert!(h.handle.health().value().holder_protocols.is_empty());
    let _pane = h.spawn(&["/bin/sh", "-c", "sleep 60"], 40, 10).await;
    assert_eq!(
        h.handle.health().value().holder_protocols,
        [nh_proto::version::HOLDER_PROTO]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_runs_what_the_user_types() {
    let h = Harness::start();
    let pane = h.spawn(&["/bin/sh"], 60, 12).await;
    // Wait for the prompt, so that the shell has set up the terminal before the first key.
    wait_for(&pane, "a prompt", |t| {
        t.iter().any(|row| row.ends_with('$'))
    })
    .await;
    h.text(&pane, "echo typed-by-the-test").await;
    h.key(&pane, "Enter", "Enter").await;
    wait_for(&pane, "the output of echo", |t| {
        has_row(t, "typed-by-the-test")
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resize_reaches_the_pty_and_the_engine() {
    let h = Harness::start();
    let pane = h.spawn(&["/bin/sh"], 60, 12).await;
    wait_for(&pane, "a prompt", |t| {
        t.iter().any(|row| row.ends_with('$'))
    })
    .await;
    h.call(
        "pane.resize",
        json!({ "pane": pane.id().to_string(), "cols": 100, "rows": 30, "window": WINDOW }),
    )
    .await;
    // The engine changes size when the holder reports the resize marker.
    wait_for(&pane, "the new size", |t| t.len() == 30).await;
    assert_eq!(pane.size(), (100, 30));
    h.text(&pane, "stty size").await;
    h.key(&pane, "Enter", "Enter").await;
    wait_for(&pane, "the size that the program sees", |t| {
        has_row(t, "30 100")
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_same_size_twice_is_one_resize() {
    let h = Harness::start();
    let pane = h
        .spawn(
            &[
                "/bin/sh",
                "-c",
                "trap 'echo winch' WINCH; echo ready; while :; do sleep 0.05; done",
            ],
            60,
            12,
        )
        .await;
    wait_for(&pane, "ready", |t| has_row(t, "ready")).await;
    let params = json!({ "pane": pane.id().to_string(), "cols": 80, "rows": 20, "window": WINDOW });
    h.call("pane.resize", params.clone()).await;
    h.call("pane.resize", params).await;
    wait_for(&pane, "one winch", |t| has_row(t, "winch")).await;
    // A second signal would print a second row. Give it time to arrive.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let winches = pane.text().iter().filter(|row| *row == "winch").count();
    assert_eq!(winches, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_exited_child_is_reported_and_its_holder_leaves() {
    let h = Harness::start();
    let mut panes = Messages::subscribe(&h.handle, "panes");
    let pane = h
        .spawn(&["/bin/sh", "-c", "echo bye; exit 3"], 40, 10)
        .await;
    let id = pane.id().to_string();
    panes
        .wait_for("the pane exited", |m| {
            m["type"] == "patch"
                && m["ops"].as_array().is_some_and(|ops| {
                    ops.iter()
                        .any(|op| op["key"] == id && op["value"]["liveness"] == "exited")
                })
        })
        .await;
    assert_eq!(pane.info().liveness, Liveness::Exited(ExitStatus::Code(3)));
    // The screen stays.
    assert!(has_row(&pane.text(), "bye"));
    let pid = Pid::from_raw(pane.holder_pid()).expect("a holder pid");
    let deadline = std::time::Instant::now() + core_support::TIMEOUT;
    while test_kill_process(pid).is_ok() {
        assert!(
            std::time::Instant::now() < deadline,
            "the holder is still there"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_engine_answers_a_terminal_query_through_the_holder() {
    let h = Harness::start();
    // The program asks for the device attributes, reads one byte of the reply, and prints it.
    let pane = h
        .spawn(
            &[
                "/bin/sh",
                "-c",
                "stty raw -echo; printf '\\033[c'; head -c 1 | od -An -tx1; sleep 60",
            ],
            60,
            12,
        )
        .await;
    // 1b is ESC, the first byte of the reply. Raw mode sends no carriage return, so the row starts
    // away from the left edge.
    wait_for(&pane, "the first byte of the reply", |t| {
        t.iter().any(|row| row.trim() == "1b")
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_panes_projection_follows_a_pane() {
    let h = Harness::start();
    let mut panes = Messages::subscribe(&h.handle, "panes");
    let snapshot = panes
        .wait_for("the snapshot", |m| m["type"] == "snapshot")
        .await;
    assert_eq!(snapshot["data"]["order"], json!([]));
    let pane = h
        .spawn(
            &["/bin/sh", "-c", "printf '\\033]2;a title\\007'; sleep 60"],
            50,
            8,
        )
        .await;
    let id = pane.id().to_string();
    let added = panes
        .wait_for("the new pane", |m| {
            m["type"] == "patch" && m["ops"][1]["op"] == "order"
        })
        .await;
    assert_eq!(added["ops"][0]["value"]["render_id"], pane.render_id());
    assert_eq!(added["ops"][0]["value"]["cols"], 50);
    assert_eq!(added["ops"][1]["keys"], json!([id]));
    // The title can already be in the first patch: the hub merges the changes of 16 ms.
    let titled = if added["ops"][0]["value"]["title"] == "a title" {
        added
    } else {
        panes
            .wait_for("the title", |m| {
                m["type"] == "patch" && m["ops"][0]["value"]["title"] == "a title"
            })
            .await
    };
    assert_eq!(titled["ops"][0]["key"], id);
    assert_eq!(titled["ops"][0]["value"]["liveness"], "running");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bad_call_is_a_typed_error() {
    let h = Harness::start();
    let unknown = h
        .handle
        .rpc("pane.nope", json!({}))
        .await
        .expect_err("no such method");
    assert_eq!(unknown.code, ErrorCode::MethodNotFound);
    let missing = h
        .handle
        .rpc(
            "pane.input",
            json!({ "pane": "01ARZ3NDEKTSV4RRFFQ69G5FAV", "input_id": 1, "event": { "type": "text", "text": "x" } }),
        )
        .await
        .expect_err("no such pane");
    assert_eq!(missing.code, ErrorCode::NotFound);
    let malformed = h
        .handle
        .rpc("pane.resize", json!({ "pane": "x" }))
        .await
        .expect_err("missing fields");
    assert_eq!(malformed.code, ErrorCode::InvalidParams);
    let unknown_projection = h
        .handle
        .subscribe("nope", None, Box::new(NoSink))
        .expect_err("no such projection");
    assert_eq!(unknown_projection.code, ErrorCode::NotFound);
}

struct NoSink;

impl nh_core::MessageSink for NoSink {
    fn send(&self, _: serde_json::Value) -> bool {
        true
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn twenty_panes_start_in_order_and_all_paint() {
    let h = Harness::start();
    let mut panes = Messages::subscribe(&h.handle, "panes");
    let started = std::time::Instant::now();
    let result = h
        .call(
            "debug.spawn_panes",
            json!({
                "count": 20,
                "cols": 100,
                "rows": 30,
                "argv": ["/bin/sh", "-c", "echo up; sleep 60"],
            }),
        )
        .await;
    let spawned = started.elapsed();
    let list = result["panes"].as_array().expect("a list of panes");
    assert_eq!(list.len(), 20);
    // The ids are in the order of the call, and the projection has the same order.
    let render_ids: Vec<u64> = list
        .iter()
        .map(|p| p["render_id"].as_u64().expect("a render id"))
        .collect();
    assert!(
        render_ids.windows(2).all(|w| w[0] + 1 == w[1]),
        "{render_ids:?}"
    );
    let snapshot = panes
        .wait_for("the snapshot", |m| m["type"] == "snapshot")
        .await;
    assert_eq!(snapshot["data"]["order"], json!([]));
    let mut order = Vec::new();
    while order.len() < 20 {
        let patch = panes.wait_for("a new pane", |m| m["type"] == "patch").await;
        if let Some(keys) = patch["ops"]
            .as_array()
            .and_then(|ops| ops.iter().find(|o| o["op"] == "order"))
        {
            order = keys["keys"].as_array().expect("keys").clone();
        }
    }
    let expected: Vec<_> = list.iter().map(|p| p["pane"].clone()).collect();
    assert_eq!(order, expected);
    for entry in list {
        let id: ulid::Ulid = entry["pane"]
            .as_str()
            .expect("an id")
            .parse()
            .expect("a ULID");
        let pane = h.handle.pane(id).expect("the pane is known");
        wait_for(&pane, "up", |t| has_row(t, "up")).await;
    }
    eprintln!(
        "20 panes started in {spawned:?} and painted in {:?}",
        started.elapsed()
    );
}

/// The directory of the fake sidecar. Dropping it kills the sleeping "holder" and removes the
/// directory, also when the test fails: the core kills the holder only when it read READY.
struct FakeSidecar(std::path::PathBuf);

impl FakeSidecar {
    /// The pid of the sleeping process, which the script writes before READY.
    fn sleeper(&self) -> Option<Pid> {
        std::fs::read_to_string(self.0.join("pid"))
            .ok()?
            .trim()
            .parse()
            .ok()
            .and_then(Pid::from_raw)
    }
}

impl Drop for FakeSidecar {
    fn drop(&mut self) {
        if let Some(pid) = self.sleeper() {
            // The pid can belong to another process by now. Kill it only if it still sleeps.
            let args = std::process::Command::new("ps")
                .args(["-o", "args=", "-p", &pid.as_raw_nonzero().to_string()])
                .output()
                .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
                .unwrap_or_default();
            if args == "sleep 600" {
                let _ = kill_process(pid, Signal::KILL);
            }
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_holder_that_cannot_be_reached_is_killed() {
    use std::{io::Write, os::unix::fs::PermissionsExt};

    // A sidecar that reports READY and never binds a socket. Its "holder" is a sleeping process.
    let fake = FakeSidecar(std::path::PathBuf::from(format!(
        "/tmp/nhc-fake-{}",
        std::process::id()
    )));
    std::fs::create_dir_all(&fake.0).expect("a directory");
    let script = fake.0.join("nh");
    let mut file = std::fs::File::create(&script).expect("a script");
    file.write_all(
        b"#!/bin/sh\nsleep 600 >/dev/null 2>&1 &\necho $! > \"$(dirname \"$0\")/pid\"\necho \"READY $!\"\n",
    )
    .expect("the script is written");
    drop(file);
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("executable");

    let h = Harness::start_with(script);
    let error = h
        .handle
        .rpc(
            "debug.spawn_panes",
            json!({ "cols": 40, "rows": 10, "argv": ["/bin/sh"] }),
        )
        .await
        .expect_err("nobody can connect to the holder");
    assert_eq!(error.code, ErrorCode::HolderError);
    // The pid that the script reported is in no message, so the script also wrote it to a file.
    let pid = fake.sleeper().expect("the script wrote the pid");
    let deadline = std::time::Instant::now() + core_support::TIMEOUT;
    while test_kill_process(pid).is_ok() {
        assert!(
            std::time::Instant::now() < deadline,
            "the holder {} still runs",
            pid.as_raw_nonzero()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
