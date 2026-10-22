//! Snapshot and restore of a 200×60 screen. The budget for a restore is 5 ms on Apple Silicon, in
//! a release build. `mise run bench` runs it on a Mac with Apple Silicon.
//!
//! It measures three screens: a busy TUI (every row has many runs of colours and attributes), a
//! screen of plain text, and an empty screen. For each it reports the time to snapshot, the time to
//! restore (decode included), the time of `checkpoint` (which also copies the scrollback in
//! memory), and the size of the snapshot.

use std::time::{Duration, Instant};

use nh_term::{
    AlacrittyEngine, EngineConfig, MIN_HISTORY, Replaying, Snapshot, VtEngine, xterm_palette,
};

const COLS: u16 = 200;
const ROWS: u16 = 60;
const BUDGET: Duration = Duration::from_millis(5);

fn busy() -> Vec<u8> {
    let mut out = b"\x1b[H".to_vec();
    for y in 0..ROWS {
        for x in 0..COLS {
            if x % 4 == 0 {
                let n = (usize::from(x) * 7 + usize::from(y) * 13) % 256;
                let attr = [0, 1, 3, 4][(usize::from(x) / 4 + usize::from(y)) % 4];
                out.extend_from_slice(
                    format!("\x1b[0;{attr};38;5;{n};48;2;{};{};{}m", x % 256, y * 4, n).as_bytes(),
                );
            }
            out.push(
                b"abcdefghijklmnopqrstuvwxyz0123456789"[(usize::from(x) + usize::from(y)) % 36],
            );
        }
        if y + 1 < ROWS {
            out.extend_from_slice(b"\r\n");
        }
    }
    out
}

fn text() -> Vec<u8> {
    (0..ROWS * 2)
        .flat_map(|y| format!("{y:4} {}\r\n", "lorem ipsum dolor sit amet ".repeat(7)).into_bytes())
        .collect()
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

fn main() {
    let config = EngineConfig {
        history: MIN_HISTORY,
        theme: xterm_palette(),
        cell_px: (0, 0),
    };
    let mut worst = Duration::ZERO;
    for (label, bytes) in [
        ("busy TUI", busy()),
        ("plain text", text()),
        ("empty", Vec::new()),
    ] {
        let mut engine = AlacrittyEngine::new(COLS, ROWS, MIN_HISTORY, xterm_palette());
        engine.advance(&bytes, &mut Replaying::default());
        let (mut snap, mut restore, mut checkpoint) = (Vec::new(), Vec::new(), Vec::new());
        let mut size = 0;
        for _ in 0..200 {
            let t = Instant::now();
            let snapshot = engine.snapshot(0).expect("snapshot");
            snap.push(t.elapsed());
            let encoded = snapshot.encode();
            size = encoded.len();
            let t = Instant::now();
            let decoded = Snapshot::decode(&encoded).expect("decodes");
            let restored = AlacrittyEngine::restore(&decoded, &config).expect("restores");
            restore.push(t.elapsed());
            drop(restored);
            let t = Instant::now();
            engine.checkpoint(0).expect("checkpoint");
            checkpoint.push(t.elapsed());
        }
        let restore = median(restore);
        worst = worst.max(restore);
        println!(
            "{label}: {size} bytes; snapshot {:?}, restore {restore:?}, checkpoint {:?} (medians of 200)",
            median(snap),
            median(checkpoint)
        );
    }
    assert!(
        worst <= BUDGET,
        "restore of a {COLS}x{ROWS} snapshot took {worst:?}, the budget is {BUDGET:?}"
    );
}
