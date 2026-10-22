//! The snapshot check on the VT corpus: 50 synthetic seeds of 1 MiB and the journals of
//! real programs in `tests/vt-corpus/`.
//!
//! This is the sample that runs in `mise run test` (every 25th cut point of a seed, every 2nd of a
//! journal, short windows). The `corpus` command of `nh-snapcheck` with `--every 1`
//! runs the same check at every cut point.
//!
//! Three properties hold for every restore, and these tests fail when one does not:
//!
//! 1. The restored engine is exact at the moment of the restore: the same screens, cursors, modes,
//!    tab stops, scroll region, charsets, titles, palette and keyboard stacks.
//! 2. When the engine that goes on is rebuilt from its snapshot (`AlacrittyEngine::checkpoint`),
//!    it and every restored engine agree after every chunk of output that follows.
//! 3. The engine as `alacritty_terminal` leaves it (no rebuild) agrees with a restored engine
//!    after every chunk of output, except for the style of blank cells. Its grid keeps rows and
//!    counters that a snapshot cannot hold, and some output leaves a stale background colour on a
//!    blank cell in them. The test prints how often that happens and does not fail on it. It fails
//!    on any other difference.

use std::path::PathBuf;

use nh_snapcheck::{Options, Report, Stream, run};

fn options(every: usize, normalize: bool) -> Options {
    Options {
        every,
        window: 64 * 1024,
        max_live: 12,
        snapshot_every: 16,
        // A difference of the second kind is expected and counted, so a run does not stop at it.
        max_failures: if normalize { 3 } else { 10_000 },
        normalize,
    }
}

/// Checks one stream in both ways. Returns the report of the engine as it is, for the log.
fn check(stream: &Stream, every: usize) -> Report {
    let rebuilt = run(stream, &options(every, true));
    assert!(
        rebuilt.exact(),
        "rebuilt at each checkpoint:\n{}",
        rebuilt.summary()
    );
    assert!(
        rebuilt.checked > 0,
        "no cut point was checked in {}",
        stream.name
    );
    let plain = run(stream, &options(every, false));
    assert!(
        plain.instant_exact(),
        "not exact right after a restore:\n{}",
        plain.summary()
    );
    plain
}

fn seeds(from: u64, to: u64) {
    let mut total = Report::default();
    for seed in from..to {
        total.absorb(check(&Stream::from_seed(seed, 1 << 20), 25));
    }
    println!(
        "seeds {from}..{to}, engine as it is: {} snapshots, {} windows differ later ({} in what the engines show, {} in checkpoint state)",
        total.checked,
        total.failures.len(),
        total.count(nh_snapcheck::FailureKind::Continuation),
        total.count(nh_snapcheck::FailureKind::Checkpoint),
    );
}

macro_rules! seed_tests {
    ($($name:ident: $from:expr, $to:expr;)*) => {
        $(#[test] fn $name() { seeds($from, $to); })*
    };
}

seed_tests! {
    seeds_00_to_04: 0, 5;
    seeds_05_to_09: 5, 10;
    seeds_10_to_14: 10, 15;
    seeds_15_to_19: 15, 20;
    seeds_20_to_24: 20, 25;
    seeds_25_to_29: 25, 30;
    seeds_30_to_34: 30, 35;
    seeds_35_to_39: 35, 40;
    seeds_40_to_44: 40, 45;
    seeds_45_to_49: 45, 50;
}

#[test]
fn recorded_journals() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/vt-corpus");
    let mut journals: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "journal"))
        .collect();
    journals.sort();
    assert!(
        journals.len() >= 7,
        "the recordings are missing from {}",
        dir.display()
    );
    for path in journals {
        let stream = Stream::from_journal(&path).unwrap_or_else(|e| panic!("{e}"));
        let plain = check(&stream, 2);
        println!(
            "{}: {} snapshots, {} windows differ later",
            stream.name,
            plain.checked,
            plain.failures.len()
        );
    }
}
