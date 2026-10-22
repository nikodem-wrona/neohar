//! Frame build time for a full 200×60 redraw. The budget is 4 ms on Apple Silicon, in a release
//! build. `mise run bench` runs it on a Mac with Apple Silicon.

use std::time::{Duration, Instant};

use nh_proto::frame::FrameKind;
use nh_term::{
    AlacrittyEngine, ClientState, FrameBuilder, MIN_HISTORY, Replaying, VtEngine, xterm_palette,
};

const COLS: u16 = 200;
const ROWS: u16 = 60;
const BUDGET: Duration = Duration::from_millis(4);

/// A screen like a busy TUI: every row has many runs of different colours and attributes.
fn redraw() -> Vec<u8> {
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

fn stats(label: &str, mut times: Vec<Duration>) -> Duration {
    times.sort();
    let median = times[times.len() / 2];
    let p99 = times[times.len() * 99 / 100];
    println!(
        "{label}: median {median:?}, p99 {p99:?}, max {:?} ({} runs)",
        times[times.len() - 1],
        times.len()
    );
    median
}

fn main() {
    let mut engine = AlacrittyEngine::new(COLS, ROWS, MIN_HISTORY, xterm_palette());
    let bytes = redraw();
    let started = Instant::now();
    engine.advance(&bytes, &mut Replaying::default());
    let parse = started.elapsed();
    println!(
        "advance: {} bytes in {parse:?} ({:.0} MB/s)",
        bytes.len(),
        bytes.len() as f64 / parse.as_secs_f64() / 1e6
    );

    let mut builder = FrameBuilder::new(1, &engine);
    builder.absorb(&mut engine);

    let mut known = ClientState::new();
    let first = builder.build(&engine, &mut known, FrameKind::Keyframe);
    println!(
        "keyframe: {} styles, {} bytes encoded",
        first.new_styles.len(),
        postcard::to_stdvec(&first)
            .expect("the frame serializes")
            .len()
    );

    let runs = 300;
    let mut warm = Vec::new();
    let mut cold = Vec::new();
    for _ in 0..runs {
        // A client that has every style: the usual keyframe after a scroll or a reveal.
        let t = Instant::now();
        let frame = builder.build(&engine, &mut known, FrameKind::Keyframe);
        warm.push(t.elapsed());
        std::hint::black_box(&frame);
        // A client that joins: the builder also sends all styles.
        let mut fresh = ClientState::new();
        let t = Instant::now();
        let frame = builder.build(&engine, &mut fresh, FrameKind::Keyframe);
        cold.push(t.elapsed());
        std::hint::black_box(&frame);
    }
    let warm = stats("keyframe, client has the styles", warm);
    let cold = stats("keyframe, new client", cold);

    // A full redraw as a delta: all rows damaged.
    let mut deltas = Vec::new();
    for _ in 0..runs {
        engine.advance(&bytes, &mut Replaying::default());
        builder.absorb(&mut engine);
        let t = Instant::now();
        let frame = builder.build(&engine, &mut known, FrameKind::Delta);
        deltas.push(t.elapsed());
        std::hint::black_box(&frame);
    }
    let delta = stats("delta, every row changed", deltas);

    if !cfg!(debug_assertions) {
        for (label, median) in [
            ("warm keyframe", warm),
            ("new-client keyframe", cold),
            ("full delta", delta),
        ] {
            assert!(
                median <= BUDGET,
                "{label} takes {median:?}, the budget is {BUDGET:?}"
            );
        }
        println!("within the budget of {BUDGET:?}");
    }
}
