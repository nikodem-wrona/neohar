//! The render server of `nh-core` against real panes: keyframes, deltas, pacing, the limit of two
//! frames in flight, and the rules for hidden and unfocused windows. The panes are panes of the
//! layout, and the windows are windows of the layout.

mod core_support;

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use core_support::{Frames, Harness, has_row, wait_for};
use nh_core::{ErrorCode, FrameSink, pane::Pane};
use nh_proto::{
    frame::{Frame, FrameKind},
    version::FRAME_PROTO,
};
use nh_term::client_grid::ClientGrid;
use serde_json::{Value, json};

/// A program that changes its screen about a hundred times a second for a few seconds.
const COUNTER: &str =
    "i=0; while [ $i -lt 600 ]; do echo line$i; i=$((i+1)); sleep 0.01; done; sleep 60";

/// A spinner: two rows at most change, 20 times a second.
const SPINNER: &str =
    "echo spin; while :; do printf '\\r|'; sleep 0.05; printf '\\r/'; sleep 0.05; done";

/// The command of a pane that runs `script` in `sh`.
fn sh(script: &str) -> Value {
    json!({ "argv": ["/bin/sh", "-c", script] })
}

/// A window of the layout and the panes of the tab that it draws.
struct Desk {
    window: String,
    panes: Vec<Arc<Pane>>,
}

/// Makes a workspace whose tab has one pane for each command, side by side, and opens a window
/// that draws the tab. The window sizes each pane to `cols` by `rows`, so it owns their size.
async fn desk(h: &Harness, commands: &[Value], cols: u16, rows: u16) -> Desk {
    let (first, rest) = commands.split_first().expect("a command");
    let created = h
        .call("workspace.create", json!({ "command": first }))
        .await;
    let mut ids = vec![created["pane"].clone()];
    for command in rest {
        let split = h
            .call(
                "pane.split",
                json!({ "pane": created["pane"], "direction": "right", "command": command }),
            )
            .await;
        ids.push(split["pane"].clone());
    }
    let window = h.open_window().await;
    let mut panes = Vec::new();
    for id in &ids {
        let pane = h.started(id).await;
        h.call(
            "pane.resize",
            json!({ "pane": id, "cols": cols, "rows": rows, "window": window }),
        )
        .await;
        panes.push(pane);
    }
    let deadline = Instant::now() + core_support::TIMEOUT;
    for pane in &panes {
        while pane.size() != (cols, rows) {
            assert!(Instant::now() < deadline, "the pane is {:?}", pane.size());
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    Desk { window, panes }
}

/// Like [`desk`], with one pane.
async fn one_pane(h: &Harness, command: Value, cols: u16, rows: u16) -> (String, Arc<Pane>) {
    let mut desk = desk(h, &[command], cols, rows).await;
    (desk.window, desk.panes.remove(0))
}

fn rows(grid: &ClientGrid) -> Vec<String> {
    grid.dump()
        .expect("the client screen resolves")
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|(text, _)| text.as_str())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

async fn ack(h: &Harness, window: &str, frame: &Frame) {
    h.call(
        "render.ack",
        json!({ "window": window, "acks": [{ "pane": frame.pane, "rev": frame.rev }] }),
    )
    .await;
}

async fn focus(h: &Harness, window: &str, pane: &Pane) {
    h.call(
        "pane.focus",
        json!({ "window": window, "pane": pane.id().to_string() }),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_first_frame_is_a_keyframe_and_the_client_follows_the_pane() {
    let h = Harness::start();
    let (w, pane) = one_pane(
        &h,
        sh("printf 'one\\r\\ntwo'; sleep 1; printf '\\r\\nthree'; sleep 60"),
        40,
        8,
    )
    .await;
    let mut frames = Frames::open(&h.handle, &w, 120.0);
    let mut grid = ClientGrid::new();
    let first = frames
        .next(core_support::TIMEOUT)
        .await
        .expect("a first frame");
    assert_eq!(first.kind, FrameKind::Keyframe);
    assert_eq!(first.pane, pane.render_id());
    grid.apply(&first).expect("the keyframe applies");
    ack(&h, &w, &first).await;
    let deadline = Instant::now() + core_support::TIMEOUT;
    while !has_row(&rows(&grid), "three") {
        let left = deadline
            .checked_duration_since(Instant::now())
            .expect("the client sees three");
        let frame = frames.next(left).await.expect("another frame");
        grid.apply(&frame).expect("the frame applies");
        ack(&h, &w, &frame).await;
    }
    assert_eq!(rows(&grid), pane.text());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn at_most_two_frames_are_in_flight_and_each_follows_the_one_before() {
    let h = Harness::start();
    let (w, _pane) = one_pane(&h, sh(COUNTER), 40, 8).await;
    let mut frames = Frames::open(&h.handle, &w, 120.0);
    let one = frames.next(core_support::TIMEOUT).await.expect("frame 1");
    let two = frames.next(core_support::TIMEOUT).await.expect("frame 2");
    assert_eq!(one.kind, FrameKind::Keyframe);
    assert_eq!(two.kind, FrameKind::Delta);
    assert_eq!(
        two.base_rev, one.rev,
        "a frame follows the frame before it, not the last ack"
    );
    // The pane changes all the time, and no third frame comes while two are in flight.
    assert!(frames.next(Duration::from_millis(400)).await.is_none());
    // An acknowledgement of the first frame lets the third one out. It follows the second, which
    // is not acknowledged.
    ack(&h, &w, &one).await;
    let three = frames.next(core_support::TIMEOUT).await.expect("frame 3");
    assert_eq!(three.base_rev, two.rev);
    assert!(three.rev > two.rev);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_that_dropped_a_frame_gets_a_keyframe() {
    let h = Harness::start();
    let (w, _pane) = one_pane(&h, sh(COUNTER), 40, 8).await;
    let mut frames = Frames::open(&h.handle, &w, 120.0);
    let one = frames.next(core_support::TIMEOUT).await.expect("frame 1");
    let two = frames.next(core_support::TIMEOUT).await.expect("frame 2");
    // The client could not apply frame 2 and says so. It will not acknowledge either frame.
    h.call(
        "render.ack",
        json!({ "window": w, "acks": [], "need_keyframe": [two.pane] }),
    )
    .await;
    let next = frames
        .next(core_support::TIMEOUT)
        .await
        .expect("a frame after the request");
    assert_eq!(next.kind, FrameKind::Keyframe);
    assert!(next.rev > one.rev);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hidden_window_gets_no_frames_and_a_keyframe_when_shown() {
    let h = Harness::start();
    let (w, _pane) = one_pane(&h, sh(COUNTER), 40, 8).await;
    let mut frames = Frames::open(&h.handle, &w, 120.0);
    let first = frames.next(core_support::TIMEOUT).await.expect("frame 1");
    ack(&h, &w, &first).await;
    h.call(
        "window.focus_report",
        json!({ "window": w, "focused": false, "visible": false, "refresh_hz": 120.0 }),
    )
    .await;
    // Frames that were already built may arrive. Then it stays quiet.
    while frames.next(Duration::from_millis(300)).await.is_some() {}
    h.call(
        "window.focus_report",
        json!({ "window": w, "focused": true, "visible": true, "refresh_hz": 120.0 }),
    )
    .await;
    let shown = frames
        .next(core_support::TIMEOUT)
        .await
        .expect("a frame when shown");
    assert_eq!(shown.kind, FrameKind::Keyframe);
    let mut grid = ClientGrid::new();
    grid.apply(&shown).expect("the keyframe applies");
    assert_eq!(rows(&grid).len(), 8);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn minor_damage_without_focus_goes_out_at_most_once_a_second() {
    let h = Harness::start();
    let desk = desk(&h, &[sh(SPINNER), sh("sleep 60")], 40, 4).await;
    let (w, spinner, other) = (&desk.window, &desk.panes[0], &desk.panes[1]);
    let spinner_id = spinner.render_id();

    let count = |target: &Arc<Pane>| {
        let h = &h;
        let target = Arc::clone(target);
        async move {
            // Focus the pane that the test names. The window is focused.
            focus(h, w, &target).await;
            let mut frames = Frames::open(&h.handle, w, 120.0);
            let started = Instant::now();
            let mut seen = 0;
            while let Some(left) = Duration::from_millis(1500).checked_sub(started.elapsed()) {
                let Some(frame) = frames.next(left).await else {
                    break;
                };
                if frame.pane == spinner_id {
                    seen += 1;
                }
                ack(h, w, &frame).await;
            }
            seen
        }
    };
    let focused = count(spinner).await;
    let unfocused = count(other).await;
    // 20 changes a second for 1.5 s, against one a second: the keyframe and one or two more.
    assert!(
        focused > 15,
        "the focused spinner gets its frames: {focused}"
    );
    assert!(
        unfocused <= 4,
        "the unfocused spinner is limited: {unfocused}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_acknowledgement_does_not_start_a_period() {
    // A period of 200 ms (5 Hz). The program prints `a`, and `b` 300 ms later. The test
    // acknowledges the first frame after 150 ms. The tick that the acknowledgement causes sends
    // nothing, so it must not delay the frame of `b`. A pacer that counted that tick would hold the
    // frame of `b` until 200 ms after the tick, which is 100 ms after `b` arrived.
    let h = Harness::start();
    // The program waits for the flag, so that `b` comes 300 ms after `a` also when the setup of
    // the window is slow.
    let flag = h.dir.join("go");
    let script = format!(
        "while [ ! -e {} ]; do sleep 0.01; done; printf a; sleep 0.3; printf b; sleep 60",
        flag.display()
    );
    let (w, pane) = one_pane(&h, sh(&script), 40, 4).await;
    focus(&h, &w, &pane).await;
    std::fs::write(&flag, b"").expect("the flag");
    // The stream opens when `a` is on the screen, so that the first frame is the keyframe with `a`
    // and no other frame comes before the one of `b`.
    wait_for(&pane, "a on the screen", |t| has_row(t, "a")).await;
    let mut frames = Frames::open(&h.handle, &w, 5.0);
    let first = frames
        .next(Duration::from_secs(5))
        .await
        .expect("a first frame");
    tokio::time::sleep(Duration::from_millis(150)).await;
    ack(&h, &w, &first).await;
    wait_for(&pane, "b on the screen", |t| has_row(t, "ab")).await;
    let shown = Instant::now();
    let mut waited = None;
    while let Some(frame) = frames.next(Duration::from_secs(2)).await {
        ack(&h, &w, &frame).await;
        if frame.rev >= pane.rev() {
            waited = Some(shown.elapsed());
            break;
        }
    }
    let waited = waited.expect("a frame with b");
    assert!(
        waited < Duration::from_millis(60),
        "the frame of b came {waited:?} after b"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn minor_damage_without_focus_goes_out_in_one_tick_for_all_panes() {
    // Two spinners, the second one starts 125 ms after the first. If each pane had its own period,
    // their frames would be 125 ms apart. With one clock for the window they go out together, and
    // the page paints once and acknowledges once.
    let h = Harness::start();
    let desk = desk(
        &h,
        &[
            sh(SPINNER),
            sh(&format!("sleep 0.125; {SPINNER}")),
            sh("sleep 60"),
        ],
        40,
        4,
    )
    .await;
    let (w, focused) = (&desk.window, &desk.panes[2]);
    focus(&h, w, focused).await;
    let mut frames = Frames::open(&h.handle, w, 120.0);
    let started = Instant::now();
    let mut arrivals: Vec<(Instant, u32)> = Vec::new();
    while let Some(left) = Duration::from_millis(5000).checked_sub(started.elapsed()) {
        let Some(frame) = frames.next(left).await else {
            break;
        };
        if frame.pane != focused.render_id() {
            arrivals.push((Instant::now(), frame.pane));
        }
        ack(&h, w, &frame).await;
    }
    // Leave the first second and a half out: the keyframes and the first flush come in it.
    let late: Vec<&(Instant, u32)> = arrivals
        .iter()
        .filter(|(at, _)| at.duration_since(started) > Duration::from_millis(1500))
        .collect();
    let lone = late
        .iter()
        .filter(|(at, pane)| {
            !late.iter().any(|(other_at, other)| {
                other != pane
                    && (if other_at > at {
                        *other_at - *at
                    } else {
                        *at - *other_at
                    }) < Duration::from_millis(40)
            })
        })
        .count();
    assert!(late.len() >= 4, "both spinners send frames: {}", late.len());
    assert!(
        lone <= 1,
        "frames of one spinner without a frame of the other near it: {lone} of {}",
        late.len()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_windows_follow_one_pane_at_their_own_pace() {
    let h = Harness::start();
    // Both windows draw the tab, so both draw its pane.
    let (fast_window, _pane) = one_pane(&h, sh(COUNTER), 40, 8).await;
    let slow_window = h.open_window().await;
    let mut fast = Frames::open(&h.handle, &fast_window, 120.0);
    let mut slow = Frames::open(&h.handle, &slow_window, 60.0);
    // The slow window never acknowledges: two frames, then nothing.
    let s1 = slow
        .next(core_support::TIMEOUT)
        .await
        .expect("slow frame 1");
    let s2 = slow
        .next(core_support::TIMEOUT)
        .await
        .expect("slow frame 2");
    assert_eq!((s1.kind, s2.kind), (FrameKind::Keyframe, FrameKind::Delta));
    // The fast window keeps going.
    let mut grid = ClientGrid::new();
    let mut applied = 0;
    while applied < 10 {
        let frame = fast
            .next(core_support::TIMEOUT)
            .await
            .expect("a fast frame");
        grid.apply(&frame).expect("the frame applies");
        ack(&h, &fast_window, &frame).await;
        applied += 1;
    }
    assert!(slow.next(Duration::from_millis(300)).await.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_stream_for_a_window_starts_again_from_keyframes() {
    let h = Harness::start();
    let (w, _pane) = one_pane(&h, sh("echo a; sleep 60"), 40, 4).await;
    let mut previous = Frames::open(&h.handle, &w, 60.0);
    let f = previous.next(core_support::TIMEOUT).await.expect("a frame");
    ack(&h, &w, &f).await;
    // The pane is quiet, so nothing but the new stream itself can start the frame. Many rounds
    // find a pacer that waits for a wake-up which an older pacer took.
    for round in 0..40 {
        let mut next = Frames::open(&h.handle, &w, 60.0);
        let again = next
            .next(core_support::TIMEOUT)
            .await
            .unwrap_or_else(|| panic!("round {round}: no frame on the new stream"));
        assert_eq!(again.kind, FrameKind::Keyframe, "round {round}");
        ack(&h, &w, &again).await;
        // The old stream gets nothing more.
        assert!(
            previous.next(Duration::from_millis(20)).await.is_none(),
            "round {round}"
        );
        previous = next;
    }
}

/// A sink that says when its receiver is gone, and when the render server lets go of it.
struct Closing {
    closed: Arc<AtomicBool>,
    dropped: Arc<AtomicBool>,
}

impl FrameSink for Closing {
    fn send(&self, _: Vec<u8>) -> bool {
        !self.closed.load(Ordering::SeqCst)
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

impl Drop for Closing {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hidden_window_drops_a_sink_whose_receiver_is_gone() {
    let h = Harness::start();
    let (w, _pane) = one_pane(&h, sh("echo a; sleep 60"), 40, 4).await;
    let (closed, dropped) = (
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    h.handle
        .stream(
            "render",
            json!({ "window": w, "viewport_hz": 60.0, "proto": FRAME_PROTO }),
            Arc::new(Closing {
                closed: Arc::clone(&closed),
                dropped: Arc::clone(&dropped),
            }),
        )
        .expect("the stream opens");
    let hide = json!({ "window": w, "focused": false, "visible": false, "refresh_hz": 60.0 });
    h.call("window.focus_report", hide.clone()).await;
    // The window gets no frames, so no `send` says that the receiver is gone, for example the
    // sink of a link that is down. The next pass of the window drops it.
    closed.store(true, Ordering::SeqCst);
    h.call("window.focus_report", hide).await;
    let deadline = Instant::now() + core_support::TIMEOUT;
    while !dropped.load(Ordering::SeqCst) {
        assert!(
            Instant::now() < deadline,
            "the render server keeps the sink"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_of_another_frame_format_is_refused() {
    let h = Harness::start_layout();
    h.call("workspace.create", json!({})).await;
    let w = h.open_window().await;
    let error = h
        .handle
        .stream(
            "render",
            json!({ "window": w, "proto": FRAME_PROTO + 1 }),
            Arc::new(Discard),
        )
        .expect_err("the format is not the same");
    assert_eq!(error.code, ErrorCode::ProtoUnsupported);
    let unknown = h
        .handle
        .stream("nope", json!({}), Arc::new(Discard))
        .expect_err("no such stream");
    assert_eq!(unknown.code, ErrorCode::NotFound);
}

struct Discard;

impl FrameSink for Discard {
    fn send(&self, _: Vec<u8>) -> bool {
        true
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn typing_shows_up_in_the_frames() {
    let h = Harness::start();
    let (w, pane) = one_pane(&h, json!({ "argv": ["/bin/sh"] }), 60, 8).await;
    let mut frames = Frames::open(&h.handle, &w, 120.0);
    let mut grid = ClientGrid::new();
    wait_for(&pane, "a prompt", |t| {
        t.iter().any(|row| row.ends_with('$'))
    })
    .await;
    h.text(&pane, "echo seen-in-a-frame").await;
    h.key(&pane, "Enter", "Enter").await;
    let deadline = Instant::now() + core_support::TIMEOUT;
    let mut painted = false;
    while !painted {
        let left = deadline
            .checked_duration_since(Instant::now())
            .expect("the frames show the output");
        let frame = frames.next(left).await.expect("a frame");
        grid.apply(&frame).expect("the frame applies");
        ack(&h, &w, &frame).await;
        painted = has_row(&rows(&grid), "seen-in-a-frame");
    }
}
