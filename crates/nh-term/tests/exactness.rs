//! Bytes in, frames out, exactly: a client that applies the frames shows the screen of the
//! engine, after every chunk.

use nh_fixture::{Op, ops};
use nh_proto::frame::{Frame, FrameKind};
use nh_term::{
    AlacrittyEngine, ClientState, FrameBuilder, MIN_HISTORY, Replaying, VtEngine,
    client_grid::{ClientGrid, dump_engine},
    xterm_palette,
};
use proptest::prelude::*;

fn engine(cols: u16, rows: u16) -> AlacrittyEngine {
    AlacrittyEngine::new(cols, rows, MIN_HISTORY, xterm_palette())
}

/// Sends the frame over the wire format and back, as the core and the webview do.
fn over_the_wire(frame: &Frame) -> Frame {
    let bytes = postcard::to_stdvec(frame).expect("the frame serializes");
    let borrowed: nh_proto::frame::FrameRef<'_> =
        postcard::from_bytes(&bytes).expect("the frame decodes");
    let frame_back = borrowed.to_frame();
    assert_eq!(&frame_back, frame, "the wire changed the frame");
    frame_back
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn the_client_screen_equals_the_engine_screen_after_every_chunk(stream in ops(40)) {
        let mut engine = engine(80, 24);
        let mut builder = FrameBuilder::new(7, &engine);
        let mut state = ClientState::new();
        let mut client = ClientGrid::new();
        for (n, op) in stream.iter().enumerate() {
            match op {
                Op::Bytes(bytes) => engine.advance(bytes, &mut Replaying::default()),
                Op::Resize { cols, rows } => engine.resize(*cols, *rows),
            }
            builder.absorb(&mut engine);
            let frame = over_the_wire(&builder.build(&engine, &mut state, FrameKind::Delta));
            prop_assert!(client.apply(&frame).is_ok(), "op {n}: the frame does not apply");
            let shown = client.dump().expect("the client screen resolves");
            let expected = dump_engine(&engine);
            if let Some(difference) = shown.diff(&expected) {
                return Err(TestCaseError::fail(format!("after op {n} {op:?}: {difference}")));
            }
        }
    }

    #[test]
    fn a_keyframe_alone_gives_the_same_screen(stream in ops(30)) {
        let mut engine = engine(80, 24);
        let mut builder = FrameBuilder::new(7, &engine);
        for op in &stream {
            match op {
                Op::Bytes(bytes) => engine.advance(bytes, &mut Replaying::default()),
                Op::Resize { cols, rows } => engine.resize(*cols, *rows),
            }
            builder.absorb(&mut engine);
        }
        // A client that joins at the end needs one keyframe, whatever came before.
        let mut state = ClientState::new();
        let mut client = ClientGrid::new();
        let frame = over_the_wire(&builder.build(&engine, &mut state, FrameKind::Delta));
        prop_assert_eq!(frame.kind, FrameKind::Keyframe);
        prop_assert!(client.apply(&frame).is_ok());
        if let Some(difference) = client.dump().expect("resolves").diff(&dump_engine(&engine)) {
            return Err(TestCaseError::fail(difference));
        }
    }
}
