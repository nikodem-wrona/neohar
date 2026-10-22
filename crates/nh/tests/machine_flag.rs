//! `nh --machine <label>` and handles with a machine prefix, with the
//! fake ssh. The "remote" machine is a daemon that a link starts, with directories of its own.
//! The CLI runs as a child process, as a person runs it: it has no core of its own.

mod core_support;
mod ssh_support;

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use core_support::Harness;
use serde_json::Value;
use ssh_support::{NH, Remote, connect, fake_ssh};

struct Ran {
    code: i32,
    out: String,
    err: String,
}

impl Ran {
    fn json(&self) -> Value {
        serde_json::from_str(&self.out)
            .unwrap_or_else(|e| panic!("stdout is JSON: {e}: {} / {}", self.out, self.err))
    }
}

/// The client: its own config and runtime directories, `[machines.studio]` for `remote`.
struct Client {
    dir: PathBuf,
    env: Vec<(String, String)>,
}

impl Client {
    fn new(remote: &Remote, session: &str) -> Self {
        let dir = remote.dir.join("client");
        std::fs::create_dir_all(dir.join("config")).expect("a config dir");
        std::fs::write(
            dir.join("config/config.toml"),
            format!(
                "[machines.studio]\nssh = \"studio\"\nsession = \"{session}\"\n\
                 connect = \"manual\"\nremote_nh = \"{NH}\"\n\
                 [machines.mac]\nssh = \"mac\"\nremote_nh = \"{NH}\"\n"
            ),
        )
        .expect("a config");
        let mut env = remote.env();
        let path = |sub: &str| dir.join(sub).display().to_string();
        env.push(("NEOHAR_CONFIG_DIR".into(), path("config")));
        env.push(("NEOHAR_DATA_DIR".into(), path("data")));
        env.push(("NEOHAR_RUNTIME_DIR".into(), path("run")));
        env.push(("NEOHAR_SSH".into(), fake_ssh().display().to_string()));
        env.push(("HOME".into(), path("home")));
        env.push(("PATH".into(), "/usr/bin:/bin".into()));
        Self { dir, env }
    }

    fn nh(&self, args: &[&str], extra: &[(&str, &str)]) -> Ran {
        let mut command = Command::new(NH);
        command
            .args(args)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (name, value) in self.env.iter() {
            command.env(name, value);
        }
        for (name, value) in extra {
            command.env(name, value);
        }
        let output = command.output().expect("nh runs");
        Ran {
            code: output.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&output.stdout).into_owned(),
            err: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

/// A remote machine with a daemon (a link starts it) and a client with the machine in its config.
async fn start() -> (Remote, Client, nh_remote::LinkClient) {
    let remote = Remote::new();
    let link = connect(&remote.spec()).await.expect("a link");
    let client = Client::new(&remote, &remote.session);
    (remote, client, link)
}

fn handles(list: &Value) -> Vec<String> {
    list["panes"]
        .as_array()
        .expect("a list of panes")
        .iter()
        .map(|row| row["handle"].as_str().expect("a handle").to_owned())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_flag_runs_the_command_on_the_machine_and_a_prefix_does_the_same() {
    let (remote, client, _link) = start().await;
    // The client has no core: every answer comes from the second one.
    let made = client.nh(
        &["--machine", "studio", "workspace", "create", "--json"],
        &[],
    );
    assert_eq!(made.code, 0, "{}", made.err);
    assert_eq!(made.json()["pane_handle"], "w1:p1");
    let panes = client.nh(&["--machine", "studio", "pane", "list", "--json"], &[]);
    assert_eq!(panes.code, 0, "{}", panes.err);
    let ids = handles(&panes.json());
    assert!(!ids.is_empty(), "{}", panes.out);
    let id = &ids[0];
    // The prefix sends the command, and the machine gets the handle without it.
    let shown = client.nh(&["pane", "show", &format!("studio/{id}"), "--json"], &[]);
    assert_eq!(shown.code, 0, "{}", shown.err);
    assert_eq!(shown.json()["pane"]["handle"], id.as_str());
    // The same command without the machine finds no core on the client.
    let local = client.nh(&["pane", "show", id, "--json"], &[]);
    assert_ne!(local.code, 0);
    // The stop of the daemon keeps its panes running, as a quit does. The guard of the machine
    // kills their holders.
    let deadline = std::time::Instant::now() + ssh_support::WAIT;
    let panes = loop {
        let panes = core_support::reap::panes_under(&remote.dir);
        if !core_support::reap::running(&panes).is_empty() {
            break panes;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the pane of the machine has no holder"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    };
    drop(remote);
    assert!(
        core_support::reap::running(&panes).is_empty(),
        "a holder of the machine still runs: {panes:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_exit_code_of_the_remote_command_passes_through() {
    let (_remote, client, _link) = start().await;
    let ran = client.nh(&["--machine", "studio", "pane", "show", "w99:p99"], &[]);
    assert_eq!(ran.code, 3, "{} / {}", ran.out, ran.err);
    assert!(
        ran.err.contains("not found") || ran.err.contains("w99"),
        "{}",
        ran.err
    );
    let ran = client.nh(&["pane", "show", "studio/w99:p99"], &[]);
    assert_eq!(ran.code, 3, "{}", ran.err);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_ssh_exits_1_with_its_message() {
    let (_remote, client, _link) = start().await;
    let ran = client.nh(
        &["--machine", "studio", "pane", "list"],
        &[("FAKE_SSH_FAIL", "studio: Permission denied (publickey).")],
    );
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(
        ran.err.contains("Permission denied (publickey)."),
        "{}",
        ran.err
    );
    assert_eq!(ran.out, "");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_machines_and_an_unknown_machine_are_usage_errors() {
    let remote = Remote::new();
    let client = Client::new(&remote, &remote.session);
    let ran = client.nh(&["pane", "move", "studio/w1:p1", "mac/w1:t1"], &[]);
    assert_eq!(ran.code, 2, "{}", ran.err);
    assert!(
        ran.err.contains("studio") && ran.err.contains("mac"),
        "{}",
        ran.err
    );
    let ran = client.nh(&["--machine", "studio", "pane", "show", "mac/w1:p1"], &[]);
    assert_eq!(ran.code, 2, "{}", ran.err);
    let ran = client.nh(&["--machine", "nowhere", "pane", "list"], &[]);
    assert_eq!(ran.code, 2, "{}", ran.err);
    assert!(ran.err.contains("nowhere"), "{}", ran.err);
}

/// The remote command of the one call that the fake ssh logged. The log has one line for each
/// argument, then `--`, then variables. The command is the argument after the first `--` line.
fn logged_command(log: &Path) -> String {
    let text = std::fs::read_to_string(log).expect("a log");
    let call = text
        .split("\n--\nSSH_ASKPASS=")
        .next()
        .expect("a call")
        .to_owned();
    assert_eq!(text.matches("\n--\nSSH_ASKPASS=").count(), 1, "{text}");
    let (_, command) = call.split_once("\n--\n").expect("the command");
    command.to_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn arguments_reach_the_machine_as_they_were_typed() {
    let (remote, client, _link) = start().await;
    let log = client.dir.join("ssh.log");
    let log_text = log.display().to_string();
    let name = "it's a $HOME \"name\" with `ticks` and * stars; & | ( ) { } ~ !";
    let made = client.nh(
        &["--machine", "studio", "workspace", "create", name, "--json"],
        &[("FAKE_SSH_LOG", &log_text)],
    );
    assert_eq!(made.code, 0, "{}", made.err);
    let handle = made.json()["handle"].as_str().expect("a handle").to_owned();
    let shown = client.nh(
        &[
            "--machine",
            "studio",
            "workspace",
            "show",
            &handle,
            "--json",
        ],
        &[],
    );
    assert_eq!(shown.code, 0, "{}", shown.err);
    assert_eq!(shown.json()["workspace"]["name"], name, "{}", shown.out);
    let command = logged_command(&log);
    assert!(
        command.starts_with(&format!(
            "{NH} --session {} workspace create 'it'\\''s a $HOME",
            remote.session
        )),
        "{command}"
    );
    // The command of the person has no `--machine` and no prefix.
    assert!(!command.contains("machine"), "{command}");
    // A newline reaches the core of the machine, which refuses it in a name. If the shell had
    // cut the argument, the error would be another one.
    let ran = client.nh(
        &[
            "--machine",
            "studio",
            "workspace",
            "rename",
            &handle,
            "two\nlines",
            "--json",
        ],
        &[],
    );
    assert_eq!(ran.code, 1, "{}", ran.err);
    assert!(ran.err.contains("control character"), "{}", ran.err);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_label_local_runs_the_command_here_without_the_prefix() {
    let core = Harness::start();
    let remote = Remote::new();
    let client = Client::new(&remote, "unused");
    let socket = core.api_socket().display().to_string();
    let made = client.nh(&["--socket", &socket, "workspace", "create", "--json"], &[]);
    assert_eq!(made.code, 0, "{}", made.err);
    let id = made.json()["handle"].as_str().expect("a handle").to_owned();
    let shown = client.nh(
        &[
            "--socket",
            &socket,
            "workspace",
            "show",
            &format!("local/{id}"),
            "--json",
        ],
        &[],
    );
    assert_eq!(shown.code, 0, "{}", shown.err);
    assert_eq!(shown.json()["workspace"]["handle"], id.as_str());
}
