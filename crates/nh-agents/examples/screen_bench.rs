//! The time of one evaluation of the built-in screen manifests on a screen of 200 columns and 60
//! rows (at most 0.3 ms). The budget is for a release build.
//!
//! `cargo run --release -p nh-agents --example screen_bench`

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use nh_agents::screen::{Detector, Screen, builtin};

/// A busy screen: the text of the agent above, a dialog below, wide rows.
fn busy_screen() -> Screen {
    let filler =
        "The quick brown fox jumps over the lazy dog and runs through src/lib.rs 12345 times ";
    let mut lines: Vec<String> = (0..44)
        .map(|n| {
            format!("{n:>3} {}", filler.repeat(3))
                .chars()
                .take(200)
                .collect()
        })
        .collect();
    lines.extend(
        [
            "────────────────────────────────────────────────────────────────────────",
            " Bash command",
            " git commit -m \"a long message that goes on for a while\" --amend --no-edit",
            "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌",
            " Do you want to proceed?",
            " ❯ 1. Yes",
            "   2. Yes, and don't ask again for commands that start with `git commit`",
            "   3. No",
            "",
            " Esc to cancel · Tab to amend",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
        ]
        .map(str::to_owned),
    );
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    Screen::from_text(&refs)
}

fn main() {
    let screen = busy_screen();
    for id in ["claude", "codex"] {
        let mut detector = Detector::new(Arc::new(builtin(id).expect("a manifest")));
        let rows = detector.rows_needed(screen.total_rows);
        let mut times: Vec<Duration> = Vec::new();
        let start = Instant::now();
        for _ in 0..3_000 {
            let begin = Instant::now();
            let found = detector.evaluate(&screen, begin);
            std::hint::black_box(&found);
            times.push(begin.elapsed());
        }
        times.sort();
        let mean = start.elapsed() / 3_000;
        let p99 = times[times.len() * 99 / 100];
        println!(
            "{id}: rows {rows:?}, mean {mean:?}, p50 {:?}, p99 {p99:?} (budget 300µs)",
            times[times.len() / 2]
        );
    }
}
