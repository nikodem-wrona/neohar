//! The budget of the process walk: less than 1 ms for each pane on macOS, and less
//! than 0.2 ms on Linux. Run it with `cargo run --release -p nh-agents --example walk_bench`.
//!
//! The tree is the one of a pane with an agent: a shell with five shells below it in a chain, and
//! a `sleep` at the bottom, so that the walk reads 7 processes, the most it reads at depth 6. The
//! run measures two cases. A **cold** walk reads every command line, as the first walk of a pane
//! does. A **warm** walk reads the system for the ids and the paths only, as every walk after the
//! first one does. The budget is the warm walk, because it is the one that runs every second.

use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use nh_agents::process::Walker;

const WARM_WALKS: usize = 2_000;
const COLD_WALKS: usize = 300;

/// The most a warm walk may take at the 95th percentile.
#[cfg(target_os = "macos")]
const BUDGET: Duration = Duration::from_millis(1);
#[cfg(not(target_os = "macos"))]
const BUDGET: Duration = Duration::from_micros(200);

struct Guard(Child);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn chain() -> String {
    let mut script = "sleep 60; true".to_owned();
    for _ in 0..5 {
        script = format!("/bin/sh -c '{}'; true", script.replace('\'', "'\\''"));
    }
    script
}

fn percentile(sorted: &[Duration], percent: usize) -> Duration {
    sorted[(sorted.len() * percent / 100).min(sorted.len() - 1)]
}

fn summary(label: &str, mut times: Vec<Duration>) -> Duration {
    times.sort_unstable();
    let p50 = percentile(&times, 50);
    let p95 = percentile(&times, 95);
    let max = times.last().copied().unwrap_or_default();
    println!("{label:>5}: p50 {p50:>9.1?}  p95 {p95:>9.1?}  max {max:>9.1?}");
    p95
}

fn main() {
    let child = Guard(
        Command::new("/bin/sh")
            .args(["-c", &chain()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("a shell"),
    );
    let leader = i32::try_from(child.0.id()).expect("a pid");
    let mut walker = Walker::new(false);
    let deadline = Instant::now() + Duration::from_secs(10);
    let processes = loop {
        let tree = walker.walk(leader);
        if tree.processes.len() >= 7 || Instant::now() > deadline {
            break tree.processes.len();
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    println!("processes in the tree: {processes}");

    let mut cold = Vec::with_capacity(COLD_WALKS);
    for _ in 0..COLD_WALKS {
        let mut fresh = Walker::new(false);
        let start = Instant::now();
        let _ = fresh.walk(leader);
        cold.push(start.elapsed());
    }
    let mut warm = Vec::with_capacity(WARM_WALKS);
    for _ in 0..WARM_WALKS {
        let start = Instant::now();
        let _ = walker.walk(leader);
        warm.push(start.elapsed());
    }
    summary("cold", cold);
    let p95 = summary("warm", warm);
    println!("budget (warm p95): {BUDGET:?}");
    if processes < 7 {
        eprintln!("the tree has {processes} processes, not 7: the measure is not valid");
        std::process::exit(2);
    }
    if p95 > BUDGET {
        eprintln!("over the budget");
        std::process::exit(1);
    }
}
