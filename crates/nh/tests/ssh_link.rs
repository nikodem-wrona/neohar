//! The link over ssh, `nh askpass` and the version rule.
//!
//! The fake ssh (`tests/support/fake-ssh.sh`) runs the remote command on this Mac with the config,
//! data and runtime directories of a "remote" machine that each test makes, so no test needs a
//! network. A real remote machine has its own test in `remote_bridge.rs`. Each test uses its own
//! session and stops the daemon that it started.

mod core_support;
mod ssh_support;

use std::{
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

use core_support::{Harness, TEST_PANE_TOKEN, client::Client};
use nh_core::{RecordingHost, SecretKind, host::HostCall};
use nh_remote::ssh::{Askpass, SshError, SshSpec};
use serde_json::json;
use ssh_support::{NH, Remote, connect, eventually, host, write_program};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_link_starts_a_call_works_and_the_end_of_the_client_ends_ssh() {
    let remote = Remote::new();
    let log = remote.dir.join("ssh.log");
    let mut spec = remote.spec();
    spec.env.push(("FAKE_SSH_LOG".into(), log.clone().into()));

    let client = connect(&spec).await.expect("the link opens");
    assert_eq!(client.welcome().core_version, env!("CARGO_PKG_VERSION"));
    let info = client
        .call(None, "session.info", json!({}))
        .await
        .expect("a call works");
    assert_eq!(info["name"], json!(remote.session));
    assert_eq!(remote.bridges(), 1, "one bridge runs while the link is up");

    // The call of ssh has exactly the options of a link.
    let logged = std::fs::read_to_string(&log).expect("the fake ssh wrote its arguments");
    let args: Vec<&str> = logged.lines().take_while(|l| *l != "--").collect();
    let control = format!("ControlPath={}/local/ssh/%C", remote.dir.display());
    for expected in [
        "-T",
        "ControlMaster=auto",
        control.as_str(),
        "ControlPersist=600",
        "ServerAliveInterval=15",
        "ServerAliveCountMax=3",
        "ObscureKeystrokeTiming=no",
        "studio",
    ] {
        assert!(args.contains(&expected), "{expected} is in {args:?}");
    }
    assert!(
        logged.contains(&format!(
            "bridge --session {} --proto link/1",
            remote.session
        )),
        "{logged}"
    );
    let mode = std::fs::metadata(remote.dir.join("local/ssh"))
        .expect("the control directory")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700);

    drop(client);
    eventually("the end of ssh with the client", || remote.bridges() == 0).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bridge_that_is_too_old_gives_bridge_too_old() {
    let remote = Remote::new();
    let old = remote.dir.join("old-nh");
    write_program(&old, "#!/bin/sh\necho 'NH-BRIDGE 0.0.0 link/0'\nexit 3\n");
    let mut spec = remote.spec();
    spec.remote_nh = old.display().to_string();
    match connect(&spec).await {
        Err(SshError::BridgeTooOld { version, protos }) => {
            assert_eq!((version.as_str(), protos.as_str()), ("0.0.0", "link/0"));
        }
        other => panic!("expected BridgeTooOld, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ssh_that_fails_gives_its_status_and_the_last_lines_of_its_error() {
    let remote = Remote::new();
    let mut spec = remote.spec();
    spec.env.push((
        "FAKE_SSH_FAIL".into(),
        "Host key verification failed.".into(),
    ));
    match connect(&spec).await {
        Err(error @ SshError::Ssh { .. }) => {
            let SshError::Ssh { status, stderr } = &error else {
                unreachable!()
            };
            assert_eq!(*status, Some(255));
            assert!(stderr.contains("Host key verification failed."), "{stderr}");
            assert!(!error.nh_is_missing());
        }
        other => panic!("expected Ssh, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_machine_without_nh_gives_status_127() {
    let remote = Remote::new();
    let mut spec = remote.spec();
    spec.remote_nh = "/nonexistent/bin/nh".to_owned();
    match connect(&spec).await {
        Err(error) => {
            assert!(error.nh_is_missing(), "{error:?}");
        }
        Ok(_) => panic!("a link without nh opened"),
    }
}

/// A local core whose host answers the questions of ssh with `answer`, and the spec that makes
/// the fake ssh ask `prompt` first.
fn asking(remote: &Remote, core: &Harness, prompt: &str, expect: &str) -> SshSpec {
    let mut spec = remote.spec();
    spec.askpass = Some(Askpass {
        nh: NH.into(),
        socket: core.api_socket(),
        session: "test".to_owned(),
        machine: "studio".to_owned(),
    });
    spec.env.push(("FAKE_SSH_ASK".into(), prompt.into()));
    spec.env.push(("FAKE_SSH_EXPECT".into(), expect.into()));
    spec
}

fn questions(host: &RecordingHost) -> Vec<(String, Option<String>, SecretKind)> {
    host.calls()
        .into_iter()
        .filter_map(|call| match call {
            HostCall::AskSecret(q) => Some((q.prompt, q.machine, q.kind)),
            _ => None,
        })
        .collect()
}

/// Every file under `dirs` that holds `needle`.
fn files_with(dirs: &[&Path], needle: &str) -> Vec<PathBuf> {
    fn walk(dir: &Path, needle: &[u8], found: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                walk(&path, needle, found);
            } else if kind.is_file()
                && std::fs::read(&path)
                    .is_ok_and(|bytes| bytes.windows(needle.len()).any(|w| w == needle))
            {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    for dir in dirs {
        walk(dir, needle.as_bytes(), &mut found);
    }
    found
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_question_of_ssh_goes_to_the_sheet_and_the_answer_is_kept_nowhere() {
    let secret = "pw-7f3a9c1e-not-in-any-file";
    let host = host();
    host.answer_secret(Some(secret.to_owned()));
    let core = Harness::start_hosted(Arc::clone(&host) as Arc<dyn nh_core::Host>);
    let remote = Remote::new();
    let spec = asking(&remote, &core, "me@studio's password: ", secret);

    let client = connect(&spec)
        .await
        .expect("ssh got the answer and the link opened");
    client
        .call(None, "session.info", json!({}))
        .await
        .expect("a call works");
    assert_eq!(
        questions(&host),
        [(
            "me@studio's password: ".to_owned(),
            Some("studio".to_owned()),
            SecretKind::Secret
        )]
    );
    drop(client);
    eventually("the end of ssh", || remote.bridges() == 0).await;
    // The store, the logs and the sockets of both machines have no copy of the answer.
    let found = files_with(&[core.dir.as_path(), remote.dir.as_path()], secret);
    assert!(found.is_empty(), "the answer is in {found:?}");
}

#[test]
fn the_search_for_a_secret_finds_one() {
    let remote = Remote::new();
    std::fs::write(
        remote.dir.join("data/state.db-wal"),
        b"..event pw-needle ..",
    )
    .expect("a file");
    let found = files_with(&[remote.dir.as_path()], "pw-needle");
    assert_eq!(found, [remote.dir.join("data/state.db-wal")]);
    assert!(files_with(&[remote.dir.as_path()], "pw-other").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_host_key_question_is_a_host_key_and_a_wrong_answer_fails() {
    let key = "The authenticity of host 'studio' can't be established.\n\
               Are you sure you want to continue connecting (yes/no/[fingerprint])? ";
    let host = host();
    host.answer_secret(Some("yes".to_owned()));
    let core = Harness::start_hosted(Arc::clone(&host) as Arc<dyn nh_core::Host>);
    let remote = Remote::new();
    connect(&asking(&remote, &core, key, "yes"))
        .await
        .expect("the host key was accepted");
    assert_eq!(questions(&host)[0].2, SecretKind::HostKey);

    // The person types a wrong password: the fake ssh refuses, as ssh does.
    let wrong = RecordingHost::new(PathBuf::from("/bin/nh"), true);
    wrong.answer_secret(Some("wrong".to_owned()));
    let wrong = Arc::new(wrong);
    let core = Harness::start_hosted(Arc::clone(&wrong) as Arc<dyn nh_core::Host>);
    let other = Remote::new();
    match connect(&asking(&other, &core, "Password: ", "right")).await {
        Err(SshError::Ssh { status, stderr }) => {
            assert_eq!(status, Some(255));
            assert!(stderr.contains("Permission denied"), "{stderr}");
            assert!(!stderr.contains("wrong"), "the answer is not in the error");
        }
        other => panic!("expected Ssh, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_person_who_cancels_ends_the_link_with_no_answer() {
    let host = host();
    host.answer_secret(None);
    let core = Harness::start_hosted(Arc::clone(&host) as Arc<dyn nh_core::Host>);
    let remote = Remote::new();
    match connect(&asking(&remote, &core, "Password: ", "x")).await {
        Err(SshError::Ssh { stderr, .. }) => {
            assert!(stderr.contains("cancelled"), "{stderr}");
        }
        other => panic!("expected Ssh, got {other:?}"),
    }
}

/// Runs `nh askpass <prompt>` against the socket of `core`.
fn nh_askpass(core: &Harness, prompt: &str) -> std::process::Output {
    Command::new(NH)
        .args(["askpass", prompt])
        .env("NEOHAR_SOCKET", core.api_socket())
        .stdin(Stdio::null())
        .output()
        .expect("nh runs")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nh_askpass_prints_the_answer_and_exits_1_without_one() {
    let host = host();
    host.answer_secret(Some("pw".to_owned()));
    let core = Harness::start_hosted(Arc::clone(&host) as Arc<dyn nh_core::Host>);
    let out = nh_askpass(&core, "Password: ");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "pw\n");

    let quiet = RecordingHost::new(PathBuf::from("/bin/nh"), true);
    quiet.answer_secret(None);
    let core = Harness::start_hosted(Arc::new(quiet));
    let out = nh_askpass(&core, "Password: ");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "no output on a cancel");

    // A host without a sheet: the core answers `unsupported`, and the command prints no answer.
    let core = Harness::start();
    let out = nh_askpass(&core, "Password: ");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("cannot ask a question of ssh"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // No core at all.
    let out = Command::new(NH)
        .args(["askpass", "Password: "])
        .env("NEOHAR_SOCKET", "/tmp/nhs-no-such/api.sock")
        .output()
        .expect("nh runs");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_cannot_call_askpass_ask() {
    let host = host();
    host.answer_secret(Some("pw".to_owned()));
    let core = Harness::start_hosted(Arc::clone(&host) as Arc<dyn nh_core::Host>);
    let mut c = Client::connect(&core.api_socket()).await;
    c.send(
        &json!({"jsonrpc": "2.0", "id": 1, "pane_token": TEST_PANE_TOKEN,
        "method": "askpass.ask", "params": {"prompt": "Password: "}}),
    )
    .await;
    let reply = c.recv().await;
    assert!(reply.get("error").is_some(), "a pane is refused: {reply}");
    assert!(questions(&host).is_empty(), "the host was not asked");
    // A client without a token is the CLI, and the CLI may ask.
    let reply = c
        .call(
            "askpass.ask",
            json!({"prompt": "Password: ", "machine": "studio"}),
        )
        .await;
    assert_eq!(reply["result"], json!({"answer": "pw"}), "{reply}");
    // A long question is refused.
    let reply = c
        .call("askpass.ask", json!({"prompt": "x".repeat(5000)}))
        .await;
    assert_eq!(
        reply["error"]["data"]["code"],
        json!("invalid_params"),
        "{reply}"
    );
}

#[test]
fn the_first_write_to_a_pipe_is_not_lost_when_the_bridge_is_gone() {
    // A bridge that exits at once: the client writes Hello to a closed pipe. The error is about
    // the old bridge or about ssh, never a hang. (Guards the order "tee, then Hello".)
    let remote = Remote::new();
    let old = remote.dir.join("old-nh");
    write_program(&old, "#!/bin/sh\nexit 3\n");
    let mut spec = remote.spec();
    spec.remote_nh = old.display().to_string();
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let started = Instant::now();
    let result = runtime.block_on(connect(&spec));
    assert!(
        matches!(
            result,
            Err(SshError::Ssh {
                status: Some(3),
                ..
            })
        ),
        "{result:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    let _ = std::io::stdout().flush();
}
