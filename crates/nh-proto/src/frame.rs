//! Frames: the state the core sends to the webview.
//!
//! A frame is state sync, not a byte relay. It applies only to a client whose state is exactly
//! `base_rev` in the same `reset_epoch`. [`Frame`] owns its data. [`FrameRef`] has the same wire
//! format and borrows its byte and string fields from the input buffer.

use serde::{Deserialize, Serialize};

use crate::{modes::ModeFlags, style::Style};

/// Id of an interned style. [`crate::cells::STYLE_DEFAULT`] is never sent.
pub type StyleId = u16;

/// Id of an interned multi-codepoint grapheme cluster.
pub type GraphemeId = u32;

/// Id of an input that a client sent to a pane.
pub type InputId = u64;

/// Whether a frame replaces all rows or only the dirty rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum FrameKind {
    /// Every row. It applies to any client state in the same epoch.
    Keyframe,
    /// The dirty rows only. It applies to a client at exactly `base_rev`.
    Delta,
}

/// One row of cells. `cells` holds packed 6-byte records ([`crate::cells`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct RowUpdate {
    pub y: u16,
    #[serde(with = "serde_bytes")]
    pub cells: Vec<u8>,
}

/// The borrowed form of [`RowUpdate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowUpdateRef<'a> {
    pub y: u16,
    #[serde(with = "serde_bytes", borrow)]
    pub cells: &'a [u8],
}

/// The shape of the cursor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum CursorShape {
    #[default]
    Block,
    Underline,
    Bar,
    HollowBlock,
}

/// The cursor of a pane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct Cursor {
    pub x: u16,
    pub y: u16,
    pub shape: CursorShape,
    pub visible: bool,
    pub blink: bool,
}

/// The scrollback position. `total_lines` includes the on-disk archive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct Viewport {
    pub top_line: u64,
    pub total_lines: u64,
}

/// An image that the UI draws over the cells. `blob_hash` is a BLAKE3 hash into the blob store.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct Placement {
    pub id: u32,
    pub blob_hash: [u8; 32],
    /// Column of the top-left corner. It is negative when the image starts left of the grid.
    pub col: i32,
    /// Row of the top-left corner. It is negative when the image has scrolled above the viewport.
    pub row: i32,
    pub cols: u16,
    pub rows: u16,
    pub z: i16,
    pub offset_px: (i16, i16),
}

/// A point in the scrollback, counted in absolute lines (the unit of [`Viewport::top_line`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct SelectionPoint {
    pub col: u16,
    pub line: u64,
}

/// How a selection extends between its two points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum SelectionMode {
    /// Character by character.
    Simple,
    /// Whole words.
    Word,
    /// Whole lines.
    Line,
    /// A rectangle.
    Block,
}

/// The selection of a pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct Selection {
    pub start: SelectionPoint,
    pub end: SelectionPoint,
    pub mode: SelectionMode,
}

/// What happened to an input that a client sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum AckState {
    /// The holder wrote the input and acknowledged it.
    Acked,
    /// The input was not acknowledged. It is never replayed.
    Unconfirmed,
}

/// The effective colours of a pane. Each colour is `0x00RRGGBB`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct Palette {
    #[serde(with = "palette_colors")]
    #[cfg_attr(test, proptest(strategy = "crate::test_support::palette_colors()"))]
    pub colors: [u32; 256],
    pub fg: u32,
    pub bg: u32,
    pub cursor: u32,
}

/// Serde support for the 256 colours: a fixed-size tuple, so the wire has no length prefix.
mod palette_colors {
    use core::fmt;

    use serde::{
        Deserializer, Serializer,
        de::{Error, SeqAccess, Visitor},
        ser::SerializeTuple,
    };

    const LEN: usize = 256;

    pub fn serialize<S: Serializer>(colors: &[u32; LEN], serializer: S) -> Result<S::Ok, S::Error> {
        let mut tuple = serializer.serialize_tuple(LEN)?;
        for color in colors {
            tuple.serialize_element(color)?;
        }
        tuple.end()
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u32; LEN], D::Error> {
        struct Colors;

        impl<'de> Visitor<'de> for Colors {
            type Value = [u32; LEN];

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{LEN} colours")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut colors = [0; LEN];
                for (i, slot) in colors.iter_mut().enumerate() {
                    *slot = seq
                        .next_element()?
                        .ok_or_else(|| Error::invalid_length(i, &self))?;
                }
                Ok(colors)
            }
        }

        deserializer.deserialize_tuple(LEN, Colors)
    }
}

/// One frame of a pane.
///
/// The fields are in wire order. Append new fields at the end only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct Frame {
    /// The pane's render id.
    pub pane: u32,
    pub reset_epoch: u32,
    pub base_rev: u64,
    pub rev: u64,
    pub kind: FrameKind,
    /// Grid width.
    pub cols: u16,
    /// Grid height.
    pub rows: u16,
    /// The row updates. A keyframe has all rows. A delta has the dirty rows only.
    pub lines: Vec<RowUpdate>,
    pub new_styles: Vec<(StyleId, Style)>,
    pub new_graphemes: Vec<(GraphemeId, String)>,
    pub cursor: Cursor,
    pub viewport: Viewport,
    pub modes: ModeFlags,
    pub title: Option<String>,
    pub bell: bool,
    pub images: Vec<Placement>,
    pub selection: Option<Selection>,
    pub ack_status: Vec<(InputId, AckState)>,
    /// Sent in every keyframe and when the effective palette changed.
    pub palette: Option<Palette>,
}

/// The borrowed form of [`Frame`]. It has the same wire format.
///
/// Decode with `postcard::from_bytes::<FrameRef>` to avoid copying the cells and the strings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameRef<'a> {
    pub pane: u32,
    pub reset_epoch: u32,
    pub base_rev: u64,
    pub rev: u64,
    pub kind: FrameKind,
    pub cols: u16,
    pub rows: u16,
    #[serde(borrow)]
    pub lines: Vec<RowUpdateRef<'a>>,
    pub new_styles: Vec<(StyleId, Style)>,
    #[serde(borrow)]
    pub new_graphemes: Vec<(GraphemeId, &'a str)>,
    pub cursor: Cursor,
    pub viewport: Viewport,
    pub modes: ModeFlags,
    #[serde(borrow)]
    pub title: Option<&'a str>,
    pub bell: bool,
    pub images: Vec<Placement>,
    pub selection: Option<Selection>,
    pub ack_status: Vec<(InputId, AckState)>,
    pub palette: Option<Palette>,
}

impl FrameRef<'_> {
    /// Copies the borrowed data into an owned [`Frame`].
    pub fn to_frame(&self) -> Frame {
        Frame {
            pane: self.pane,
            reset_epoch: self.reset_epoch,
            base_rev: self.base_rev,
            rev: self.rev,
            kind: self.kind,
            cols: self.cols,
            rows: self.rows,
            lines: self
                .lines
                .iter()
                .map(|l| RowUpdate {
                    y: l.y,
                    cells: l.cells.to_vec(),
                })
                .collect(),
            new_styles: self.new_styles.clone(),
            new_graphemes: self
                .new_graphemes
                .iter()
                .map(|(id, s)| (*id, (*s).to_owned()))
                .collect(),
            cursor: self.cursor,
            viewport: self.viewport,
            modes: self.modes,
            title: self.title.map(str::to_owned),
            bell: self.bell,
            images: self.images.clone(),
            selection: self.selection,
            ack_status: self.ack_status.clone(),
            palette: self.palette.clone(),
        }
    }
}
