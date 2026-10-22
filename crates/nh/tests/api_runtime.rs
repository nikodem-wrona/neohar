//! The pane runtime behind the layout: a pane that the layout makes gets a holder with the
//! environment of the pane and a token, a closed pane loses its process, a pane that ended can
//! start again, and all of it comes back after a restart of the core.

mod core_support;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use core_support::{
    Harness, Messages, TIMEOUT,
    client::{Client, error_of},
    wait_for,
};
use nh_core::{
    ErrorCode, RecordingHost, StopReport, Tuning,
    host::HostCall,
    pane::{Liveness, Pane},
};
use rustix::process::{Pid, test_kill_process};
use serde_json::{Value, json};
use ulid::Ulid;

async fn client(h: &Harness) -> Client {
    Client::connect(&h.api_socket()).await
}

fn ulid(value: &Value) -> Ulid {
    value.as_str().expect("an id").parse().expect("a ULID")
}

/// The pane of the core, once the pane service has started its holder.
async fn started(h: &Harness, id: &Value) -> Arc<Pane> {
    let id = ulid(id);
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(pane) = h.handle.pane(id) {
            return pane;
        }
        assert!(
            Instant::now() < deadline,
            "the holder of {id} did not start"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The pane as `pane.show` gives it, once its state is `state`.
async fn wait_state(c: &mut Client, pane: &str, state: &str) -> Value {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let shown = c.ok("pane.show", json!({ "pane": pane })).await;
        if shown["pane"]["liveness"]["state"] == state {
            return shown["pane"].clone();
        }
        assert!(
            Instant::now() < deadline,
            "waited for {pane} to be {state}: {shown}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn alive(pid: i32) -> bool {
    Pid::from_raw(pid).is_some_and(|pid| test_kill_process(pid).is_ok())
}

async fn wait_gone(pid: i32, what: &str) {
    let deadline = Instant::now() + TIMEOUT;
    while alive(pid) {
        assert!(Instant::now() < deadline, "{what} is still alive");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A program that prints the variables of the pane and then waits.
const SHOW_ENV: &str = r#"echo "id=$NEOHAR_PANE_ID ws=$NEOHAR_WORKSPACE_ID tab=$NEOHAR_TAB_ID"
echo "n=$NEOHAR s=$NEOHAR_SESSION t=$TERM_PROGRAM"
echo "ulid=$NEOHAR_PANE_ULID"
echo "sock=$NEOHAR_SOCKET"
echo "bin=$NEOHAR_BIN"
echo "tok=$NEOHAR_PANE_TOKEN"
echo "extra=$EXTRA"
pwd
echo end
exec sleep 60"#;

fn sleeper() -> Value {
    json!({ "argv": ["/bin/sh", "-c", "exec sleep 60"] })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_of_the_layout_gets_a_holder_with_the_environment_and_a_token() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let root = std::fs::canonicalize(h.dir.join("home")).expect("the root exists");
    let created = c
        .ok(
            "workspace.create",
            json!({
                "name": "Main",
                "root": root,
                "command": {
                    "argv": ["/bin/sh", "-c", SHOW_ENV],
                    // A variable of the spec must not replace one of neohar.
                    "env": { "EXTRA": "yes", "NEOHAR_SESSION": "spoofed" },
                },
            }),
        )
        .await;
    let pane = started(&h, &created["pane"]).await;
    let text = wait_for(&pane, "the end of the output", |t| {
        t.iter().any(|row| row == "end")
    })
    .await;
    // A long line wraps at the edge of the screen, so the rows are joined before they are read.
    let screen = text.concat();
    let id = created["pane"].as_str().expect("an id");
    assert!(
        screen.contains("id=w1:p1 ws=w1 tab=w1:t1"),
        "the handles: {screen}"
    );
    assert!(
        screen.contains("n=1 s=test t=neohar"),
        "the session wins over the spec: {screen}"
    );
    assert!(screen.contains(&format!("ulid={id}")), "{screen}");
    assert!(
        screen.contains(&format!("sock={}", h.api_socket().display())),
        "{screen}"
    );
    assert!(
        screen.contains(&format!("bin={}", env!("CARGO_BIN_EXE_nh"))),
        "a test run names its own binary: {screen}"
    );
    assert!(screen.contains("extra=yes"), "{screen}");
    assert!(
        screen.contains(&format!("{}end", root.display())),
        "the cwd is the root of the workspace: {screen}"
    );

    // The token is in the pane and in the database, and it gives pane scope.
    let token = screen
        .split("tok=")
        .nth(1)
        .and_then(|rest| rest.get(..64))
        .expect("a token of 64 digits")
        .to_owned();
    assert!(token.bytes().all(|b| b.is_ascii_hexdigit()), "{token}");
    assert_eq!(
        h.db().pane_tokens().expect("the tokens"),
        [(ulid(&created["pane"]), token.clone())]
    );
    let reply = c
        .call(
            "adapter.signal",
            json!({ "token": token, "harness": "claude", "event": "Stop" }),
        )
        .await;
    assert_eq!(reply["result"], json!({}), "{reply}");

    // The layout knows that the child runs, and the row of the database has the holder.
    let shown = wait_state(&mut c, "w1:p1", "alive").await;
    assert_eq!(shown["render_id"], json!(pane.render_id()));
    let row = h
        .db()
        .pane(ulid(&created["pane"]))
        .expect("the pane is read")
        .expect("a row");
    assert_eq!(row.holder_pid, pane.holder_pid());
    assert_eq!(row.liveness, nh_store::PaneLiveness::Running);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_split_starts_a_holder_and_a_close_ends_it_with_its_token() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let created = c
        .ok("workspace.create", json!({ "command": sleeper() }))
        .await;
    let split = c
        .ok(
            "pane.split",
            json!({ "pane": "w1:p1", "direction": "right", "command": sleeper() }),
        )
        .await;
    assert_eq!(split["handle"], "w1:p2");
    let first = started(&h, &created["pane"]).await;
    let second = started(&h, &split["pane"]).await;
    wait_state(&mut c, "w1:p1", "alive").await;
    wait_state(&mut c, "w1:p2", "alive").await;
    assert_ne!(first.holder_pid(), second.holder_pid());
    let tokens = h.db().pane_tokens().expect("the tokens");
    let token_of = |pane: &Value| {
        tokens
            .iter()
            .find(|(id, _)| *id == ulid(pane))
            .map(|(_, token)| token.clone())
            .expect("a token")
    };
    let token = token_of(&split["pane"]);
    assert_ne!(token, token_of(&created["pane"]));

    // A pane that runs needs `force`.
    let reply = c.call("pane.close", json!({ "pane": "w1:p2" })).await;
    assert_eq!(error_of(&reply).1, "confirm_required", "{reply}");
    assert!(alive(second.holder_pid()));
    c.ok("pane.close", json!({ "pane": "w1:p2", "force": true }))
        .await;

    // The child and its holder end, the pane leaves the core, and its token stops.
    wait_gone(second.holder_pid(), "the holder of the closed pane").await;
    assert!(h.handle.pane(ulid(&split["pane"])).is_none());
    let reply = c
        .call(
            "adapter.signal",
            json!({ "token": token, "harness": "claude", "event": "Stop" }),
        )
        .await;
    assert_eq!(error_of(&reply).1, "permission_denied", "{reply}");
    assert!(h.db().pane(ulid(&split["pane"])).expect("read").is_none());
    // The other pane did not notice.
    assert!(alive(first.holder_pid()));
    wait_state(&mut c, "w1:p1", "alive").await;
}

/// A pane that starts processes which the kill of its child and its foreground group does not
/// reach: `a` left the session (`setsid`), `b` is a job with a group of its own (`set -m`), and
/// `c` is a job whose parent ended. The child prints their pids and waits.
const FAMILY: &str = r#"perl -MPOSIX -e 'POSIX::setsid(); exec @ARGV' sleep 301 &
a=$!
set -m
sleep 302 &
b=$!
c=$(sleep 303 >/dev/null 2>&1 & echo $!)
echo "pids $a $b $c"
exec sleep 60"#;

/// The pids that [`FAMILY`] prints, once it printed them.
async fn family_of(pane: &Pane) -> Vec<i32> {
    let text = wait_for(pane, "the pids of the family", |t| {
        t.iter().any(|row| row.starts_with("pids "))
    })
    .await;
    let row = text
        .iter()
        .find_map(|row| row.strip_prefix("pids "))
        .expect("a row of pids");
    let pids: Vec<i32> = row
        .split_whitespace()
        .map(|pid| pid.parse().expect("a pid"))
        .collect();
    assert_eq!(pids.len(), 3, "{row}");
    pids
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_closed_tab_or_workspace_kills_every_process_that_its_panes_started() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let command = json!({ "argv": ["/bin/sh", "-c", FAMILY] });
    let created = c
        .ok("workspace.create", json!({ "command": command }))
        .await;
    let tab = c
        .ok(
            "tab.create",
            json!({ "workspace": "w1", "command": command }),
        )
        .await;
    let first = started(&h, &created["pane"]).await;
    let second = started(&h, &tab["pane"]).await;
    let first_family = family_of(&first).await;
    let second_family = family_of(&second).await;
    for pid in first_family.iter().chain(&second_family) {
        assert!(alive(*pid), "{pid} runs before the close");
    }

    // The tab closes: its processes end, and the ones of the other tab run on.
    c.ok("tab.close", json!({ "tab": tab["tab"], "force": true }))
        .await;
    for pid in &second_family {
        wait_gone(*pid, &format!("process {pid} of the closed tab")).await;
    }
    wait_gone(second.holder_pid(), "the holder of the closed tab").await;
    for pid in &first_family {
        assert!(alive(*pid), "{pid} of the other tab still runs");
    }

    // The workspace closes with its last tab.
    c.ok(
        "workspace.close",
        json!({ "workspace": "w1", "force": true }),
    )
    .await;
    for pid in &first_family {
        wait_gone(*pid, &format!("process {pid} of the closed workspace")).await;
    }
    wait_gone(first.holder_pid(), "the holder of the closed workspace").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_child_that_ends_is_reported_and_pane_restart_starts_it_again() {
    let h = Harness::start();
    let mut watcher = client(&h).await;
    let events = watcher
        .ok(
            "events.subscribe",
            json!({ "filter": { "kinds": ["pane.exited", "pane.restarted"] } }),
        )
        .await;
    let events_id = events["subscription"].as_str().expect("an id").to_owned();
    let mut c = client(&h).await;
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", "echo first; exit 3"] } }),
        )
        .await;
    let first = started(&h, &created["pane"]).await;
    let ended = wait_state(&mut c, "w1:p1", "exited").await;
    assert_eq!(ended["liveness"], json!({ "state": "exited", "code": 3 }));
    let line = loop {
        let line = watcher.recv().await;
        if line["method"] == "events.event" && line["params"]["subscription"] == events_id.as_str()
        {
            break line;
        }
    };
    assert_eq!(line["params"]["event"]["kind"], "pane.exited");
    assert_eq!(line["params"]["event"]["payload"]["code"], 3);
    let row = h
        .db()
        .pane(ulid(&created["pane"]))
        .expect("the pane is read")
        .expect("a row");
    assert_eq!(row.liveness, nh_store::PaneLiveness::Exited);

    // A program that ends can start again in the same pane, with another command.
    c.ok(
        "pane.restart",
        json!({
            "pane": "w1:p1",
            "command": { "argv": ["/bin/sh", "-c", "echo second; exec sleep 60"] },
        }),
    )
    .await;
    wait_state(&mut c, "w1:p1", "alive").await;
    let second = started(&h, &created["pane"]).await;
    assert!(
        !Arc::ptr_eq(&first, &second),
        "a restart makes a new pane object"
    );
    assert_ne!(first.holder_pid(), second.holder_pid());
    let text = wait_for(&second, "the output of the new program", |t| {
        t.iter().any(|row| row == "second")
    })
    .await;
    assert!(
        !text.iter().any(|row| row == "first"),
        "the screen of the old run is gone: {text:?}"
    );
    let line = loop {
        let line = watcher.recv().await;
        if line["method"] == "events.event" && line["params"]["subscription"] == events_id.as_str()
        {
            break line;
        }
    };
    assert_eq!(line["params"]["event"]["kind"], "pane.restarted");

    // A program that runs cannot restart.
    let reply = c.call("pane.restart", json!({ "pane": "w1:p1" })).await;
    assert_eq!(error_of(&reply).1, "invalid_params", "{reply}");
    let row = h
        .db()
        .pane(ulid(&created["pane"]))
        .expect("the pane is read")
        .expect("a row");
    assert_eq!(row.liveness, nh_store::PaneLiveness::Running);
    assert_eq!(row.holder_pid, second.holder_pid());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_panes_of_the_layout_and_their_tokens_come_back_after_a_restart_of_the_core() {
    let mut h = Harness::start();
    let created = {
        let mut c = client(&h).await;
        c.ok("workspace.create", json!({ "command": sleeper() }))
            .await
    };
    let before = started(&h, &created["pane"]).await;
    let token = h.db().pane_tokens().expect("the tokens")[0].1.clone();

    h.crash().await;
    h.restart();
    let after = h.restored(ulid(&created["pane"])).await;
    assert_eq!(after.holder_pid(), before.holder_pid(), "the same holder");
    let mut c = client(&h).await;
    let shown = wait_state(&mut c, "w1:p1", "alive").await;
    assert_eq!(shown["render_id"], json!(after.render_id()));
    // The shim that waited for the new core finds its token valid.
    let reply = c
        .call(
            "adapter.signal",
            json!({ "token": token, "harness": "claude", "event": "Stop" }),
        )
        .await;
    assert_eq!(reply["result"], json!({}), "{reply}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_that_the_last_core_made_and_did_not_start_gets_its_holder() {
    let mut h = Harness::start_layout();
    let created = {
        let mut c = client(&h).await;
        let created = c
            .ok("workspace.create", json!({ "command": sleeper() }))
            .await;
        let shown = c.ok("pane.show", json!({ "pane": "w1:p1" })).await;
        assert_eq!(shown["pane"]["liveness"], json!({ "state": "starting" }));
        created
    };
    assert!(h.handle.pane(ulid(&created["pane"])).is_none());

    h.crash().await;
    h.restart_tuned(Tuning::default());
    let pane = started(&h, &created["pane"]).await;
    let mut c = client(&h).await;
    wait_state(&mut c, "w1:p1", "alive").await;
    assert!(alive(pane.holder_pid()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_holder_that_does_not_start_leaves_a_pane_that_can_restart() {
    // The core cannot run the sidecar.
    let h = Harness::start_with("/nonexistent/nh".into());
    let mut c = client(&h).await;
    c.ok("workspace.create", json!({ "command": sleeper() }))
        .await;
    let shown = wait_state(&mut c, "w1:p1", "exited").await;
    assert_eq!(shown["liveness"], json!({ "state": "exited" }));
    // The pane stays in the layout, and a restart asks for a holder again.
    c.ok("pane.restart", json!({ "pane": "w1:p1" })).await;
    wait_state(&mut c, "w1:p1", "exited").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_that_closes_while_its_holder_starts_leaves_no_process() {
    // The core waits a second between the start of the holder and the connection to it.
    let h = Harness::start_tuned(Tuning {
        attach_delay: Duration::from_secs(1),
        ..Tuning::default()
    });
    let mut c = client(&h).await;
    let created = c
        .ok("workspace.create", json!({ "command": sleeper() }))
        .await;
    let socket = h
        .paths
        .holder_socket("test", ulid(&created["pane"]))
        .expect("the socket path");
    // The pane has no process yet, so it closes without `force`.
    c.ok("workspace.close", json!({ "workspace": "w1" })).await;

    // The holder started anyway. The core ends it when it sees that the pane is gone.
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let refused = tokio::net::UnixStream::connect(&socket).await.is_err();
        if refused && socket.exists() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the holder of a pane that closed still listens"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(h.handle.pane(ulid(&created["pane"])).is_none());
    assert!(h.db().pane(ulid(&created["pane"])).expect("read").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_of_the_layout_and_a_pane_outside_it_never_share_a_render_id() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let outside = h.spawn(&["/bin/sh", "-c", "exec sleep 60"], 80, 24).await;
    let created = c
        .ok("workspace.create", json!({ "command": sleeper() }))
        .await;
    let layout = started(&h, &created["pane"]).await;
    assert_ne!(outside.render_id(), layout.render_id());
    let next = h.spawn(&["/bin/sh", "-c", "exec sleep 60"], 80, 24).await;
    assert!(next.render_id() > layout.render_id());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_whose_holder_is_gone_starts_again_with_a_shell_where_it_ran() {
    let mut h = Harness::start();
    let work = std::fs::canonicalize(h.dir.join("home"))
        .expect("the home exists")
        .join("work");
    std::fs::create_dir_all(&work).expect("the work directory");
    let created = {
        let mut c = client(&h).await;
        c.ok(
            "workspace.create",
            json!({ "root": work, "command": sleeper() }),
        )
        .await
    };
    let before = started(&h, &created["pane"]).await;
    let holder = before.holder_pid();
    h.crash().await;
    // The holder and the child die while no core runs, as at a reboot.
    let pid = Pid::from_raw(holder).expect("a pid");
    rustix::process::kill_process(pid, rustix::process::Signal::KILL).expect("a kill");
    wait_gone(holder, "the killed holder").await;
    h.restart();

    // The pane is not in the core until the new holder runs.
    let after = started(&h, &created["pane"]).await;
    assert_ne!(after.holder_pid(), holder);
    let mut c = client(&h).await;
    wait_state(&mut c, "w1:p1", "alive").await;
    // The new program is the login shell, in the directory of the old one.
    h.text(&after, "pwd\r").await;
    // The terminal can echo the command before the shell prints its first prompt. The path then
    // follows that prompt on the same row.
    let path = work.display().to_string();
    wait_for(&after, "the working directory", |t| {
        t.iter().any(|row| row.ends_with(&path))
    })
    .await;
    // The row has the new holder, and the same handle.
    let row = h
        .db()
        .pane(ulid(&created["pane"]))
        .expect("the pane is read")
        .expect("a row");
    assert_eq!(row.holder_pid, after.holder_pid());
    assert_eq!(row.argv.first().map(String::as_str), Some("/bin/sh"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_paste_goes_to_the_pane_and_bytes_and_dropped_files_follow_the_rules() {
    let h = Harness::start();
    let mut c = client(&h).await;
    // `cat` shows what the terminal sends to it.
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", "stty -echo; exec cat"] } }),
        )
        .await;
    let pane = started(&h, &created["pane"]).await;
    wait_state(&mut c, "w1:p1", "alive").await;

    // A paste by handle. Line ends become carriage returns, which the tty turns into line ends.
    let result = c
        .ok(
            "pane.paste",
            json!({ "pane": "w1:p1", "input_id": 9001, "text": "one\ntwo\n" }),
        )
        .await;
    assert_eq!(result, json!({}));
    wait_for(&pane, "the pasted lines", |t| {
        t.iter().any(|row| row == "one") && t.iter().any(|row| row == "two")
    })
    .await;

    // Raw bytes by id, as base64.
    let b64 = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(b"raw bytes\n")
    };
    c.ok(
        "pane.send_bytes",
        json!({ "pane": created["pane"], "input_id": 9002, "bytes": b64 }),
    )
    .await;
    wait_for(&pane, "the raw bytes", |t| {
        t.iter().any(|row| row == "raw bytes")
    })
    .await;

    // Dropped files come back as quoted paths.
    // `pane.drop_files` is a method of the UI, not of the CLI.
    h.call(
        "pane.drop_files",
        json!({ "pane": "w1:p1", "input_id": 9003, "paths": ["/tmp/a b", "/tmp/it's"] }),
    )
    .await;
    // The line discipline gives `cat` the line when it ends.
    c.ok(
        "pane.send_bytes",
        json!({ "pane": "w1:p1", "input_id": 9007, "bytes": "Cg==" }),
    )
    .await;
    wait_for(&pane, "the quoted paths", |t| {
        t.iter()
            .any(|row| row.contains("'/tmp/a b' '/tmp/it'\\''s'"))
    })
    .await;

    // The ids of the inputs are the ones that the caller gave.
    let acked = h
        .call(
            "debug.pane_inputs",
            json!({ "pane": created["pane"].clone() }),
        )
        .await;
    for id in [9001, 9002, 9003] {
        assert!(
            acked["acked"]
                .as_array()
                .is_some_and(|ids| ids.contains(&json!(id))),
            "{id} is acked: {acked}"
        );
    }

    // A NUL in a path and a huge write are refused, and a pane that is not there is an error.
    let error = h
        .handle
        .rpc(
            "pane.drop_files",
            json!({ "pane": "w1:p1", "input_id": 9004, "paths": ["/a\0b"] }),
        )
        .await
        .expect_err("a NUL in a path");
    assert_eq!(error.code, ErrorCode::InvalidParams);
    let reply = c
        .call(
            "pane.paste",
            json!({ "pane": "w1:p1", "input_id": 9005, "text": "x".repeat(2 << 20) }),
        )
        .await;
    assert_eq!(error_of(&reply).1, "invalid_params", "{reply}");
    let reply = c
        .call(
            "pane.paste",
            json!({ "pane": "w1:p9", "input_id": 9006, "text": "x" }),
        )
        .await;
    assert_eq!(error_of(&reply).1, "not_found", "{reply}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_without_a_process_refuses_a_paste() {
    // The core cannot run the sidecar, so the pane exists in the layout and has no process.
    let h = Harness::start_with("/nonexistent/nh".into());
    let mut c = client(&h).await;
    c.ok("workspace.create", json!({ "command": sleeper() }))
        .await;
    wait_state(&mut c, "w1:p1", "exited").await;
    let reply = c
        .call(
            "pane.paste",
            json!({ "pane": "w1:p1", "input_id": 1, "text": "x" }),
        )
        .await;
    assert_eq!(error_of(&reply).1, "pane_gone", "{reply}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_everything_asks_politely_and_kills_the_children_that_refuse() {
    let h = Harness::start();
    let mut c = client(&h).await;
    // A login shell obeys `Hup`. A program obeys `Term`. The last one ignores both.
    let shell = c.ok("workspace.create", json!({})).await;
    let program = c
        .ok(
            "pane.split",
            json!({ "pane": "w1:p1", "direction": "right", "command": sleeper() }),
        )
        .await;
    let stubborn = c
        .ok(
            "pane.split",
            json!({
                "pane": "w1:p1",
                "direction": "down",
                "command": { "argv": ["/bin/sh", "-c", "trap '' TERM HUP; while :; do sleep 1; done"] },
            }),
        )
        .await;
    let mut panes = Vec::new();
    for (handle, created) in [("w1:p1", &shell), ("w1:p2", &program), ("w1:p3", &stubborn)] {
        wait_state(&mut c, handle, "alive").await;
        panes.push(started(&h, &created["pane"]).await);
    }
    // Let the shells run their `trap` before the signals come.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let started_at = Instant::now();
    let report = h.handle.stop_everything(Duration::from_millis(1500)).await;
    assert_eq!(
        report,
        StopReport {
            ended: 2,
            killed: 1,
            left: 0
        }
    );
    assert!(
        started_at.elapsed() >= Duration::from_millis(1400),
        "the stubborn child got its time"
    );
    // Every child is gone, every pane shows it, and every holder leaves.
    for pane in &panes {
        assert!(
            matches!(pane.info().liveness, Liveness::Exited(_)),
            "{:?}",
            pane.info().liveness
        );
        wait_gone(pane.child_pid(), "a child").await;
        // The call waits for the holders, so a host that drops the core at once leaves none.
        assert!(!alive(pane.holder_pid()), "a holder left behind");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_stop_everything_with_force_kills_at_once_and_the_core_stays() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let stubborn = c
        .ok(
            "workspace.create",
            json!({
                "command": { "argv": ["/bin/sh", "-c", "trap '' TERM HUP; while :; do sleep 1; done"] },
            }),
        )
        .await;
    wait_state(&mut c, "w1:p1", "alive").await;
    let pane = started(&h, &stubborn["pane"]).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    // The CLI calls it. With `force` there is no grace: the child gets `Kill` at once.
    let started_at = Instant::now();
    let report = c
        .ok("session.stop_everything", json!({ "force": true }))
        .await;
    assert_eq!(report, json!({"ended": 0, "killed": 1, "left": 0}));
    assert!(
        started_at.elapsed() < Duration::from_secs(4),
        "no grace with force: {:?}",
        started_at.elapsed()
    );
    wait_gone(pane.child_pid(), "the child").await;
    assert!(!alive(pane.holder_pid()), "a holder left behind");
    // The core keeps running: a stop is not a shutdown.
    assert_eq!(
        c.ok("session.info", json!({})).await["pid"],
        json!(std::process::id())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_shutdown_asks_a_host_that_accepts_it_and_is_for_the_cli() {
    let h = Harness::start();
    let mut c = client(&h).await;
    // A host that does not accept it, as the app: it quits from its menu.
    let reply = c.call("session.shutdown", json!({})).await;
    assert_eq!(error_of(&reply).1, "unsupported", "{reply}");
    // The UI may not call it.
    let denied = h.handle.rpc("session.shutdown", json!({})).await;
    assert_eq!(denied.map_err(|e| e.code), Err(ErrorCode::PermissionDenied));
    // A host that accepts it hears the request, and the call answers before the host ends.
    let (tx, rx) = std::sync::mpsc::channel();
    h.handle.accept_shutdown(move || {
        let _ = tx.send(());
    });
    assert_eq!(c.ok("session.shutdown", json!({})).await, json!({}));
    assert!(
        rx.recv_timeout(Duration::from_secs(5)).is_ok(),
        "the host heard it"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_daemon_wants_the_machine_awake_by_the_setting() {
    let h = Harness::start();
    let mut c = client(&h).await;
    // By default only while an agent works or a gate holds a hook: nothing does here.
    assert!(!h.handle.wants_awake().await);
    for (value, awake) in [
        ("always", true),
        ("never", false),
        ("while_agents_work", false),
    ] {
        c.ok(
            "config.set",
            json!({"key": "daemon.prevent_sleep", "value": value}),
        )
        .await;
        assert_eq!(h.handle.wants_awake().await, awake, "{value}");
    }
    let wrong = c
        .call(
            "config.set",
            json!({"key": "daemon.prevent_sleep", "value": "sometimes"}),
        )
        .await;
    assert!(wrong.get("error").is_some(), "{wrong}");
}

/// The text of the lines that the archive holds for a pane, from line 0.
fn archived_texts(h: &Harness, pane: &Value) -> Vec<String> {
    let lines = h
        .db()
        .read_lines(ulid(pane), 0, 10_000)
        .expect("the archive is read");
    for (n, line) in lines.iter().enumerate() {
        assert_eq!(line.line_no, n as u64, "no gap and no repeat: {line:?}");
    }
    lines.into_iter().map(|line| line.text).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_lines_that_scroll_off_the_screen_go_to_the_archive() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", "seq 1 100; exec sleep 60"] } }),
        )
        .await;
    let pane = started(&h, &created["pane"]).await;
    wait_for(&pane, "the end of the numbers", |t| {
        t.iter().any(|row| row == "100")
    })
    .await;
    // The archive writes after a second at most.
    let deadline = Instant::now() + TIMEOUT;
    let texts = loop {
        let texts = archived_texts(&h, &created["pane"]);
        if texts.len() >= 70 {
            break texts;
        }
        assert!(
            Instant::now() < deadline,
            "the archive has {} lines",
            texts.len()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let expected: Vec<String> = (1..=texts.len()).map(|n| n.to_string()).collect();
    assert_eq!(texts, expected);
    // The row of the pane knows how far the archive goes.
    let segments = h
        .db()
        .scrollback_segments(ulid(&created["pane"]))
        .expect("segments");
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].last_line, texts.len() as u64 - 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_program_that_sets_its_title_all_the_time_reaches_the_projection_four_times_a_second() {
    let h = Harness::start();
    let mut panes = Messages::subscribe(&h.handle, "panes");
    let mut c = client(&h).await;
    // 50 titles, one every 20 ms, then a pause, then the last one.
    let script = r#"i=0; while [ $i -lt 50 ]; do printf '\033]0;t%d\007' $i; i=$((i+1)); sleep 0.02; done; sleep 1; printf '\033]0;last\007'; exec sleep 60"#;
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", script] } }),
        )
        .await;
    let id = created["pane"].as_str().expect("an id").to_owned();
    let mut titles: Vec<String> = Vec::new();
    let deadline = Instant::now() + TIMEOUT;
    while titles.last().map(String::as_str) != Some("last") {
        assert!(Instant::now() < deadline, "the titles so far: {titles:?}");
        let message = panes
            .wait_for("a title", |m| {
                m["type"] == "patch" || m["type"] == "snapshot"
            })
            .await;
        let rows: Vec<&Value> = match message["type"].as_str() {
            Some("snapshot") => message["data"]["items"].get(&id).into_iter().collect(),
            _ => message["ops"]
                .as_array()
                .map(|ops| {
                    ops.iter()
                        .filter(|op| op["op"] == "set" && op["key"] == id.as_str())
                        .map(|op| &op["value"])
                        .collect()
                })
                .unwrap_or_default(),
        };
        for row in rows {
            let title = row["title"].as_str().unwrap_or_default().to_owned();
            if !title.is_empty() && titles.last() != Some(&title) {
                titles.push(title);
            }
        }
    }
    // The first title shows at once, and a title follows each quarter of a second. The title
    // after the pause is the last one. Without the limit there would be 50 changes.
    assert!(
        titles.len() <= 9,
        "{} titles reached the projection: {titles:?}",
        titles.len()
    );
}

/// The `cwd` of the pane as `pane.show` gives it, once it is `want`.
async fn wait_cwd(c: &mut Client, pane: &str, want: &str) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let shown = c.ok("pane.show", json!({ "pane": pane })).await;
        if shown["pane"]["cwd"] == want {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "waited for the cwd {want} of {pane}: {shown}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_cwd_of_a_pane_follows_its_foreground_process() {
    let h = Harness::start();
    let mut c = client(&h).await;
    // The system gives the real path of a folder (`/tmp` is `/private/tmp` on macOS).
    let root = h.dir.canonicalize().expect("the folder of the test");
    let below = root.join("below");
    std::fs::create_dir(&below).expect("a folder");
    let flag = root.join("go");
    let script = format!(
        "while [ ! -e '{}' ]; do sleep 0.05; done; cd below && exec sleep 60",
        flag.display()
    );
    let created = c
        .ok(
            "workspace.create",
            json!({
                "root": root.display().to_string(),
                "command": { "argv": ["/bin/sh", "-c", script] },
            }),
        )
        .await;
    let id = created["pane"].as_str().expect("an id").to_owned();
    wait_state(&mut c, &id, "alive").await;
    wait_cwd(&mut c, &id, &root.display().to_string()).await;
    std::fs::write(&flag, b"").expect("the flag");
    // The watcher of the pane walks once a second, so the new folder shows within a second or so.
    wait_cwd(&mut c, &id, &below.display().to_string()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_archive_has_no_gap_and_no_repeat_across_a_restart_of_the_core() {
    let tuning = Tuning {
        checkpoint_idle: Duration::from_millis(100),
        ..Tuning::default()
    };
    let mut h = Harness::start_tuned(tuning.clone());
    let flag = h.dir.join("go");
    let script = format!(
        "seq 1 100; while [ ! -e {} ]; do sleep 0.05; done; seq 101 200; exec sleep 60",
        flag.display()
    );
    let created = {
        let mut c = client(&h).await;
        c.ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", script] } }),
        )
        .await
    };
    let pane = started(&h, &created["pane"]).await;
    wait_for(&pane, "the first numbers", |t| {
        t.iter().any(|row| row == "100")
    })
    .await;
    // A snapshot of this screen exists before the core ends.
    h.wait_snapshot(ulid(&created["pane"]), 1).await;

    h.crash().await;
    h.restart_tuned(tuning);
    let pane = h.restored(ulid(&created["pane"])).await;
    std::fs::write(&flag, b"").expect("the flag file");
    wait_for(&pane, "the last numbers", |t| {
        t.iter().any(|row| row == "200")
    })
    .await;

    let deadline = Instant::now() + TIMEOUT;
    let texts = loop {
        let texts = archived_texts(&h, &created["pane"]);
        if texts.len() >= 170 {
            break texts;
        }
        assert!(
            Instant::now() < deadline,
            "the archive has {} lines",
            texts.len()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let expected: Vec<String> = (1..=texts.len()).map(|n| n.to_string()).collect();
    assert_eq!(texts, expected, "the numbers follow each other");
}

fn recording_host() -> Arc<RecordingHost> {
    Arc::new(RecordingHost::new(env!("CARGO_BIN_EXE_nh").into(), false))
}

/// The calls of the host so far that `pick` takes.
fn host_calls<T>(host: &RecordingHost, pick: impl Fn(&HostCall) -> Option<T>) -> Vec<T> {
    host.calls().iter().filter_map(pick).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_bell_is_an_event_at_most_every_two_seconds() {
    let h = Harness::start();
    let mut watcher = client(&h).await;
    let events = watcher
        .ok(
            "events.subscribe",
            json!({ "filter": { "kinds": ["pane.bell"] } }),
        )
        .await;
    let events_id = events["subscription"].as_str().expect("an id").to_owned();
    let mut c = client(&h).await;
    // Three bells at once, a pause, and one more.
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c",
                "printf '\\a\\a\\a'; sleep 2.3; printf '\\a'; exec sleep 60"] } }),
        )
        .await;
    let mut bells = Vec::new();
    while bells.len() < 2 {
        let line = watcher.recv().await;
        if line["method"] == "events.event" && line["params"]["subscription"] == events_id.as_str()
        {
            bells.push(line["params"]["event"].clone());
        }
    }
    assert!(bells.iter().all(|e| e["kind"] == "pane.bell"));
    assert!(
        bells
            .iter()
            .all(|e| e["payload"]["pane"] == created["pane"])
    );
    // No third event follows the first burst.
    let more = watcher.try_recv(Duration::from_millis(700)).await;
    assert!(more.is_none(), "{more:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_notification_from_the_program_goes_to_the_host_with_a_limit() {
    let host = recording_host();
    let h = Harness::start_hosted(host.clone());
    let mut c = client(&h).await;
    // OSC 9, OSC 777, then four more of OSC 9 at once.
    let script = r#"printf '\033]9;built it\007'; printf '\033]777;notify;Codex;needs approval\033\\'; for n in 1 2 3 4; do printf '\033]9;spam %d\007' $n; done; echo end; exec sleep 60"#;
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", script] } }),
        )
        .await;
    let pane = started(&h, &created["pane"]).await;
    wait_for(&pane, "the end", |t| t.iter().any(|row| row == "end")).await;
    let deadline = Instant::now() + TIMEOUT;
    while host_calls(&host, |call| {
        matches!(call, HostCall::Notify(..)).then_some(())
    })
    .len()
        < 3
    {
        assert!(Instant::now() < deadline, "{:?}", host.calls());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let notes = host_calls(&host, |call| match call {
        HostCall::Notify(_, note) => Some((note.title.clone(), note.body.clone())),
        _ => None,
    });
    assert_eq!(
        notes,
        [
            ("neohar".to_owned(), "built it".to_owned()),
            ("Codex".to_owned(), "needs approval".to_owned()),
            ("neohar".to_owned(), "spam 1".to_owned()),
        ],
        "three in 10 seconds, the title falls back to the name of the app"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_osc_52_write_asks_once_and_the_answer_can_be_remembered() {
    let host = recording_host();
    let h = Harness::start_hosted(host.clone());
    let mut c = client(&h).await;
    let flag = h.dir.join("again");
    // The first write, then a wait for the flag file, then two more writes.
    let script = format!(
        r#"printf '\033]52;c;%s\007' "$(printf 'first text' | base64)"; echo one;
while [ ! -e {flag} ]; do sleep 0.05; done;
printf '\033]52;c;%s\007' "$(printf 'second text' | base64)"; echo two;
printf '\033]52;c;%s\007' "$(printf 'third text' | base64)"; echo three;
exec sleep 60"#,
        flag = flag.display()
    );
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", script] } }),
        )
        .await;
    let pane = started(&h, &created["pane"]).await;
    wait_for(&pane, "the first write", |t| {
        t.iter().any(|row| row == "one")
    })
    .await;

    // The write waits: the row of the pane shows the request, and the host got nothing.
    let deadline = Instant::now() + TIMEOUT;
    let request = loop {
        let shown = c.ok("pane.show", json!({ "pane": "w1:p1" })).await;
        if !shown["pane"]["clipboard_request"].is_null() {
            break shown["pane"]["clipboard_request"].clone();
        }
        assert!(Instant::now() < deadline, "no request: {shown}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(
        request,
        json!({ "bytes": 10, "preview": "first text", "allowed": false })
    );
    assert!(
        host_calls(&host, |c| matches!(c, HostCall::ClipboardWrite(_))
            .then_some(()))
        .is_empty()
    );

    // `pane.clipboard_allow` is a method of the UI. Allow, and remember the answer.
    h.call(
        "pane.clipboard_allow",
        json!({ "pane": "w1:p1", "allow": true, "remember": true }),
    )
    .await;
    assert_eq!(
        host_calls(&host, |c| match c {
            HostCall::ClipboardWrite(text) => Some(text.clone()),
            _ => None,
        }),
        ["first text"]
    );
    let shown = c.ok("pane.show", json!({ "pane": "w1:p1" })).await;
    assert!(shown["pane"]["clipboard_request"].is_null(), "{shown}");
    assert_eq!(
        h.db()
            .clipboard_policy(ulid(&created["pane"]))
            .expect("read"),
        Some(true)
    );
    let error = h
        .handle
        .rpc(
            "pane.clipboard_allow",
            json!({ "pane": "w1:p1", "allow": true }),
        )
        .await
        .expect_err("nothing waits");
    assert_eq!(error.code, ErrorCode::NotFound);

    // The later writes follow the answer without a question.
    std::fs::write(&flag, b"").expect("the flag file");
    wait_for(&pane, "the other writes", |t| {
        t.iter().any(|row| row == "three")
    })
    .await;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let texts = host_calls(&host, |c| match c {
            HostCall::ClipboardWrite(text) => Some(text.clone()),
            _ => None,
        });
        if texts.len() == 3 {
            assert_eq!(texts, ["first text", "second text", "third text"]);
            break;
        }
        assert!(Instant::now() < deadline, "{texts:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let shown = c.ok("pane.show", json!({ "pane": "w1:p1" })).await;
    assert!(shown["pane"]["clipboard_request"].is_null(), "{shown}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_denied_and_remembered_clipboard_write_is_dropped_for_good() {
    let host = recording_host();
    let h = Harness::start_hosted(host.clone());
    let mut c = client(&h).await;
    let flag = h.dir.join("again");
    let script = format!(
        r#"printf '\033]52;c;%s\007' "$(printf 'one' | base64)"; echo one;
while [ ! -e {flag} ]; do sleep 0.05; done;
printf '\033]52;c;%s\007' "$(printf 'two' | base64)"; echo two; exec sleep 60"#,
        flag = flag.display()
    );
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", script] } }),
        )
        .await;
    let pane = started(&h, &created["pane"]).await;
    wait_for(&pane, "the first write", |t| {
        t.iter().any(|row| row == "one")
    })
    .await;
    let deadline = Instant::now() + TIMEOUT;
    while c.ok("pane.show", json!({ "pane": "w1:p1" })).await["pane"]["clipboard_request"].is_null()
    {
        assert!(Instant::now() < deadline, "no request");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    h.call(
        "pane.clipboard_allow",
        json!({ "pane": "w1:p1", "allow": false, "remember": true }),
    )
    .await;
    std::fs::write(&flag, b"").expect("the flag file");
    wait_for(&pane, "the second write", |t| {
        t.iter().any(|row| row == "two")
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        host_calls(&host, |c| matches!(c, HostCall::ClipboardWrite(_))
            .then_some(()))
        .is_empty()
    );
    let shown = c.ok("pane.show", json!({ "pane": "w1:p1" })).await;
    assert!(shown["pane"]["clipboard_request"].is_null(), "{shown}");
    assert_eq!(
        h.db()
            .clipboard_policy(ulid(&created["pane"]))
            .expect("read"),
        Some(false)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_foreground_process_group_reaches_the_watch_of_the_pane() {
    let h = Harness::start();
    let mut c = client(&h).await;
    // The login shell of the harness is interactive, so it runs a command in a group of its own.
    let created = c.ok("workspace.create", json!({})).await;
    let pane = started(&h, &created["pane"]).await;
    wait_state(&mut c, "w1:p1", "alive").await;
    let mut foreground = pane.foreground();
    let shell = pane.child_pid();
    h.text(&pane, "sleep 30\r").await;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let now = *foreground.borrow_and_update();
        if now.is_some_and(|pgid| pgid != shell) {
            break;
        }
        assert!(Instant::now() < deadline, "the group stayed {now:?}");
        let _ = tokio::time::timeout(Duration::from_millis(200), foreground.changed()).await;
    }
    // Control-C ends `sleep`, and the shell has the foreground again.
    h.call(
        "pane.send_bytes",
        json!({ "pane": created["pane"], "input_id": 7001, "bytes": "Aw==" }),
    )
    .await;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if *foreground.borrow_and_update() == Some(shell) {
            break;
        }
        assert!(Instant::now() < deadline, "{:?}", *foreground.borrow());
        let _ = tokio::time::timeout(Duration::from_millis(200), foreground.changed()).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_input_that_the_holder_does_not_confirm_is_an_event_and_a_fact_of_the_pane() {
    let h = Harness::start_tuned(Tuning {
        input_ack_timeout: Duration::from_millis(300),
        ..Tuning::default()
    });
    let mut watcher = client(&h).await;
    let events = watcher
        .ok(
            "events.subscribe",
            json!({ "filter": { "kinds": ["pane.input_unconfirmed"] } }),
        )
        .await;
    let events_id = events["subscription"].as_str().expect("an id").to_owned();
    let mut c = client(&h).await;
    let created = c
        .ok("workspace.create", json!({ "command": sleeper() }))
        .await;
    let pane = started(&h, &created["pane"]).await;
    wait_state(&mut c, "w1:p1", "alive").await;

    // A holder that is stopped writes nothing and acknowledges nothing.
    let holder = Pid::from_raw(pane.holder_pid()).expect("a pid");
    rustix::process::kill_process(holder, rustix::process::Signal::STOP).expect("stop");
    c.ok(
        "pane.send_bytes",
        json!({ "pane": "w1:p1", "input_id": 4242, "bytes": "eA==" }),
    )
    .await;
    let event = loop {
        let line = watcher.recv().await;
        if line["method"] == "events.event" && line["params"]["subscription"] == events_id.as_str()
        {
            break line["params"]["event"].clone();
        }
    };
    rustix::process::kill_process(holder, rustix::process::Signal::CONT).expect("continue");
    assert_eq!(event["kind"], "pane.input_unconfirmed");
    assert_eq!(
        event["payload"],
        json!({ "pane": created["pane"], "input_id": 4242, "reason": "not_written" })
    );
    let shown = c.ok("pane.show", json!({ "pane": "w1:p1" })).await;
    assert_eq!(shown["pane"]["input_unconfirmed"], json!([4242]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn effects_of_the_first_output_of_a_new_pane_happen_when_the_core_attaches_late() {
    // The core waits before it connects to the holder, so the child prints first. A fast child
    // does this by chance on a busy machine.
    let host = recording_host();
    let h = Harness::start_hosted_tuned(
        host.clone(),
        Tuning {
            attach_delay: Duration::from_millis(600),
            ..Tuning::default()
        },
    );
    let mut c = client(&h).await;
    let script = r#"printf '\033]9;too early\007\033]52;c;%s\007' "$(printf 'copied' | base64)"; echo end; exec sleep 60"#;
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", script] } }),
        )
        .await;
    let pane = started(&h, &created["pane"]).await;
    wait_for(&pane, "the end", |t| t.iter().any(|row| row == "end")).await;
    let deadline = Instant::now() + TIMEOUT;
    let notes = loop {
        let notes = host_calls(&host, |call| match call {
            HostCall::Notify(_, note) => Some(note.body.clone()),
            _ => None,
        });
        if !notes.is_empty() {
            break notes;
        }
        assert!(
            Instant::now() < deadline,
            "no notification: {:?}",
            host.calls()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(notes, ["too early"]);
    // The clipboard write waits for the answer, as one from a pane that has run for a while.
    let shown = c.ok("pane.show", json!({ "pane": "w1:p1" })).await;
    assert_eq!(
        shown["pane"]["clipboard_request"],
        json!({ "bytes": 6, "preview": "copied", "allowed": false })
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_osc_52_write_goes_to_the_client_that_sent_the_last_input() {
    // The clipboard of the client that sent the last input to the pane. With that
    // client gone, the write is dropped and the pane shows it.
    let local = recording_host();
    let h = Harness::start_hosted(local.clone());
    let laptop = recording_host();
    let laptop_device = Ulid(0x0200_0000_0000_0000_0000_0000_0000_0002);
    let attachment = h.handle.attach_client(
        nh_core::ClientInfo {
            device: laptop_device,
            name: "laptop".to_owned(),
        },
        laptop.clone(),
    );
    let mut c = client(&h).await;
    let script = r#"read first; printf '\033]52;c;%s\007' "$(printf 'first text' | base64)"; echo one;
read second; printf '\033]52;c;%s\007' "$(printf 'second text' | base64)"; echo two;
exec sleep 60"#;
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", script] } }),
        )
        .await;
    let pane = started(&h, &created["pane"]).await;
    let laptop_origin = nh_core::Origin::ui().from_device(laptop_device);
    // The laptop types the first line.
    h.handle
        .call_as(
            laptop_origin,
            "pane.send_bytes",
            json!({ "pane": "w1:p1", "input_id": 9101, "bytes": "Cg==" }),
        )
        .await
        .expect("the input goes");
    wait_for(&pane, "the first write", |t| {
        t.iter().any(|row| row == "one")
    })
    .await;
    let deadline = Instant::now() + TIMEOUT;
    while c.ok("pane.show", json!({ "pane": "w1:p1" })).await["pane"]["clipboard_request"].is_null()
    {
        assert!(Instant::now() < deadline, "no request");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    h.handle
        .call_as(
            laptop_origin,
            "pane.clipboard_allow",
            json!({ "pane": "w1:p1", "allow": true, "remember": true }),
        )
        .await
        .expect("the answer goes");
    let writes = |host: &RecordingHost| {
        host_calls(host, |c| match c {
            HostCall::ClipboardWrite(text) => Some(text.clone()),
            _ => None,
        })
    };
    assert_eq!(writes(&laptop), ["first text"]);
    assert!(
        writes(&local).is_empty(),
        "the host in the process gets nothing"
    );

    // The laptop goes. A line from the CLI has no device, so the laptop stays the last client.
    drop(attachment);
    c.ok(
        "pane.send_bytes",
        json!({ "pane": "w1:p1", "input_id": 9102, "bytes": "Cg==" }),
    )
    .await;
    wait_for(&pane, "the second write", |t| {
        t.iter().any(|row| row == "two")
    })
    .await;
    let deadline = Instant::now() + TIMEOUT;
    let dropped = loop {
        let shown = c.ok("pane.show", json!({ "pane": "w1:p1" })).await;
        if !shown["pane"]["clipboard_dropped"].is_null() {
            break shown["pane"]["clipboard_dropped"].clone();
        }
        assert!(Instant::now() < deadline, "the drop is not shown: {shown}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(dropped["bytes"], 11);
    assert_eq!(writes(&laptop), ["first text"]);
    assert!(writes(&local).is_empty());
}
