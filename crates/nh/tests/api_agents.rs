//! The detection of agents against a real core: a program with the name of a harness runs
//! in a pane, and the core finds it, makes a run that is only suggested, and ends it with the
//! program. The program is a copy of `sleep` that is called `claude`.

mod core_support;

use std::{path::PathBuf, time::Duration};

use core_support::{Harness, client::Client};
use serde_json::{Value, json};

async fn client(h: &Harness) -> Client {
    Client::connect(&h.api_socket()).await
}

/// A program with the name of a harness. macOS kills a copy of `sleep`, so it is a script that
/// starts `sleep` under the name of the harness with `exec -a`. `direct` makes the script replace
/// itself, so the pane runs the program as its child. Otherwise the script stays as the parent of
/// the program, with the arguments that it got, as a wrapper does. `seconds` is unique for each
/// test, so that `kill_programs` ends the program of this test only.
fn fake(h: &Harness, name: &str, direct: bool, seconds: u32) -> PathBuf {
    let dir = h.dir.join("bin");
    std::fs::create_dir_all(&dir).expect("a directory");
    let path = dir.join(name);
    let start = format!("exec -a {name} /bin/sleep {seconds}");
    let body = if direct {
        start
    } else {
        format!("( {start} )")
    };
    std::fs::write(&path, format!("#!/bin/bash\n{body}\n")).expect("a script");
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("a mode");
    path
}

/// One projection as a snapshot: `(order, items)`.
async fn snapshot(c: &mut Client, name: &str) -> (Vec<String>, Value) {
    let reply = c.ok("projection.subscribe", json!({"name": name})).await;
    let id = reply["subscription"].as_str().expect("an id").to_owned();
    loop {
        let line = c.recv().await;
        if line["method"] == "projection.message"
            && line["params"]["subscription"] == id.as_str()
            && line["params"]["message"]["type"] == "snapshot"
        {
            let data = &line["params"]["message"]["data"];
            let order = data["order"]
                .as_array()
                .expect("an order")
                .iter()
                .map(|k| k.as_str().expect("a key").to_owned())
                .collect();
            c.ok("projection.unsubscribe", json!({"subscription": id}))
                .await;
            return (order, data["items"].clone());
        }
    }
}

/// Reads `agents` until `done` likes it. The watchers walk once a second, so a run takes a moment.
async fn agents_until(c: &mut Client, done: impl Fn(&Value) -> bool) -> Value {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let (_, items) = snapshot(c, "agents").await;
        if done(&items) || std::time::Instant::now() > deadline {
            return items;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn kill_programs(name: &str, seconds: u32) {
    let _ = std::process::Command::new("pkill")
        .args(["-9", "-f", &format!("^{name} {seconds}$")])
        .status();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_program_named_claude_in_a_pane_makes_a_suggested_run_that_ends_with_the_program() {
    let h = Harness::start();
    let claude = fake(&h, "claude", true, 3601);
    let mut c = client(&h).await;
    // Another connection hears the events, because the polling below skips every line that is not
    // a snapshot.
    let mut watcher = client(&h).await;
    let events = watcher
        .ok(
            "events.subscribe",
            json!({"filter": {"kinds": ["agent.detected", "agent.state_changed"]}}),
        )
        .await["subscription"]
        .as_str()
        .expect("an id")
        .to_owned();
    let created = c
        .ok(
            "workspace.create",
            json!({"name": "A", "command": {"argv": [claude.display().to_string()]}}),
        )
        .await;
    let pane = created["pane"].as_str().expect("a pane").to_owned();

    let items = agents_until(&mut c, |items| {
        items.as_object().is_some_and(|runs| !runs.is_empty())
    })
    .await;
    let runs = items.as_object().expect("runs");
    assert_eq!(runs.len(), 1, "{items}");
    let (run_id, run) = runs.iter().next().expect("a run");
    assert_eq!(run["harness"], "claude");
    assert_eq!(run["pane"], pane.as_str());
    // Neohar did not launch it and no signal carried the token, so it is only suggested.
    assert_eq!(run["binding"]["state"], "suggested");
    assert_eq!(run["facets"]["liveness"]["value"]["state"], "alive");
    assert_eq!(
        run["facets"]["execution"]["provenance"]["source"],
        "process"
    );
    // And nothing can be answered.
    let caps = &run["capabilities"];
    assert_eq!(caps["observe"], true);
    assert_eq!(caps["gate"], false);
    assert_eq!(caps["answer_keystroke"], false);
    assert_eq!(caps["answer_native"], json!([]));
    assert_eq!(caps["steer"], false);

    // The pane shows its run, and the log has the event.
    let (_, panes) = snapshot(&mut c, "panes").await;
    assert_eq!(panes[&pane]["run"], run_id.as_str());
    let detected = loop {
        let line = watcher.recv().await;
        if line["method"] == "events.event"
            && line["params"]["subscription"] == events.as_str()
            && line["params"]["event"]["kind"] == "agent.detected"
        {
            break line["params"]["event"]["payload"].clone();
        }
    };
    assert_eq!(detected["run"], run_id.as_str());
    assert_eq!(detected["harness"], "claude");
    assert_eq!(detected["via"], "process");

    // The program ends. The holder reports it and the run ends with it.
    kill_programs("claude", 3601);
    let items = agents_until(&mut c, |items| {
        items[run_id]["facets"]["liveness"]["value"]["state"] == "exited"
    })
    .await;
    let run = &items[run_id];
    assert_eq!(run["facets"]["liveness"]["value"]["state"], "exited");
    assert_eq!(run["facets"]["execution"]["value"]["state"], "exited");
    let (_, panes) = snapshot(&mut c, "panes").await;
    assert!(
        panes[&pane].get("run").is_none() || panes[&pane]["run"].is_null(),
        "{}",
        panes[&pane]
    );
    // An ended run is no reason for attention.
    let (order, _) = snapshot(&mut c, "attention").await;
    assert!(order.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_program_that_a_shell_runs_is_found_and_its_end_is_seen_by_the_walk() {
    let h = Harness::start();
    let codex = fake(&h, "codex", true, 3602);
    let mut c = client(&h).await;
    // `sh` runs `codex` as a child, and then goes on with another program.
    let script = format!("{}; exec /bin/sleep 3699", codex.display());
    let created = c
        .ok(
            "workspace.create",
            json!({"name": "B", "command": {"argv": ["/bin/sh", "-c", script]}}),
        )
        .await;
    let pane = created["pane"].as_str().expect("a pane").to_owned();
    let items = agents_until(&mut c, |items| {
        items.as_object().is_some_and(|runs| !runs.is_empty())
    })
    .await;
    let (run_id, run) = items
        .as_object()
        .and_then(|runs| runs.iter().next())
        .expect("a run");
    assert_eq!(run["harness"], "codex");
    assert_eq!(run["pane"], pane.as_str());
    assert_eq!(run["binding"]["state"], "suggested");
    assert_eq!(run["yolo"], false);

    // The child ends, and the shell stays: the pane lives, the run does not.
    kill_programs("codex", 3602);
    let items = agents_until(&mut c, |items| {
        items[run_id]["facets"]["liveness"]["value"]["state"] == "exited"
    })
    .await;
    assert_eq!(
        items[run_id]["facets"]["execution"]["provenance"]["source"],
        "process"
    );
    let (_, panes) = snapshot(&mut c, "panes").await;
    assert_eq!(panes[&pane]["liveness"]["state"], "alive");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_flag_that_skips_the_dialogs_shows_as_yolo() {
    let h = Harness::start();
    // The script is the parent of the program and has the flag in its arguments, as a wrapper
    // that passes its arguments on does.
    let claude = fake(&h, "claude", false, 3603);
    let mut c = client(&h).await;
    c.ok(
        "workspace.create",
        json!({
            "name": "C",
            "command": {"argv": [claude.display().to_string(), "--dangerously-skip-permissions"]}
        }),
    )
    .await;
    let items = agents_until(&mut c, |items| {
        items.as_object().is_some_and(|runs| !runs.is_empty())
    })
    .await;
    let run = items
        .as_object()
        .and_then(|runs| runs.values().next())
        .expect("a run");
    assert_eq!(run["harness"], "claude");
    assert_eq!(run["yolo"], true);
    kill_programs("claude", 3603);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_the_pane_ends_the_run_and_takes_it_from_the_projection() {
    let h = Harness::start();
    let claude = fake(&h, "claude", true, 3604);
    let mut c = client(&h).await;
    c.ok(
        "workspace.create",
        json!({"name": "D", "command": {"argv": [claude.display().to_string()]}}),
    )
    .await;
    let items = agents_until(&mut c, |items| {
        items.as_object().is_some_and(|runs| !runs.is_empty())
    })
    .await;
    let run_id = items
        .as_object()
        .and_then(|runs| runs.keys().next())
        .expect("a run")
        .clone();
    // A pane with a program that is an agent asks first, and says that it is an agent.
    let refused = c
        .call("pane.close", json!({"pane": "w1:p1", "force": false}))
        .await;
    assert_eq!(
        refused["error"]["data"]["details"]["is_agent"], true,
        "{refused}"
    );
    c.ok("pane.close", json!({"pane": "w1:p1", "force": true}))
        .await;
    let items = agents_until(&mut c, |items| items.get(&run_id).is_none()).await;
    assert!(items.get(&run_id).is_none(), "{items}");
    kill_programs("claude", 3604);
}

// --- hooks ------------------------------------------------------------------------------------

/// Gives a pane a token that the test knows, as the pane runtime does with a random one.
fn token_for(h: &Harness, pane: &str, token: &str) {
    let ulid: ulid::Ulid = pane.parse().expect("a pane id");
    h.handle.pane_tokens().register(ulid, token);
}

/// A hook as the shim sends it.
async fn hook(c: &mut Client, token: &str, harness: &str, event: &str, payload: Value) {
    c.ok(
        "adapter.signal",
        json!({"token": token, "harness": harness, "event": event, "payload": payload}),
    )
    .await;
}

fn payload(session: &str, extra: Value) -> Value {
    let mut base = json!({
        "session_id": session,
        "transcript_path": format!("/t/{session}.jsonl"),
        "cwd": "/work",
        "model": "claude-opus-5-5",
    });
    if let (Some(base), Some(more)) = (base.as_object_mut(), extra.as_object()) {
        base.extend(more.clone());
    }
    base
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_hooks_of_a_pane_make_a_bound_run_that_follows_the_session() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    let mut watcher = client(&h).await;
    let events = watcher
        .ok(
            "events.subscribe",
            json!({"filter": {"kinds": ["agent.detected", "agent.identified"]}}),
        )
        .await["subscription"]
        .as_str()
        .expect("an id")
        .to_owned();
    let a = c.ok("workspace.create", json!({"name": "A"})).await;
    let b = c.ok("workspace.create", json!({"name": "B"})).await;
    let (pane_a, pane_b) = (
        a["pane"].as_str().expect("a pane").to_owned(),
        b["pane"].as_str().expect("a pane").to_owned(),
    );
    token_for(&h, &pane_a, "token-a");
    token_for(&h, &pane_b, "token-b");

    hook(
        &mut c,
        "token-a",
        "claude",
        "SessionStart",
        payload("s1", json!({"source": "startup"})),
    )
    .await;
    let items = agents_until(&mut c, |items| {
        items.as_object().is_some_and(|runs| {
            runs.values()
                .any(|run| run["facets"]["execution"]["value"]["state"] == "idle")
        })
    })
    .await;
    let (run_id, run) = items
        .as_object()
        .and_then(|runs| runs.iter().next())
        .expect("a run");
    assert_eq!(run["pane"], pane_a.as_str());
    assert_eq!(run["binding"]["state"], "bound");
    assert_eq!(run["session_id"], "s1");
    assert_eq!(run["model"], "claude-opus-5-5");
    assert_eq!(
        run["facets"]["execution"]["provenance"]["source"],
        "structured"
    );

    // The events say that a hook found the run, and name its session.
    let mut seen = Vec::new();
    while seen.len() < 2 {
        let line = watcher.recv().await;
        if line["method"] == "events.event" && line["params"]["subscription"] == events.as_str() {
            seen.push(line["params"]["event"].clone());
        }
    }
    assert_eq!(seen[0]["kind"], "agent.detected");
    assert_eq!(seen[0]["payload"]["via"], "hook");
    assert_eq!(seen[1]["kind"], "agent.identified");
    assert_eq!(seen[1]["payload"]["session_id"], "s1");

    // A prompt makes the run work, and `Stop` makes it idle.
    hook(
        &mut c,
        "token-a",
        "claude",
        "UserPromptSubmit",
        payload("s1", json!({})),
    )
    .await;
    agents_until(&mut c, |items| {
        items[run_id]["facets"]["execution"]["value"]["state"] == "working"
    })
    .await;
    hook(
        &mut c,
        "token-a",
        "claude",
        "Stop",
        payload("s1", json!({})),
    )
    .await;
    let items = agents_until(&mut c, |items| {
        items[run_id]["facets"]["execution"]["value"]["state"] == "idle"
    })
    .await;
    assert_eq!(
        items[run_id]["facets"]["execution"]["value"]["state"],
        "idle"
    );

    // The token of another pane reaches the run of that pane only.
    hook(
        &mut c,
        "token-b",
        "codex",
        "SessionStart",
        payload("c1", json!({})),
    )
    .await;
    let items = agents_until(&mut c, |items| {
        items.as_object().is_some_and(|runs| runs.len() == 2)
    })
    .await;
    let second = items
        .as_object()
        .and_then(|runs| runs.values().find(|run| run["harness"] == "codex"))
        .expect("a codex run");
    assert_eq!(second["pane"], pane_b.as_str());
    assert_eq!(
        items[run_id]["session_id"], "s1",
        "the first run is as it was"
    );
    assert_eq!(
        items[run_id]["facets"]["execution"]["value"]["state"],
        "idle"
    );

    // A token that no pane has makes nothing.
    let refused = c
        .call(
            "adapter.signal",
            json!({"token": "nobody", "harness": "claude", "event": "SessionStart",
                   "payload": payload("zz", json!({}))}),
        )
        .await;
    assert!(refused.get("error").is_some(), "{refused}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (_, items) = snapshot(&mut c, "agents").await;
    assert_eq!(items.as_object().map(serde_json::Map::len), Some(2));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hook_and_the_process_tree_make_one_run_for_one_program() {
    let h = Harness::start();
    let claude = fake(&h, "claude", true, 3605);
    let mut c = client(&h).await;
    let made = c
        .ok(
            "workspace.create",
            json!({"name": "E", "command": {"argv": [claude.display().to_string()]}}),
        )
        .await;
    let pane = made["pane"].as_str().expect("a pane").to_owned();
    token_for(&h, &pane, "token-e");
    hook(
        &mut c,
        "token-e",
        "claude",
        "SessionStart",
        payload("s5", json!({"source": "startup"})),
    )
    .await;
    // The walk finds the program, and the run that the hook made is bound all the same.
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    let items = agents_until(&mut c, |items| {
        items.as_object().is_some_and(|runs| {
            runs.values()
                .any(|run| run["binding"]["state"] == "bound" && run["session_id"] == "s5")
        })
    })
    .await;
    let runs = items.as_object().expect("runs");
    assert_eq!(runs.len(), 1, "one run for one program: {items}");
    let run = runs.values().next().expect("a run");
    assert_eq!(run["pane"], pane.as_str());
    // The version could not be found for the fake program, and the run is only observed. A bound
    // run on a recorded version would have more.
    assert_eq!(run["capabilities"]["observe"], true);
    kill_programs("claude", 3605);
}

// --- the shims in the PATH of a pane -------------------------------------------------------------

/// Starts a pane that writes its PATH to a file, and gives the PATH.
async fn path_of_a_pane(h: &Harness, c: &mut Client, name: &str) -> String {
    let out = h.dir.join(format!("path-{name}.txt"));
    let script = format!("echo \"$PATH\" > {}; exec /bin/sleep 3699", out.display());
    c.ok(
        "workspace.create",
        json!({"name": name, "command": {"argv": ["/bin/sh", "-c", script]}}),
    )
    .await;
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(text) = std::fs::read_to_string(&out)
            && text.ends_with('\n')
        {
            return text.trim().to_owned();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the pane wrote no PATH"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_gets_the_shims_first_in_its_path_while_the_setting_is_on() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let shims = h.dir.join("data").join("test").join("shims");
    // The core wrote the shim of Codex at its start.
    let script = std::fs::read_to_string(shims.join("codex")).expect("the shim");
    assert!(script.contains("daemon_auto_start"));
    let path = path_of_a_pane(&h, &mut c, "on").await;
    assert!(path.starts_with(&format!("{}:", shims.display())), "{path}");
    assert!(path.contains("/usr/bin"), "the old PATH follows: {path}");
    // With the setting off, a new pane has the PATH it started from.
    c.ok("config.set", json!({"key": "agents.shims", "value": false}))
        .await;
    let path = path_of_a_pane(&h, &mut c, "off").await;
    assert!(!path.contains("shims"), "{path}");
    pkill_sleep(3699);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_zsh_pane_starts_with_the_zshenv_of_neohar_which_gives_the_zdotdir_of_the_user_back() {
    if !std::path::Path::new("/bin/zsh").exists() {
        eprintln!("no /bin/zsh: skipped");
        return;
    }
    let h = Harness::start();
    let mut c = client(&h).await;
    let shims = h.dir.join("data").join("test").join("shims");
    // The ZDOTDIR of the user, with a `.zshenv` that the shell must still read.
    let user = h.dir.join("home").join("zdot");
    std::fs::create_dir_all(&user).expect("a directory");
    std::fs::write(user.join(".zshenv"), "export USER_ZSHENV=read\n").expect("a file");
    let out = h.dir.join("zsh.txt");
    let script = format!(
        "echo \"$ZDOTDIR|${{NEOHAR_ZDOTDIR-unset}}|$NEOHAR_SHIMS|$USER_ZSHENV\" > {}; exec /bin/sleep 3698",
        out.display()
    );
    c.ok(
        "workspace.create",
        json!({"name": "zsh", "command": {
            "argv": ["/bin/zsh", "-c", script],
            "env": {"ZDOTDIR": user.display().to_string()},
        }}),
    )
    .await;
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let line = loop {
        if let Ok(text) = std::fs::read_to_string(&out)
            && text.ends_with('\n')
        {
            break text.trim().to_owned();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the pane wrote nothing"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(
        line,
        format!("{}|unset|{}|read", user.display(), shims.display())
    );
    pkill_sleep(3698);
}

fn pkill_sleep(seconds: u32) {
    let _ = std::process::Command::new("pkill")
        .args(["-9", "-f", &format!("^/bin/sleep {seconds}$")])
        .status();
}
