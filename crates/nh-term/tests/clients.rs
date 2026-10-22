//! The frame builder and its clients: joining late, deltas, epochs and overflow.

use nh_proto::{
    frame::{Frame, FrameKind},
    style::Style,
};
use nh_term::{
    AlacrittyEngine, ClientState, FrameBuilder, MIN_HISTORY, Replaying, VtEngine,
    client_grid::{ApplyError, ClientGrid, dump_engine},
    xterm_palette,
};

struct Pane {
    engine: AlacrittyEngine,
    builder: FrameBuilder,
}

impl Pane {
    fn new(cols: u16, rows: u16) -> Self {
        let engine = AlacrittyEngine::new(cols, rows, MIN_HISTORY, xterm_palette());
        let builder = FrameBuilder::new(1, &engine);
        Self { engine, builder }
    }

    fn write(&mut self, bytes: &[u8]) {
        self.engine.advance(bytes, &mut Replaying::default());
        self.builder.absorb(&mut self.engine);
    }

    fn frame_for(&mut self, state: &mut ClientState) -> Frame {
        self.builder.build(&self.engine, state, FrameKind::Delta)
    }

    fn sync(&mut self, state: &mut ClientState, client: &mut ClientGrid) -> Frame {
        let frame = self.frame_for(state);
        client.apply(&frame).expect("the frame applies");
        frame
    }

    fn assert_shows(&self, client: &ClientGrid) {
        let shown = client.dump().expect("the client screen resolves");
        if let Some(difference) = shown.diff(&dump_engine(&self.engine)) {
            panic!("the client differs from the engine: {difference}");
        }
    }
}

#[test]
fn two_clients_that_join_at_different_times_converge() {
    let mut pane = Pane::new(40, 10);
    let (mut a_state, mut a) = (ClientState::new(), ClientGrid::new());
    pane.write(b"\x1b[31mred\x1b[0m start\r\n");
    pane.sync(&mut a_state, &mut a);
    for step in 0..30 {
        pane.write(format!("\x1b[38;5;{}mrow {step}\x1b[0m\r\n", step * 7).as_bytes());
        pane.sync(&mut a_state, &mut a);
        pane.assert_shows(&a);
    }
    // B joins when A is far along, with a different rhythm: it skips frames.
    let (mut b_state, mut b) = (ClientState::new(), ClientGrid::new());
    let first = pane.sync(&mut b_state, &mut b);
    assert_eq!(first.kind, FrameKind::Keyframe);
    pane.assert_shows(&b);
    for step in 0..30 {
        pane.write(format!("\x1b[48;2;{step};9;9mpage {step}\x1b[0m\r\n").as_bytes());
        pane.sync(&mut a_state, &mut a);
        if step % 3 == 0 {
            // The builder gives B the rows that changed since its last frame, over several steps.
            pane.sync(&mut b_state, &mut b);
            pane.assert_shows(&b);
        }
        pane.assert_shows(&a);
    }
}

#[test]
fn a_keyframe_for_a_new_client_carries_every_style_it_uses_and_one_for_an_old_client_none() {
    let mut pane = Pane::new(40, 5);
    let (mut old_state, mut old) = (ClientState::new(), ClientGrid::new());
    pane.write(b"\x1b[1;31mA\x1b[0;4mB\x1b[38;2;1;2;3mC");
    pane.sync(&mut old_state, &mut old);
    let used: Vec<Style> = {
        let mut styles = Vec::new();
        for y in 0..5 {
            pane.engine.visit_row(y, &mut |c| {
                if c.style != Style::default() && !styles.contains(&c.style) {
                    styles.push(c.style);
                }
            });
        }
        styles
    };
    assert_eq!(used.len(), 3);

    let mut fresh = ClientState::new();
    let for_new = pane.frame_for(&mut fresh);
    assert_eq!(for_new.kind, FrameKind::Keyframe);
    let sent: Vec<Style> = for_new.new_styles.iter().map(|(_, s)| *s).collect();
    assert_eq!(sent.len(), 3);
    assert!(used.iter().all(|s| sent.contains(s)));

    // The old client asks for a keyframe on purpose. It has the styles already.
    let for_old = pane
        .builder
        .build(&pane.engine, &mut old_state, FrameKind::Keyframe);
    assert_eq!(for_old.kind, FrameKind::Keyframe);
    assert!(for_old.new_styles.is_empty());
    old.apply(&for_old).expect("the keyframe applies");
    pane.assert_shows(&old);
}

#[test]
fn a_delta_carries_the_rows_that_changed_and_the_news_once() {
    let mut pane = Pane::new(40, 10);
    let (mut state, mut client) = (ClientState::new(), ClientGrid::new());
    pane.write(b"one\r\ntwo\r\nthree");
    pane.sync(&mut state, &mut client);

    pane.write(b"\x1b[2;1HTWO");
    let delta = pane.frame_for(&mut state);
    client.apply(&delta).expect("the delta applies");
    assert_eq!(delta.kind, FrameKind::Delta);
    let rows: Vec<u16> = delta.lines.iter().map(|l| l.y).collect();
    assert!(rows.contains(&1) && rows.len() <= 3, "{rows:?}");
    assert!(
        delta.new_styles.is_empty()
            && delta.title.is_none()
            && !delta.bell
            && delta.palette.is_none()
    );

    pane.write(b"\x1b]2;a title\x07\x07\x1b]4;3;rgb:01/02/03\x07");
    let news = pane.frame_for(&mut state);
    client.apply(&news).expect("the delta applies");
    assert_eq!(news.title.as_deref(), Some("a title"));
    assert!(news.bell);
    assert_eq!(
        news.palette.as_ref().map(|p| p.colors[3]),
        Some(0x0001_0203)
    );
    pane.assert_shows(&client);

    // The news come once.
    pane.write(b"x");
    let after = pane.frame_for(&mut state);
    client.apply(&after).expect("the delta applies");
    assert!(after.title.is_none() && !after.bell && after.palette.is_none());

    // A cleared title is an empty string in a delta.
    pane.write(b"\x1b]2;\x07");
    let cleared = pane.frame_for(&mut state);
    assert_eq!(cleared.title.as_deref(), Some(""));
    client.apply(&cleared).expect("the delta applies");
    assert_eq!(client.dump().expect("resolves").title, None);
}

#[test]
fn a_change_of_size_starts_a_new_epoch_with_a_keyframe() {
    let mut pane = Pane::new(40, 10);
    let (mut state, mut client) = (ClientState::new(), ClientGrid::new());
    pane.write(b"hello\r\nworld");
    pane.sync(&mut state, &mut client);
    let epoch = pane.builder.epoch();
    pane.engine.resize(60, 20);
    pane.builder.absorb(&mut pane.engine);
    assert!(pane.builder.needs_keyframe(&state));
    let frame = pane.sync(&mut state, &mut client);
    assert_eq!(
        (frame.kind, frame.cols, frame.rows),
        (FrameKind::Keyframe, 60, 20)
    );
    assert_eq!(pane.builder.epoch(), epoch + 1);
    pane.assert_shows(&client);
}

#[test]
fn a_delta_for_another_revision_is_rejected_by_the_client() {
    let mut pane = Pane::new(40, 10);
    let (mut state, mut client) = (ClientState::new(), ClientGrid::new());
    pane.write(b"a");
    pane.sync(&mut state, &mut client);
    pane.write(b"b");
    let missed = pane.frame_for(&mut state);
    pane.write(b"c");
    let next = pane.frame_for(&mut state);
    // The client never saw `missed`, so `next` has the wrong base.
    assert!(matches!(client.apply(&next), Err(ApplyError::Stale { .. })));
    // A keyframe fixes it.
    let mut again = ClientState::new();
    let keyframe = pane.frame_for(&mut again);
    client.apply(&keyframe).expect("the keyframe applies");
    pane.assert_shows(&client);
    drop(missed);
}

#[test]
fn more_than_65535_styles_bump_the_epoch_and_the_client_still_converges() {
    let mut pane = Pane::new(80, 24);
    let (mut state, mut client) = (ClientState::new(), ClientGrid::new());
    pane.write(b"start");
    pane.sync(&mut state, &mut client);
    let first_epoch = pane.builder.epoch();
    let mut keyframes = 0;
    for n in 0..70_000u32 {
        let (r, g, b) = (n % 256, n / 256 % 256, n / 65_536);
        pane.write(format!("\x1b[1;1H\x1b[38;2;{r};{g};{b}mX").as_bytes());
        let frame = pane.frame_for(&mut state);
        keyframes += usize::from(frame.kind == FrameKind::Keyframe);
        client.apply(&frame).expect("the frame applies");
        if n % 5_000 == 0 || frame.kind == FrameKind::Keyframe {
            pane.assert_shows(&client);
        }
    }
    pane.assert_shows(&client);
    // Only the first frame and the frame at the overflow were keyframes.
    assert_eq!((keyframes, pane.builder.epoch()), (1, first_epoch + 1));
    assert_eq!(client.epoch(), pane.builder.epoch());
}
