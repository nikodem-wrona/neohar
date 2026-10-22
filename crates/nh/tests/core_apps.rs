//! Real full-screen programs in panes of `nh-core`: `less`, `vim` and `htop`.
//!
//! These tests need the programs on the machine, so they run only when `NEOHAR_APPS=1`. They check
//! that `vim`, `htop` and `less` render correctly and accept typing. A program that is not
//! installed fails the test.

mod core_support;

use std::path::Path;

use core_support::{Harness, Messages, has_row, wait_for};
use nh_core::pane::{Liveness, Pane};

fn enabled() -> bool {
    if std::env::var_os("NEOHAR_APPS").is_some_and(|v| v == "1") {
        return true;
    }
    eprintln!("skipped: set NEOHAR_APPS=1 to run the tests that need less, vim and htop");
    false
}

/// The path of a program on this machine.
fn program(name: &str) -> String {
    ["/usr/bin", "/opt/homebrew/bin", "/usr/local/bin", "/bin"]
        .iter()
        .map(|dir| format!("{dir}/{name}"))
        .find(|path| Path::new(path).is_file())
        .unwrap_or_else(|| panic!("{name} is not installed"))
}

fn write_file(dir: &Path, lines: usize) -> String {
    let path = dir.join("lines.txt");
    let text: String = (1..=lines).map(|n| format!("line {n}\n")).collect();
    std::fs::write(&path, text).expect("the file is written");
    path.display().to_string()
}

fn show(name: &str, pane: &Pane) {
    eprintln!("--- {name} ---\n{}\n--- end ---", pane.text().join("\n"));
}

/// Waits until the child of the pane has exited. The `panes` projection says so.
async fn wait_exit(h: &Harness, pane: &Pane) {
    let mut panes = Messages::subscribe(&h.handle, "panes");
    let id = pane.id().to_string();
    // The subscription is open, so a change after this check reaches it.
    while !matches!(pane.info().liveness, Liveness::Exited(_)) {
        panes
            .wait_for("a change of a pane", |m| {
                m["type"] == "patch"
                    && m["ops"]
                        .as_array()
                        .is_some_and(|ops| ops.iter().any(|op| op["key"] == id))
            })
            .await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn less_pages_and_quits() {
    if !enabled() {
        return;
    }
    let h = Harness::start();
    let file = write_file(&h.dir, 200);
    let pane = h.spawn(&[&program("less"), &file], 80, 24).await;
    wait_for(&pane, "the first page", |t| {
        has_row(t, "line 1") && has_row(t, "line 23")
    })
    .await;
    show("less, first page", &pane);
    h.text(&pane, " ").await;
    wait_for(&pane, "the second page", |t| {
        has_row(t, "line 24") && !has_row(t, "line 1")
    })
    .await;
    show("less, second page", &pane);
    h.text(&pane, "q").await;
    wait_exit(&h, &pane).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn vim_edits_and_quits() {
    if !enabled() {
        return;
    }
    let h = Harness::start();
    let file = write_file(&h.dir, 100);
    let pane = h
        .spawn(
            &[&program("vim"), "-u", "NONE", "-i", "NONE", "-N", &file],
            100,
            30,
        )
        .await;
    wait_for(&pane, "the file in vim", |t| {
        has_row(t, "line 1") && has_row(t, "line 2")
    })
    .await;
    // Insert a word at the start, then leave insert mode.
    h.text(&pane, "iTYPED ").await;
    h.key(&pane, "Escape", "Escape").await;
    // Vim waits a moment after Escape to see whether an escape sequence follows. Then the mode
    // line goes away.
    wait_for(&pane, "the end of insert mode", |t| {
        has_row(t, "TYPED line 1") && !t.iter().any(|row| row.contains("-- INSERT --"))
    })
    .await;
    show("vim, after typing", &pane);
    h.text(&pane, ":q!").await;
    h.key(&pane, "Enter", "Enter").await;
    wait_exit(&h, &pane).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn htop_draws_and_quits() {
    if !enabled() {
        return;
    }
    let h = Harness::start();
    let pane = h.spawn(&[&program("htop")], 120, 40).await;
    wait_for(&pane, "the process table", |t| {
        t.iter().any(|row| row.contains("PID")) && t.iter().any(|row| row.contains("Mem"))
    })
    .await;
    show("htop", &pane);
    h.text(&pane, "q").await;
    wait_exit(&h, &pane).await;
}
