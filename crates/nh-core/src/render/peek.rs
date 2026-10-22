//! The peek stream: keyframes of a pane that is not in a grid, at a low rate.
//!
//! A peek frame is a keyframe of the bottom `max_rows` rows and the left `max_cols` columns of the
//! pane. An agent keeps its prompt and its dialogs at the bottom, so the bottom is what a card or a
//! thumbnail shows. Every frame is a keyframe, so the client keeps no state, and a frame that it
//! drops costs nothing. The stream sends at most `max_hz` frames a second, and never more than 4.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use nh_proto::{
    cells::{CELL_BYTES, STYLE_DEFAULT, pack_cell, unpack_cell},
    frame::{Frame, FrameKind},
    style::CellWidth,
};
use nh_term::ClientState;
use tokio::{sync::Notify, time::sleep};

use super::FrameSink;
use crate::pane::Pane;

/// The most frames a second.
pub const MAX_PEEK_HZ: f32 = 4.0;

/// How often a stream with nothing to send looks at its sink.
const CLOSED_CHECK: Duration = Duration::from_secs(5);

/// Cuts a keyframe to the bottom `max_rows` rows and the left `max_cols` columns. The cursor moves
/// with the rows, and it is hidden when it is outside. A wide character that the cut splits
/// becomes a blank cell.
pub(crate) fn crop(frame: &mut Frame, max_cols: u16, max_rows: u16) {
    let cols = frame.cols.min(max_cols.max(1));
    let rows = frame.rows.min(max_rows.max(1));
    let skip = frame.rows - rows;
    let wide = |style: u16, styles: &[(u16, nh_proto::style::Style)]| {
        styles
            .iter()
            .find(|(id, _)| *id == style)
            .is_some_and(|(_, style)| style.width == CellWidth::Wide)
    };
    let styles = frame.new_styles.clone();
    frame.lines.retain(|row| row.y >= skip);
    for row in &mut frame.lines {
        row.y -= skip;
        row.cells.truncate(usize::from(cols) * CELL_BYTES);
        if frame.cols > cols
            && let Some(last) = row.cells.as_chunks_mut::<CELL_BYTES>().0.last_mut()
        {
            let (_, style) = unpack_cell(*last);
            if wide(style, &styles) {
                *last = pack_cell(u32::from(' '), STYLE_DEFAULT);
            }
        }
    }
    (frame.cols, frame.rows) = (cols, rows);
    let cursor = &mut frame.cursor;
    if cursor.y >= skip && cursor.y - skip < rows && cursor.x < cols {
        cursor.y -= skip;
    } else {
        cursor.visible = false;
        (cursor.x, cursor.y) = (0, 0);
    }
    frame.selection = None;
}

/// Sends keyframes of `pane` to `sink` until the sink is gone, the pane ends, or `cancel` is
/// notified.
pub(super) async fn run(
    pane: Arc<Pane>,
    hz: f32,
    max_cols: u16,
    max_rows: u16,
    sink: Arc<dyn FrameSink>,
    cancel: Arc<Notify>,
) {
    let period = Duration::from_secs_f32(1.0 / hz.clamp(0.1, MAX_PEEK_HZ));
    let mut damage = pane.subscribe_damage();
    let mut sent: Option<(u64, Instant)> = None;
    loop {
        if sent.is_none_or(|(rev, _)| rev != pane.rev()) {
            // Frames that come within one period are one frame.
            if let Some((_, at)) = sent {
                let due = at + period;
                if due > Instant::now() {
                    tokio::select! {
                        () = sleep(due - Instant::now()) => {}
                        () = cancel.notified() => return,
                    }
                }
            }
            let mut client = ClientState::new();
            let mut frame = pane.build_frame(&mut client, FrameKind::Keyframe);
            crop(&mut frame, max_cols, max_rows);
            let rev = frame.rev;
            let Ok(bytes) = postcard::to_stdvec(&frame) else {
                return;
            };
            if !sink.send(bytes) {
                return;
            }
            sent = Some((rev, Instant::now()));
            continue;
        }
        tokio::select! {
            changed = damage.changed() => {
                if changed.is_err() {
                    return;
                }
            }
            () = sleep(CLOSED_CHECK) => {
                if sink.is_closed() {
                    return;
                }
            }
            () = cancel.notified() => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use nh_proto::{
        frame::{Cursor, RowUpdate, Viewport},
        modes::ModeFlags,
        style::Style,
    };

    use super::*;

    /// A frame of 6 columns and 4 rows. Row `y` has the letters `a` to `f` shifted by `y`.
    fn frame() -> Frame {
        let lines = (0..4u16)
            .map(|y| RowUpdate {
                y,
                cells: (0..6u32)
                    .flat_map(|x| pack_cell(u32::from(b'a') + x + u32::from(y), STYLE_DEFAULT))
                    .collect(),
            })
            .collect();
        Frame {
            pane: 1,
            reset_epoch: 0,
            base_rev: 0,
            rev: 5,
            kind: FrameKind::Keyframe,
            cols: 6,
            rows: 4,
            lines,
            new_styles: Vec::new(),
            new_graphemes: Vec::new(),
            cursor: Cursor {
                x: 2,
                y: 3,
                visible: true,
                ..Cursor::default()
            },
            viewport: Viewport::default(),
            modes: ModeFlags::empty(),
            title: None,
            bell: false,
            images: Vec::new(),
            selection: None,
            ack_status: Vec::new(),
            palette: None,
        }
    }

    fn texts(frame: &Frame) -> Vec<String> {
        frame
            .lines
            .iter()
            .map(|row| {
                row.cells
                    .as_chunks::<CELL_BYTES>()
                    .0
                    .iter()
                    .map(|cell| char::from_u32(unpack_cell(*cell).0).unwrap_or('?'))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_crop_keeps_the_bottom_rows_and_the_left_columns() {
        let mut frame = frame();
        crop(&mut frame, 4, 2);
        assert_eq!((frame.cols, frame.rows), (4, 2));
        assert_eq!(texts(&frame), ["cdef".to_owned(), "defg".to_owned()]);
        assert_eq!(frame.lines.iter().map(|r| r.y).collect::<Vec<_>>(), [0, 1]);
        // The cursor was in the last row, and it stays in it.
        assert_eq!(
            (frame.cursor.x, frame.cursor.y, frame.cursor.visible),
            (2, 1, true)
        );
    }

    #[test]
    fn a_cursor_outside_the_crop_is_hidden() {
        let mut frame = frame();
        frame.cursor.y = 0;
        crop(&mut frame, 6, 2);
        assert!(!frame.cursor.visible);
        let mut frame = self::frame();
        frame.cursor.x = 5;
        crop(&mut frame, 3, 4);
        assert!(!frame.cursor.visible);
    }

    #[test]
    fn a_crop_larger_than_the_frame_changes_nothing() {
        let mut frame = frame();
        let before = frame.clone();
        crop(&mut frame, 80, 40);
        assert_eq!(frame, before);
    }

    #[test]
    fn a_wide_character_that_the_cut_splits_becomes_a_blank_cell() {
        let mut frame = frame();
        let wide = Style {
            width: CellWidth::Wide,
            ..Style::default()
        };
        frame.new_styles = vec![(7, wide)];
        // The third cell of the last row is the first half of a wide character.
        let row = frame.lines.last_mut().expect("a row");
        let cell = pack_cell(u32::from('W'), 7);
        row.cells[2 * CELL_BYTES..3 * CELL_BYTES].copy_from_slice(&cell);
        crop(&mut frame, 3, 1);
        let last = frame.lines[0].cells.as_chunks::<CELL_BYTES>().0[2];
        assert_eq!(unpack_cell(last), (u32::from(' '), STYLE_DEFAULT));
    }
}
