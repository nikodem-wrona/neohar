//! The decoder allocates nothing for frames of unchanged size after warm-up.

#![allow(unsafe_code)]

mod common;

use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
};

use nh_proto::frame::FrameKind;
use proto_wasm::{Decoder, STATUS_APPLIED};

use common::{Source, encode};

/// The system allocator, with a count of the allocations.
struct Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every call goes to the system allocator, which follows the contract of `GlobalAlloc`.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        // SAFETY: the caller's layout is valid.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the pointer came from `alloc` with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        // SAFETY: the pointer came from `alloc` with this layout.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn deltas_and_keyframes_of_unchanged_size_allocate_nothing_after_warm_up() {
    let mut source = Source::new(80, 24);
    let mut warm = Vec::new();
    let mut measured = Vec::new();
    source.write(
        "\x1b]2;a title\x07\x1b[1;31mred\x1b[0m \x1b[4mu\x1b[0m \x1b[38;2;1;2;3mrgb\x1b[0m 日 text"
            .as_bytes(),
    );
    warm.push(encode(&source.frame(FrameKind::Delta)));
    let frames = |source: &mut Source, range: std::ops::Range<usize>, into: &mut Vec<Vec<u8>>| {
        for n in range {
            // Cells change among glyphs and styles that the first frame already had.
            let line = format!(
                "\x1b[{};1H\x1b[1;31mred\x1b[0m \x1b[4mu\x1b[0m \x1b[38;2;1;2;3mrgb\x1b[0m 日 {}\x07",
                2 + n % 20,
                n % 7
            );
            source.write(line.as_bytes());
            into.push(encode(&source.frame(FrameKind::Delta)));
        }
    };
    frames(&mut source, 0..30, &mut warm);
    // A keyframe with no new output after the last delta, so that the deltas after it still follow.
    let first_keyframe = encode(&source.frame(FrameKind::Keyframe));
    frames(&mut source, 30..60, &mut measured);
    let second_keyframe = encode(&source.frame(FrameKind::Keyframe));

    let mut decoder = Decoder::new(4096);
    for frame in &warm {
        assert_eq!(decoder.decode(frame), STATUS_APPLIED);
    }
    assert_eq!(decoder.decode(&first_keyframe), STATUS_APPLIED);

    let before = ALLOCATIONS.load(Ordering::SeqCst);
    for frame in &measured {
        assert_eq!(decoder.decode(frame), STATUS_APPLIED);
    }
    assert_eq!(decoder.decode(&second_keyframe), STATUS_APPLIED);
    let allocations = ALLOCATIONS.load(Ordering::SeqCst) - before;
    assert_eq!(allocations, 0, "decode allocated {allocations} times");
}
