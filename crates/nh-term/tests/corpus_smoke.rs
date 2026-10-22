//! Smoke test: the synthetic corpus of `nh_fixture::vt` goes through the real engine and
//! nothing panics.
//!
//! The corpus pushes the kitty keyboard stack past 4,096 entries on purpose. `alacritty_terminal`
//! 0.26.0 then calls `title_stack.remove(0)` in `push_keyboard_mode` and panics when the title
//! stack is empty. The engine avoids that, and this test fails when it does not.

use nh_fixture::{Op, vt::corpus};
use nh_term::{AlacrittyEngine, MIN_HISTORY, Replaying, VtEngine, xterm_palette};

#[test]
fn the_corpus_does_not_panic_the_engine() {
    for seed in 0..20u64 {
        let mut engine = AlacrittyEngine::new(80, 24, MIN_HISTORY, xterm_palette());
        let mut sink = Replaying::default();
        for (n, op) in corpus(seed, 1 << 20).iter().enumerate() {
            match op {
                Op::Bytes(bytes) => {
                    engine.advance(bytes, &mut sink);
                    // A synchronized update that is never closed ends by a timeout in the core.
                    if n % 3 == 0 && engine.sync_deadline().is_some() {
                        engine.flush_sync(&mut sink);
                    }
                }
                Op::Resize { cols, rows } => engine.resize(*cols, *rows),
            }
        }
    }
}

/// The upstream bug, alone: 4,097 pushes with an empty title stack.
#[test]
fn a_keyboard_stack_deeper_than_the_limit_does_not_panic_the_engine() {
    let mut engine = AlacrittyEngine::new(80, 24, MIN_HISTORY, xterm_palette());
    let mut sink = Replaying::default();
    for _ in 0..4100 {
        engine.advance(b"\x1b[>1u", &mut sink);
    }
    // The next push removes the saved title, as `alacritty_terminal` does at that depth. The
    // snapshot holds the same stacks.
    engine.advance(b"\x1b]2;kept\x07\x1b[22;0t", &mut sink);
    engine.advance(b"\x1b[>1u", &mut sink);
    let snapshot = engine.snapshot(0).expect("snapshot");
    assert_eq!(snapshot.kitty.len(), 4101);
    assert!(snapshot.title_stack.is_empty());
    let restored = AlacrittyEngine::restore(
        &snapshot,
        &nh_term::EngineConfig {
            history: MIN_HISTORY,
            theme: xterm_palette(),
            cell_px: (0, 0),
        },
    )
    .expect("a stack that deep restores");
    assert_eq!(restored.modes().kitty_flags(), 1);
}
