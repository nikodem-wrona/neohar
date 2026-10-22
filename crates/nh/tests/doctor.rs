//! `nh doctor` and `nh debug bundle` against a real core.

mod core_support;

use std::process::{Command, Stdio};

use core_support::Harness;
use serde_json::{Value, json};

const NH: &str = env!("CARGO_BIN_EXE_nh");

struct Ran {
    code: i32,
    out: String,
    err: String,
}

fn nh(args: &[&str], env: &[(&str, &str)]) -> Ran {
    let mut command = Command::new(NH);
    command
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in env {
        command.env(name, value);
    }
    let output = command.output().expect("nh runs");
    Ran {
        code: output.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&output.stdout).into_owned(),
        err: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn socket(h: &Harness) -> String {
    h.api_socket().display().to_string()
}

fn status<'a>(report: &'a Value, id: &str) -> &'a str {
    report["checks"]
        .as_array()
        .and_then(|checks| checks.iter().find(|c| c["id"] == id))
        .and_then(|c| c["status"].as_str())
        .unwrap_or_else(|| panic!("no check {id} in {report}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn doctor_asks_the_core_and_finds_a_sound_install() {
    let h = Harness::start();
    h.call("workspace.create", json!({"name": "Work"})).await;
    let ran = nh(&["--socket", &socket(&h), "--json", "doctor"], &[]);
    assert_eq!(ran.code, 0, "{} {}", ran.out, ran.err);
    let report: Value = serde_json::from_str(&ran.out).expect("JSON");
    assert_eq!(status(&report, "runtime_dir"), "ok");
    assert_eq!(status(&report, "api_socket"), "ok");
    assert_eq!(status(&report, "storage"), "ok");
    assert_eq!(status(&report, "holder_sockets"), "ok");
    // Without a terminal the output is JSON also without the flag.
    let piped = nh(&["--socket", &socket(&h), "doctor"], &[]);
    assert_eq!(piped.code, 0, "{}", piped.err);
    assert!(
        serde_json::from_str::<Value>(&piped.out).is_ok(),
        "{}",
        piped.out
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn doctor_checks_the_install_when_no_core_runs() {
    let dir = std::env::temp_dir().join(format!("nh-doctor-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    let root = dir.display().to_string();
    let ran = nh(
        &["--json", "doctor"],
        &[
            ("NEOHAR_CONFIG_DIR", &format!("{root}/config")),
            ("NEOHAR_DATA_DIR", &format!("{root}/data")),
            ("NEOHAR_RUNTIME_DIR", &format!("{root}/run")),
            ("HOME", &root),
        ],
    );
    // No socket exists, so a check fails and the exit code says so.
    assert_eq!(ran.code, 1, "{} {}", ran.out, ran.err);
    let report: Value = serde_json::from_str(&ran.out).expect("JSON");
    assert_eq!(status(&report, "api_socket"), "fail");
    assert!(
        report["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .all(|c| c["id"] != "holder_sockets"),
        "no core, no holders"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The names and the bytes of the files in a zip, read by the Python of the system.
fn unzip(path: &str) -> Vec<(String, Vec<u8>)> {
    let script = "import sys, zipfile, json, base64\n\
        z = zipfile.ZipFile(sys.argv[1])\n\
        assert z.testzip() is None\n\
        print(json.dumps([[n, base64.b64encode(z.read(n)).decode()] for n in z.namelist()]))";
    let output = Command::new("/usr/bin/python3")
        .args(["-c", script, path])
        .output()
        .expect("python runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let list: Vec<(String, String)> = serde_json::from_slice(&output.stdout).expect("JSON");
    list.into_iter()
        .map(|(name, data)| {
            use base64::Engine as _;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .expect("base64");
            (name, bytes)
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bundle_has_the_report_and_not_the_secrets_that_were_logged() {
    let h = Harness::start();
    let secret = format!("{}{}", "ghp_", "a1".repeat(20));
    // The name of a workspace goes into an event of the log, with a secret in it.
    let made = h.call("workspace.create", json!({"name": "Work"})).await;
    let workspace = made["workspace"].as_str().expect("a workspace").to_owned();
    h.call(
        "workspace.rename",
        json!({"workspace": workspace, "name": format!("deploy with {secret}")}),
    )
    .await;
    let dir = std::env::temp_dir().join(format!("nh-bundle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    let out = dir.join("bundle.zip").display().to_string();
    let ran = nh(
        &[
            "--socket",
            &socket(&h),
            "--json",
            "debug",
            "bundle",
            "--out",
            &out,
        ],
        &[],
    );
    assert_eq!(ran.code, 0, "{} {}", ran.out, ran.err);
    let result: Value = serde_json::from_str(&ran.out).expect("JSON");
    assert_eq!(result["path"], out.as_str());
    let files = unzip(&out);
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    for wanted in [
        "README.txt",
        "doctor.json",
        "doctor.txt",
        "versions.json",
        "db-stats.json",
        "events.jsonl",
    ] {
        assert!(names.contains(&wanted), "{wanted} is not in {names:?}");
    }
    // The renamed workspace is in the events, and its secret is not anywhere.
    let events = files
        .iter()
        .find(|(n, _)| n == "events.jsonl")
        .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
        .expect("events");
    assert!(events.contains("deploy with"), "{events}");
    for (name, bytes) in &files {
        let text = String::from_utf8_lossy(bytes);
        assert!(!text.contains(&secret), "{name} has the secret");
    }
    // The statistics count rows, and carry no content.
    let stats = files
        .iter()
        .find(|(n, _)| n == "db-stats.json")
        .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
        .expect("stats");
    assert!(stats.contains("\"events\""), "{stats}");
    // The scrollback is not in a bundle that did not ask for it.
    assert!(
        names.iter().all(|n| !n.starts_with("scrollback/")),
        "{names:?}"
    );
    // A second bundle to the same file is refused, not overwritten.
    let again = nh(
        &[
            "--socket",
            &socket(&h),
            "--json",
            "debug",
            "bundle",
            "--out",
            &out,
        ],
        &[],
    );
    assert_ne!(again.code, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bundle_with_the_flag_has_the_text_of_the_panes() {
    let h = Harness::start();
    h.call(
        "workspace.create",
        json!({"name": "Work", "command": {"argv": ["/bin/sh", "-c", "echo marker-of-the-pane; exec sleep 60"]}}),
    )
    .await;
    // The pane shows its text a moment after it starts.
    let dir = std::env::temp_dir().join(format!("nh-bundle-sb-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    let out = dir.join("bundle.zip").display().to_string();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let text = loop {
        let _ = std::fs::remove_file(&out);
        let ran = nh(
            &[
                "--socket",
                &socket(&h),
                "--json",
                "debug",
                "bundle",
                "--out",
                &out,
                "--include-scrollback",
            ],
            &[],
        );
        assert_eq!(ran.code, 0, "{}", ran.err);
        let found = unzip(&out).into_iter().find(|(n, b)| {
            n.starts_with("scrollback/")
                && String::from_utf8_lossy(b).contains("marker-of-the-pane")
        });
        if let Some((_, bytes)) = found {
            break String::from_utf8_lossy(&bytes).into_owned();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the pane never showed its text"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    assert!(text.contains("marker-of-the-pane"));
    let _ = std::fs::remove_dir_all(&dir);
}
