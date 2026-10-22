//! Turning the screen model into frames.
//!
//! A [`FrameBuilder`] belongs to one pane. After the engine reads output, [`FrameBuilder::absorb`]
//! records which rows changed, and at which revision. Each client of the pane (a window, a peek)
//! has a [`ClientState`] that says what it has seen. [`FrameBuilder::build`] sends the rows that
//! changed after the client's revision, and the styles and graphemes that the client does not
//! have. So any number of clients can follow one pane, each at its own pace.

use nh_proto::{
    cells::{GRAPHEME_FLAG, MAX_GRAPHEME_ID, STYLE_DEFAULT, pack_cell},
    frame::{Frame, FrameKind, Palette, RowUpdate, Selection},
    style::Style,
};

use crate::{
    engine::{Damage, VtEngine},
    intern::{IdSet, Interner},
};

/// The most styles of one epoch. Id 0 is the default style, and it is never sent.
const MAX_STYLES: u32 = u16::MAX as u32;

/// What one client of a pane has received.
#[derive(Clone, Debug, Default)]
pub struct ClientState {
    epoch: u32,
    rev: u64,
    /// The client has a keyframe of the current epoch.
    synced: bool,
    styles: IdSet,
    graphemes: IdSet,
}

impl ClientState {
    /// A client that has received nothing. Its first frame is a keyframe.
    pub fn new() -> Self {
        Self::default()
    }

    /// The revision of the last frame that was built for this client.
    pub fn rev(&self) -> u64 {
        self.rev
    }
}

/// What [`FrameBuilder::absorb`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Absorbed {
    /// The rows that the engine changed (the pacer uses `minor`).
    pub damage: Damage,
    /// Something that a client can see changed: a row, the size, the title, the palette or the
    /// bell. When it is false, the new revision holds nothing that an older one lacks.
    pub changed: bool,
}

/// The frame builder of one pane.
pub struct FrameBuilder {
    pane: u32,
    epoch: u32,
    rev: u64,
    size: (u16, u16),
    /// For each row, the revision at which it last changed.
    row_rev: Vec<u64>,
    styles: Interner<Style>,
    graphemes: Interner<String>,
    title: Option<String>,
    title_rev: u64,
    palette: Palette,
    palette_rev: u64,
    bells: u64,
    bell_rev: u64,
    /// The selection that the last frame showed. Every frame carries the selection, and a change
    /// of it starts a new revision.
    selection: Option<Selection>,
}

/// An interner ran out of ids while a frame was built.
struct Overflow;

impl FrameBuilder {
    /// A builder for pane `pane` (its render id), at the state of `engine`.
    pub fn new(pane: u32, engine: &dyn VtEngine) -> Self {
        let size = engine.size();
        Self {
            pane,
            epoch: 0,
            rev: 1,
            size,
            row_rev: vec![1; usize::from(size.1)],
            styles: Interner::new(MAX_STYLES),
            graphemes: Interner::new(MAX_GRAPHEME_ID),
            title: engine.title().map(str::to_owned),
            title_rev: 0,
            palette: engine.palette(),
            palette_rev: 0,
            bells: engine.bell_count(),
            bell_rev: 0,
            selection: engine.selection(),
        }
    }

    /// The current epoch. It changes when the size changes or an interner is full.
    pub fn epoch(&self) -> u32 {
        self.epoch
    }

    /// The current revision. It grows by one for each call of [`FrameBuilder::absorb`].
    pub fn rev(&self) -> u64 {
        self.rev
    }

    /// Whether the next frame for `client` must be a keyframe.
    pub fn needs_keyframe(&self, client: &ClientState) -> bool {
        !client.synced || client.epoch != self.epoch
    }

    /// Takes the changes of the engine since the last call and starts a new revision.
    ///
    /// The result tells the caller what changed, so that it can skip a revision that no client
    /// can see.
    pub fn absorb(&mut self, engine: &mut dyn VtEngine) -> Absorbed {
        let damage = engine.take_damage();
        self.rev += 1;
        let mut changed = !damage.is_empty();
        let size = engine.size();
        if size != self.size {
            self.size = size;
            self.row_rev = vec![self.rev; usize::from(size.1)];
            self.bump_epoch();
            changed = true;
        } else if damage.full {
            self.row_rev.fill(self.rev);
        } else {
            for &y in &damage.rows {
                if let Some(slot) = self.row_rev.get_mut(usize::from(y)) {
                    *slot = self.rev;
                }
            }
        }
        if engine.title() != self.title.as_deref() {
            self.title = engine.title().map(str::to_owned);
            self.title_rev = self.rev;
            changed = true;
        }
        let palette = engine.palette();
        if palette != self.palette {
            self.palette = palette;
            self.palette_rev = self.rev;
            changed = true;
        }
        let selection = engine.selection();
        if selection != self.selection {
            self.selection = selection;
            changed = true;
        }
        if engine.bell_count() != self.bells {
            self.bells = engine.bell_count();
            self.bell_rev = self.rev;
            changed = true;
        }
        Absorbed { damage, changed }
    }

    fn bump_epoch(&mut self) {
        self.epoch += 1;
        self.styles.clear();
        self.graphemes.clear();
    }

    /// Builds the next frame for `client`. It is a keyframe when `want` says so, when the client
    /// has no keyframe of this epoch, and when an interner overflows.
    pub fn build(
        &mut self,
        engine: &dyn VtEngine,
        client: &mut ClientState,
        want: FrameKind,
    ) -> Frame {
        let mut kind = want;
        loop {
            if self.needs_keyframe(client) {
                kind = FrameKind::Keyframe;
            }
            match self.try_build(engine, client, kind) {
                Ok(frame) => return frame,
                Err(Overflow) => {
                    // Every client starts again from a keyframe. The screen alone has fewer
                    // styles than the interner holds, so the second try succeeds.
                    self.bump_epoch();
                    kind = FrameKind::Keyframe;
                }
            }
        }
    }

    fn try_build(
        &mut self,
        engine: &dyn VtEngine,
        client: &mut ClientState,
        kind: FrameKind,
    ) -> Result<Frame, Overflow> {
        if client.epoch != self.epoch {
            *client = ClientState {
                epoch: self.epoch,
                ..ClientState::default()
            };
        }
        let keyframe = kind == FrameKind::Keyframe;
        let (cols, rows) = self.size;
        let mut lines = Vec::new();
        let mut new_styles = Vec::new();
        let mut new_graphemes = Vec::new();
        let mut overflow = false;
        for y in 0..rows {
            if !keyframe && self.row_rev[usize::from(y)] <= client.rev {
                continue;
            }
            let mut cells = Vec::with_capacity(usize::from(cols) * 6);
            let mut last: Option<(Style, u16)> = None;
            engine.visit_row(y, &mut |cell| {
                let style_id = match last {
                    Some((style, id)) if style == cell.style => id,
                    _ => {
                        let id = if cell.style == Style::default() {
                            STYLE_DEFAULT
                        } else if let Some(id) = self.styles.intern(&cell.style) {
                            if client.styles.insert(id) {
                                new_styles.push((id as u16, cell.style));
                            }
                            id as u16
                        } else {
                            overflow = true;
                            STYLE_DEFAULT
                        };
                        last = Some((cell.style, id));
                        id
                    }
                };
                let glyph = if cell.zerowidth.is_empty() {
                    cell.ch as u32
                } else {
                    let mut text = String::from(cell.ch);
                    text.extend(cell.zerowidth);
                    match self.graphemes.intern(&text) {
                        Some(id) => {
                            if client.graphemes.insert(id) {
                                new_graphemes.push((id, text));
                            }
                            GRAPHEME_FLAG | id
                        }
                        None => {
                            overflow = true;
                            cell.ch as u32
                        }
                    }
                };
                cells.extend_from_slice(&pack_cell(glyph, style_id));
            });
            if overflow {
                return Err(Overflow);
            }
            lines.push(RowUpdate { y, cells });
        }
        // An empty title in a delta means that the title was cleared.
        let title = if keyframe {
            self.title.clone()
        } else if self.title_rev > client.rev {
            Some(self.title.clone().unwrap_or_default())
        } else {
            None
        };
        let frame = Frame {
            pane: self.pane,
            reset_epoch: self.epoch,
            base_rev: if keyframe { 0 } else { client.rev },
            rev: self.rev,
            kind,
            cols,
            rows,
            lines,
            new_styles,
            new_graphemes,
            cursor: engine.cursor(),
            viewport: engine.viewport(),
            modes: engine.modes(),
            title,
            bell: !keyframe && self.bell_rev > client.rev,
            images: Vec::new(),
            selection: engine.selection(),
            ack_status: Vec::new(),
            palette: (keyframe || self.palette_rev > client.rev).then(|| self.palette.clone()),
        };
        client.rev = self.rev;
        client.synced = true;
        Ok(frame)
    }
}
