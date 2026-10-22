//! A pane that an engine drives, and its frames as bytes.

#![allow(dead_code)]

use nh_proto::frame::{Frame, FrameKind};
use nh_term::{
    AlacrittyEngine, ClientState, FrameBuilder, MIN_HISTORY, Replaying, VtEngine, xterm_palette,
};

pub const PANE: u32 = 7;

pub struct Source {
    pub engine: AlacrittyEngine,
    pub builder: FrameBuilder,
    pub state: ClientState,
}

impl Source {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self::with_pane(PANE, cols, rows)
    }

    /// A source for another pane id, for tests with several panes.
    pub fn with_pane(pane: u32, cols: u16, rows: u16) -> Self {
        let engine = AlacrittyEngine::new(cols, rows, MIN_HISTORY, xterm_palette());
        let builder = FrameBuilder::new(pane, &engine);
        Self {
            engine,
            builder,
            state: ClientState::new(),
        }
    }

    pub fn write(&mut self, bytes: &[u8]) {
        self.engine.advance(bytes, &mut Replaying::default());
        self.builder.absorb(&mut self.engine);
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.engine.resize(cols, rows);
        self.builder.absorb(&mut self.engine);
    }

    /// The next frame for the one client of this source.
    pub fn frame(&mut self, want: FrameKind) -> Frame {
        self.builder.build(&self.engine, &mut self.state, want)
    }
}

pub fn encode(frame: &Frame) -> Vec<u8> {
    postcard::to_stdvec(frame).expect("the frame serializes")
}
