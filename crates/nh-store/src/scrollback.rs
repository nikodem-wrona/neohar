//! The scrollback archive.
//!
//! Lines that scroll off the screen of a pane are appended to **segment files**,
//! `scrollback/<pane>/<segment_no>.zst`. A flush writes one record: a length (`u32`, little
//! endian) and an independent zstd frame that holds a `postcard` list of lines. The table
//! `scrollback_segments` has the first and the last line of each segment and the number of bytes
//! that are valid.
//!
//! The order of a flush makes a crash harmless. The writer cuts the file back to the valid length,
//! appends the frame, syncs it, and then commits the database in one transaction: the segment row
//! and `panes.last_archived_line`. A crash before the commit leaves bytes after the valid length.
//! Readers never look at them, and the next flush cuts them off. The engine replays the lines after
//! `last_archived_line` when the core starts again, so a crash loses at most the lines of the last
//! flush interval.

use std::{
    collections::HashMap,
    fs::{self, DirBuilder, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::PathBuf,
};

use nh_proto::{
    api::streams::ScrollbackPage,
    cells::{GRAPHEME_FLAG, MAX_GRAPHEME_ID, STYLE_DEFAULT, pack_cell},
    frame::{GraphemeId, RowUpdate, StyleId},
    style::{CellWidth, Style},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ulid::Ulid;
use unicode_segmentation::UnicodeSegmentation;

use crate::{Store, StoreError, degraded};

/// How long a line waits in the buffer of the writer before a flush is due.
pub const FLUSH_INTERVAL_MS: i64 = 1_000;
/// A segment ends after this many lines.
pub const SEGMENT_LINES: u64 = 10_000;
/// A segment ends after about this many bytes of uncompressed lines.
pub const SEGMENT_BYTES: usize = 4 * 1024 * 1024;
/// The level of zstd. Lines of a terminal compress well at a low level, and the writer is fast.
const ZSTD_LEVEL: i32 = 3;

/// A run of cells with one style, in columns (`nh_term::StyleRun`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchivedRun {
    pub start: u16,
    pub len: u16,
    pub style: Style,
}

/// A line of the archive (`nh_term::HistoryLine`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchivedLine {
    /// The absolute number of the line: 0 for the first line that left the screen of the pane.
    pub line_no: u64,
    /// The row continues on the next row, a soft wrap.
    pub wrapped: bool,
    /// The text without the blank cells at the end.
    pub text: String,
    /// The cells whose style is not the default one.
    pub runs: Vec<ArchivedRun>,
}

impl ArchivedLine {
    /// A rough size of the line, to cut segments by size.
    fn size(&self) -> usize {
        self.text.len() + self.runs.len() * 12 + 16
    }
}

/// What a flush did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flushed {
    /// The lines that went to the archive.
    pub lines: usize,
    /// The segments that got a frame (also a new one).
    pub segments: usize,
    /// The bytes that were appended.
    pub bytes: u64,
}

/// A segment as the table has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentInfo {
    pub segment_no: u64,
    pub first_line: u64,
    pub last_line: u64,
    pub bytes: u64,
}

/// The place of the writer in the files. It is copied before a flush and kept only after the
/// commit.
#[derive(Clone, Copy, Debug)]
struct Position {
    /// The segment that new lines go to, or `None` before the first one.
    segment: Option<SegmentPosition>,
    /// The highest line that the archive holds.
    last_line: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
struct SegmentPosition {
    no: u64,
    first_line: u64,
    lines: u64,
    uncompressed: usize,
    /// The valid bytes of the file, as the table has them.
    bytes: u64,
}

/// The writer of the archive of one pane. It buffers lines and writes them on a flush.
pub struct ScrollbackWriter {
    pane: Ulid,
    dir: PathBuf,
    position: Position,
    buffer: Vec<ArchivedLine>,
    /// The highest line number in the buffer or in the archive, for the replay rule.
    highest: Option<u64>,
    buffer_since_ms: Option<i64>,
    max_lines: u64,
    max_bytes: usize,
}

fn io_error(path: &std::path::Path, source: io::Error) -> StoreError {
    match degraded::classify_io(&source) {
        Some(reason) => StoreError::Degraded { reason },
        None => StoreError::Io {
            path: path.to_owned(),
            source,
        },
    }
}

fn to_i64(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::BadValue(format!("{value} does not fit")))
}

fn to_u64(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| StoreError::BadValue(format!("{value} is negative")))
}

/// Writes a frame: the length of the compressed record, and the record.
fn encode_frame(lines: &[ArchivedLine]) -> Result<Vec<u8>, StoreError> {
    let raw = postcard::to_stdvec(lines)
        .map_err(|e| StoreError::BadValue(format!("a line does not encode: {e}")))?;
    let packed = zstd::bulk::compress(&raw, ZSTD_LEVEL)
        .map_err(|source| StoreError::BadValue(format!("zstd failed: {source}")))?;
    let length = u32::try_from(packed.len())
        .map_err(|_| StoreError::BadValue("a frame above 4 GiB".to_owned()))?;
    let mut frame = Vec::with_capacity(packed.len() + 4);
    frame.extend_from_slice(&length.to_le_bytes());
    frame.extend_from_slice(&packed);
    Ok(frame)
}

/// The lines of the first `valid` bytes of a segment file. A frame that is cut off at the end is
/// not an error: the caller passes the valid length, and the bytes after it are not read.
fn decode_segment(bytes: &[u8]) -> Result<Vec<ArchivedLine>, StoreError> {
    let mut lines = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        let Some((head, tail)) = rest.split_first_chunk::<4>() else {
            return Err(StoreError::BadValue(
                "a segment ends inside a length".to_owned(),
            ));
        };
        let length = u32::from_le_bytes(*head) as usize;
        if tail.len() < length {
            return Err(StoreError::BadValue(
                "a segment ends inside a frame".to_owned(),
            ));
        }
        let (frame, after) = tail.split_at(length);
        // A frame holds at most the lines of one flush. The cap is a guard against a bad file.
        let raw = zstd::bulk::decompress(frame, 1 << 30)
            .map_err(|e| StoreError::BadValue(format!("a frame does not decompress: {e}")))?;
        let mut batch: Vec<ArchivedLine> = postcard::from_bytes(&raw)
            .map_err(|e| StoreError::BadValue(format!("a frame does not decode: {e}")))?;
        lines.append(&mut batch);
        rest = after;
    }
    Ok(lines)
}

impl Store {
    fn scrollback_dir(&self, pane: Ulid) -> Result<PathBuf, StoreError> {
        Ok(self.data_dir()?.join("scrollback").join(pane.to_string()))
    }

    /// The writer of the archive of a pane. It continues where the archive of the pane ends.
    pub fn scrollback_writer(&self, pane: Ulid) -> Result<ScrollbackWriter, StoreError> {
        let key = pane.to_string();
        let row: Option<Option<i64>> = self
            .conn
            .query_row(
                "SELECT last_archived_line FROM panes WHERE id = ?1",
                [&key],
                |row| row.get(0),
            )
            .optional()?;
        let last_archived = row.ok_or_else(|| StoreError::BadValue(format!("no pane {key}")))?;
        let last_line = last_archived.map(to_u64).transpose()?;
        let segment = self
            .conn
            .query_row(
                "SELECT segment_no, first_line, last_line, bytes FROM scrollback_segments
                 WHERE pane_id = ?1 ORDER BY segment_no DESC LIMIT 1",
                [&key],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?
            .map(|(no, first, last, bytes)| -> Result<_, StoreError> {
                let (no, first, last, bytes) =
                    (to_u64(no)?, to_u64(first)?, to_u64(last)?, to_u64(bytes)?);
                Ok(SegmentPosition {
                    no,
                    first_line: first,
                    lines: last.saturating_sub(first) + 1,
                    uncompressed: 0,
                    bytes,
                })
            })
            .transpose()?;
        Ok(ScrollbackWriter {
            pane,
            dir: self.scrollback_dir(pane)?,
            position: Position { segment, last_line },
            buffer: Vec::new(),
            highest: last_line,
            buffer_since_ms: None,
            max_lines: SEGMENT_LINES,
            max_bytes: SEGMENT_BYTES,
        })
    }

    /// The segments of a pane, oldest first.
    pub fn scrollback_segments(&self, pane: Ulid) -> Result<Vec<SegmentInfo>, StoreError> {
        let mut statement = self.conn.prepare(
            "SELECT segment_no, first_line, last_line, bytes FROM scrollback_segments
             WHERE pane_id = ?1 ORDER BY segment_no",
        )?;
        let rows = statement.query_map([pane.to_string()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (no, first, last, bytes) = row?;
            Ok(SegmentInfo {
                segment_no: to_u64(no)?,
                first_line: to_u64(first)?,
                last_line: to_u64(last)?,
                bytes: to_u64(bytes)?,
            })
        })
        .collect()
    }

    /// Up to `count` lines of the archive of a pane, from line `from` on, in order. A line that the
    /// archive does not hold (it is older than the cap, or it did not arrive yet) is not in the
    /// list.
    pub fn read_lines(
        &self,
        pane: Ulid,
        from: u64,
        count: usize,
    ) -> Result<Vec<ArchivedLine>, StoreError> {
        if count == 0 {
            return Ok(Vec::new());
        }
        let end = from.saturating_add(count as u64);
        let mut statement = self.conn.prepare(
            "SELECT segment_no, bytes FROM scrollback_segments
             WHERE pane_id = ?1 AND last_line >= ?2 AND first_line < ?3 ORDER BY segment_no",
        )?;
        let segments: Vec<(u64, u64)> = statement
            .query_map(
                params![pane.to_string(), to_i64(from)?, to_i64(end)?],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )?
            .map(|row| {
                let (no, bytes) = row?;
                Ok((to_u64(no)?, to_u64(bytes)?))
            })
            .collect::<Result<_, StoreError>>()?;
        let dir = self.scrollback_dir(pane)?;
        let mut out = Vec::new();
        for (no, valid) in segments {
            let path = dir.join(format!("{no}.zst"));
            let mut file = fs::File::open(&path).map_err(|e| io_error(&path, e))?;
            let mut bytes = vec![0; usize::try_from(valid).unwrap_or(usize::MAX)];
            file.read_exact(&mut bytes)
                .map_err(|e| io_error(&path, e))?;
            out.extend(
                decode_segment(&bytes)?
                    .into_iter()
                    .filter(|line| line.line_no >= from && line.line_no < end),
            );
            if out.len() >= count {
                break;
            }
        }
        out.truncate(count);
        Ok(out)
    }

    /// The panes that have segments in the archive, in the order of their ids.
    pub fn scrollback_panes(&self) -> Result<Vec<Ulid>, StoreError> {
        let mut statement = self
            .conn
            .prepare("SELECT DISTINCT pane_id FROM scrollback_segments ORDER BY pane_id")?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| {
                id.parse()
                    .map_err(|_| StoreError::BadValue(format!("pane id {id:?}")))
            })
            .collect()
    }

    /// Deletes the oldest segments of a pane until the files hold at most `max_bytes`. The newest
    /// segment stays, because the writer appends to it. Returns the number of segments deleted.
    pub fn trim_scrollback(
        &mut self,
        pane: Ulid,
        max_bytes: u64,
        now_ms: i64,
    ) -> Result<usize, StoreError> {
        let segments = self.scrollback_segments(pane)?;
        let mut total: u64 = segments.iter().map(|s| s.bytes).sum();
        let mut doomed = Vec::new();
        for segment in segments.iter().take(segments.len().saturating_sub(1)) {
            if total <= max_bytes {
                break;
            }
            total -= segment.bytes;
            doomed.push(segment.clone());
        }
        if doomed.is_empty() {
            return Ok(0);
        }
        let key = pane.to_string();
        self.tx(now_ms, |tx| {
            for segment in &doomed {
                tx.sql().execute(
                    "DELETE FROM scrollback_segments WHERE pane_id = ?1 AND segment_no = ?2",
                    params![key, to_i64(segment.segment_no)?],
                )?;
            }
            Ok(())
        })?;
        // The files go after the commit. A crash in between leaves files that no row names.
        let dir = self.scrollback_dir(pane)?;
        for segment in &doomed {
            let _ = fs::remove_file(dir.join(format!("{}.zst", segment.segment_no)));
        }
        Ok(doomed.len())
    }
}

impl ScrollbackWriter {
    /// A writer with other limits for a segment, for tests.
    #[cfg(test)]
    fn with_limits(mut self, lines: u64, bytes: usize) -> Self {
        self.max_lines = lines;
        self.max_bytes = bytes;
        self
    }

    /// The pane that the writer serves.
    pub fn pane(&self) -> Ulid {
        self.pane
    }

    /// The highest line that the archive or the buffer holds.
    pub fn last_line(&self) -> Option<u64> {
        self.highest
    }

    /// The lines that wait for a flush.
    pub fn buffered(&self) -> usize {
        self.buffer.len()
    }

    /// Takes lines. A line whose number is not above the highest one that the writer has is
    /// dropped: the engine replays lines after a restart, and the archive does not hold them twice
    /// (the replay rule). Returns the number of lines taken.
    pub fn push(&mut self, lines: impl IntoIterator<Item = ArchivedLine>, now_ms: i64) -> usize {
        let mut taken = 0;
        for line in lines {
            if self.highest.is_some_and(|highest| line.line_no <= highest) {
                continue;
            }
            self.highest = Some(line.line_no);
            self.buffer.push(line);
            taken += 1;
        }
        if taken > 0 && self.buffer_since_ms.is_none() {
            self.buffer_since_ms = Some(now_ms);
        }
        taken
    }

    /// Whether a flush is due: lines wait, and the oldest of them waited a second, or the buffer
    /// holds as much as a segment.
    pub fn flush_due(&self, now_ms: i64) -> bool {
        match self.buffer_since_ms {
            None => false,
            Some(since) => {
                now_ms.saturating_sub(since) >= FLUSH_INTERVAL_MS
                    || self.buffer.len() as u64 >= self.max_lines
            }
        }
    }

    /// Writes the buffered lines. See the module comment for the order. If the write or the commit
    /// fails, the lines stay in the buffer and the next flush tries again.
    pub fn flush(&mut self, store: &mut Store, now_ms: i64) -> Result<Flushed, StoreError> {
        if self.buffer.is_empty() {
            return Ok(Flushed::default());
        }
        store.guard()?;
        // Cut the lines into segments.
        let mut position = self.position;
        let mut chunks: Vec<Chunk> = Vec::new();
        for line in &self.buffer {
            let needs_new = position
                .segment
                .is_none_or(|s| s.lines >= self.max_lines || s.uncompressed >= self.max_bytes);
            let mut segment = if needs_new {
                SegmentPosition {
                    no: position.segment.map_or(0, |s| s.no + 1),
                    first_line: line.line_no,
                    lines: 0,
                    uncompressed: 0,
                    bytes: 0,
                }
            } else if let Some(segment) = position.segment {
                segment
            } else {
                continue;
            };
            segment.lines += 1;
            segment.uncompressed += line.size();
            if chunks.last().is_none_or(|c| c.segment_no != segment.no) {
                chunks.push(Chunk::new(segment.no, segment.first_line));
            }
            if let Some(chunk) = chunks.last_mut() {
                chunk.last_line = line.line_no;
                chunk.lines.push(line.clone());
            }
            position.segment = Some(segment);
        }

        // Append the frames. The valid length of each segment is what the table says.
        let mut appended = 0u64;
        for chunk in &mut chunks {
            let frame = encode_frame(&chunk.lines)?;
            let start = position_of(&self.position, chunk.segment_no).unwrap_or(0);
            let path = self.dir.join(format!("{}.zst", chunk.segment_no));
            let end = self
                .append(&path, start, &frame)
                .map_err(|e| io_error(&path, e))?;
            chunk.bytes = end;
            appended += frame.len() as u64;
        }

        // Commit the rows in one transaction.
        let key = self.pane.to_string();
        let last = chunks.last().map_or(0, |c| c.last_line);
        let result = store.tx(now_ms, |tx| {
            for chunk in &chunks {
                tx.sql().execute(
                    "INSERT INTO scrollback_segments
                         (pane_id, segment_no, first_line, last_line, path, bytes)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT (pane_id, segment_no)
                     DO UPDATE SET last_line = excluded.last_line, bytes = excluded.bytes",
                    params![
                        key,
                        to_i64(chunk.segment_no)?,
                        to_i64(chunk.first_line)?,
                        to_i64(chunk.last_line)?,
                        format!("scrollback/{key}/{}.zst", chunk.segment_no),
                        to_i64(chunk.bytes)?,
                    ],
                )?;
            }
            tx.sql().execute(
                "UPDATE panes SET last_archived_line = ?2, updated_at = ?3 WHERE id = ?1",
                params![key, to_i64(last)?, now_ms],
            )?;
            Ok(())
        });
        result?;

        // The commit worked. Keep the new place.
        let lines = self.buffer.len();
        self.buffer.clear();
        self.buffer_since_ms = None;
        if let (Some(segment), Some(chunk)) = (position.segment.as_mut(), chunks.last())
            && segment.no == chunk.segment_no
        {
            segment.bytes = chunk.bytes;
        }
        position.last_line = Some(last);
        self.position = position;
        Ok(Flushed {
            lines,
            segments: chunks.len(),
            bytes: appended,
        })
    }

    /// Cuts the file back to `valid` bytes, appends `frame`, and syncs. Returns the new length.
    fn append(&self, path: &std::path::Path, valid: u64, frame: &[u8]) -> io::Result<u64> {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.dir)?;
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(path)?;
        // Bytes after the valid length are a frame that no commit named.
        file.set_len(valid)?;
        file.seek(SeekFrom::Start(valid))?;
        file.write_all(frame)?;
        file.sync_data()?;
        Ok(valid + frame.len() as u64)
    }
}

/// The valid length of a segment, as the writer knows it before the flush.
fn position_of(position: &Position, segment_no: u64) -> Option<u64> {
    position
        .segment
        .filter(|s| s.no == segment_no)
        .map(|s| s.bytes)
}

/// The lines of one flush that go to one segment.
struct Chunk {
    segment_no: u64,
    first_line: u64,
    last_line: u64,
    lines: Vec<ArchivedLine>,
    /// The valid length of the file after the frame.
    bytes: u64,
}

impl Chunk {
    fn new(segment_no: u64, first_line: u64) -> Self {
        Self {
            segment_no,
            first_line,
            last_line: first_line,
            lines: Vec::new(),
            bytes: 0,
        }
    }
}

// --- pages for the scrollback stream ----------------------------------------------------

/// A page could not be made.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum PageError {
    /// The stream used all style ids or all grapheme ids. The caller opens a new stream.
    #[error("the stream has no style or grapheme id left")]
    Overflow,
    /// A page holds up to 65,535 lines.
    #[error("a page holds at most 65535 lines")]
    TooManyLines,
}

/// Makes the pages of one scrollback stream. Style ids and grapheme ids are interned for the
/// stream, so a page carries only the ones that the client has not seen on this stream.
#[derive(Debug, Default)]
pub struct PageBuilder {
    styles: HashMap<Style, StyleId>,
    graphemes: HashMap<String, GraphemeId>,
}

impl PageBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// The page for `lines` of the pane with render id `pane`. `from_line` is the first line that
    /// the client asked for. A row has the cells up to the last one that is not blank. The client
    /// fills the rest of the width with blank cells.
    pub fn page(
        &mut self,
        pane: u32,
        from_line: u64,
        lines: &[ArchivedLine],
    ) -> Result<ScrollbackPage, PageError> {
        let mut rows = Vec::with_capacity(lines.len());
        let mut new_styles = Vec::new();
        let mut new_graphemes = Vec::new();
        for line in lines {
            let y = u16::try_from(line.line_no.saturating_sub(from_line))
                .map_err(|_| PageError::TooManyLines)?;
            let mut cells = Vec::with_capacity(line.text.len() * 6);
            let mut column: u16 = 0;
            let mut run = 0;
            let mut style_at = |column: u16| -> Style {
                while line
                    .runs
                    .get(run)
                    .is_some_and(|r| r.start.saturating_add(r.len) <= column)
                {
                    run += 1;
                }
                match line.runs.get(run) {
                    Some(r) if r.start <= column => r.style,
                    _ => Style::default(),
                }
            };
            for cluster in line.text.graphemes(true) {
                let style = style_at(column);
                let style_id = self.style_id(style, &mut new_styles)?;
                let mut chars = cluster.chars();
                let glyph = match (chars.next(), chars.next()) {
                    (Some(c), None) => c as u32,
                    _ => GRAPHEME_FLAG | self.grapheme_id(cluster, &mut new_graphemes)?,
                };
                cells.extend_from_slice(&pack_cell(glyph, style_id));
                column = column.saturating_add(1);
                if style.width == CellWidth::Wide {
                    let spacer = style_at(column);
                    let spacer_id = self.style_id(spacer, &mut new_styles)?;
                    cells.extend_from_slice(&pack_cell(u32::from(' '), spacer_id));
                    column = column.saturating_add(1);
                }
            }
            rows.push(RowUpdate { y, cells });
        }
        Ok(ScrollbackPage {
            pane,
            from_line,
            lines: rows,
            new_styles,
            new_graphemes,
        })
    }

    fn style_id(
        &mut self,
        style: Style,
        new: &mut Vec<(StyleId, Style)>,
    ) -> Result<StyleId, PageError> {
        if style == Style::default() {
            return Ok(STYLE_DEFAULT);
        }
        if let Some(id) = self.styles.get(&style) {
            return Ok(*id);
        }
        let id = StyleId::try_from(self.styles.len() + 1).map_err(|_| PageError::Overflow)?;
        self.styles.insert(style, id);
        new.push((id, style));
        Ok(id)
    }

    fn grapheme_id(
        &mut self,
        text: &str,
        new: &mut Vec<(GraphemeId, String)>,
    ) -> Result<GraphemeId, PageError> {
        if let Some(id) = self.graphemes.get(text) {
            return Ok(*id);
        }
        let id = GraphemeId::try_from(self.graphemes.len()).map_err(|_| PageError::Overflow)?;
        if id > MAX_GRAPHEME_ID {
            return Err(PageError::Overflow);
        }
        self.graphemes.insert(text.to_owned(), id);
        new.push((id, text.to_owned()));
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use nh_proto::{holder::ExitStatus, style::Color};

    use super::*;
    use crate::{PaneLiveness, PaneRecord};

    fn temp_store(name: &str) -> (Store, PathBuf) {
        let dir = std::env::temp_dir().join(format!("nh-store-sb-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("a directory");
        (Store::open(&dir.join("state.db")).expect("a store"), dir)
    }

    fn add_pane(store: &Store, render_id: u32) -> Ulid {
        let id = Ulid::from_parts(1_000 + u64::from(render_id), u128::from(render_id));
        store
            .insert_pane(
                &PaneRecord {
                    id,
                    render_id,
                    socket: "/tmp/s".to_owned(),
                    holder_pid: 1,
                    secret: [0; 32],
                    fencing_token: 1,
                    argv: vec![],
                    cwd: "/".to_owned(),
                    cols: 80,
                    rows: 24,
                    liveness: PaneLiveness::Running,
                    exit: None::<ExitStatus>,
                },
                1,
            )
            .expect("a pane");
        id
    }

    fn red() -> Style {
        Style {
            fg: Color::Palette(1),
            ..Style::default()
        }
    }

    fn line(n: u64) -> ArchivedLine {
        ArchivedLine {
            line_no: n,
            wrapped: n.is_multiple_of(5),
            text: format!("line {n} of the log"),
            runs: if n.is_multiple_of(7) {
                vec![ArchivedRun {
                    start: 0,
                    len: 4,
                    style: red(),
                }]
            } else {
                Vec::new()
            },
        }
    }

    fn lines(range: std::ops::Range<u64>) -> Vec<ArchivedLine> {
        range.map(line).collect()
    }

    fn flush_all(
        writer: &mut ScrollbackWriter,
        store: &mut Store,
        batch: Vec<ArchivedLine>,
        now: i64,
    ) -> Flushed {
        writer.push(batch, now);
        writer.flush(store, now).expect("a flush")
    }

    #[test]
    fn lines_are_written_and_read_back_exactly() {
        let (mut store, dir) = temp_store("roundtrip");
        let pane = add_pane(&store, 1);
        let mut writer = store.scrollback_writer(pane).expect("a writer");
        let flushed = flush_all(&mut writer, &mut store, lines(0..100), 10);
        assert_eq!((flushed.lines, flushed.segments), (100, 1));
        assert_eq!(writer.buffered(), 0);
        assert_eq!(
            store.read_lines(pane, 0, 1_000).expect("a read"),
            lines(0..100)
        );
        assert_eq!(
            store.read_lines(pane, 40, 10).expect("a read"),
            lines(40..50)
        );
        assert_eq!(
            store.read_lines(pane, 95, 100).expect("a read"),
            lines(95..100)
        );
        assert!(store.read_lines(pane, 200, 10).expect("a read").is_empty());
        assert!(store.read_lines(pane, 0, 0).expect("a read").is_empty());
        let last: i64 = store
            .conn
            .query_row(
                "SELECT last_archived_line FROM panes WHERE id = ?1",
                [pane.to_string()],
                |r| r.get(0),
            )
            .expect("a line");
        assert_eq!(last, 99);
        let segments = store.scrollback_segments(pane).expect("segments");
        assert_eq!(segments.len(), 1);
        assert_eq!((segments[0].first_line, segments[0].last_line), (0, 99));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_second_flush_appends_a_frame_to_the_same_segment() {
        let (mut store, dir) = temp_store("append");
        let pane = add_pane(&store, 1);
        let mut writer = store.scrollback_writer(pane).expect("a writer");
        flush_all(&mut writer, &mut store, lines(0..10), 1);
        let first = store.scrollback_segments(pane).expect("segments")[0].bytes;
        flush_all(&mut writer, &mut store, lines(10..20), 2_000);
        let segments = store.scrollback_segments(pane).expect("segments");
        assert_eq!(segments.len(), 1);
        assert!(segments[0].bytes > first);
        assert_eq!(segments[0].last_line, 19);
        assert_eq!(
            store.read_lines(pane, 0, 100).expect("a read"),
            lines(0..20)
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_segment_ends_after_the_line_limit_or_the_size_limit() {
        let (mut store, dir) = temp_store("segments");
        let pane = add_pane(&store, 1);
        let mut writer = store
            .scrollback_writer(pane)
            .expect("a writer")
            .with_limits(30, usize::MAX);
        let flushed = flush_all(&mut writer, &mut store, lines(0..100), 1);
        assert_eq!(flushed.segments, 4, "30 + 30 + 30 + 10");
        let segments = store.scrollback_segments(pane).expect("segments");
        let bounds: Vec<(u64, u64)> = segments
            .iter()
            .map(|s| (s.first_line, s.last_line))
            .collect();
        assert_eq!(bounds, [(0, 29), (30, 59), (60, 89), (90, 99)]);
        assert_eq!(
            store.read_lines(pane, 25, 40).expect("a read"),
            lines(25..65),
            "a read crosses segments"
        );

        let (mut store, dir2) = temp_store("segments-size");
        let pane = add_pane(&store, 1);
        let mut writer = store
            .scrollback_writer(pane)
            .expect("a writer")
            .with_limits(u64::MAX, 400);
        flush_all(&mut writer, &mut store, lines(0..100), 1);
        assert!(
            store.scrollback_segments(pane).expect("segments").len() > 2,
            "the size limit cuts segments"
        );
        assert_eq!(
            store.read_lines(pane, 0, 1_000).expect("a read"),
            lines(0..100)
        );
        let _ = (fs::remove_dir_all(dir), fs::remove_dir_all(dir2));
    }

    #[test]
    fn a_line_that_the_archive_holds_is_not_written_again() {
        let (mut store, dir) = temp_store("replay");
        let pane = add_pane(&store, 1);
        let mut writer = store.scrollback_writer(pane).expect("a writer");
        flush_all(&mut writer, &mut store, lines(0..50), 1);
        // The core restarts. The engine replays lines 30 to 79.
        drop(writer);
        let mut again = store.scrollback_writer(pane).expect("a writer");
        assert_eq!(again.last_line(), Some(49));
        assert_eq!(
            again.push(lines(30..80), 5),
            30,
            "only lines 50 to 79 are new"
        );
        again.flush(&mut store, 5).expect("a flush");
        assert_eq!(
            store.read_lines(pane, 0, 1_000).expect("a read"),
            lines(0..80)
        );
        // The same line twice in one push counts once.
        assert_eq!(again.push(vec![line(80), line(80), line(79)], 6), 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_crash_between_flushes_loses_only_the_buffer() {
        let (mut store, dir) = temp_store("crash");
        let pane = add_pane(&store, 1);
        let mut writer = store.scrollback_writer(pane).expect("a writer");
        flush_all(&mut writer, &mut store, lines(0..40), 1);
        writer.push(lines(40..60), 2);
        assert!(!writer.flush_due(500));
        assert!(writer.flush_due(2 + FLUSH_INTERVAL_MS));
        drop(writer); // The process ends before the flush.
        let mut again = store.scrollback_writer(pane).expect("a writer");
        assert_eq!(again.last_line(), Some(39));
        assert_eq!(
            store.read_lines(pane, 0, 1_000).expect("a read"),
            lines(0..40)
        );
        again.push(lines(40..60), 3);
        again.flush(&mut store, 3).expect("a flush");
        assert_eq!(
            store.read_lines(pane, 0, 1_000).expect("a read"),
            lines(0..60)
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn bytes_after_the_valid_length_are_never_read_and_the_next_flush_cuts_them() {
        let (mut store, dir) = temp_store("tail");
        let pane = add_pane(&store, 1);
        let mut writer = store.scrollback_writer(pane).expect("a writer");
        flush_all(&mut writer, &mut store, lines(0..20), 1);
        let path = dir.join("scrollback").join(pane.to_string()).join("0.zst");
        // A frame that was written and never committed, then half of another.
        let mut file = OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("the segment");
        file.write_all(&encode_frame(&lines(20..30)).expect("a frame"))
            .expect("a write");
        file.write_all(&[9, 9, 9, 9, 1, 2]).expect("a cut frame");
        drop(file);
        assert_eq!(
            store.read_lines(pane, 0, 100).expect("a read"),
            lines(0..20),
            "the tail is invisible"
        );
        let valid = store.scrollback_segments(pane).expect("segments")[0].bytes;
        assert!(fs::metadata(&path).expect("metadata").len() > valid);

        let mut again = store.scrollback_writer(pane).expect("a writer");
        flush_all(&mut again, &mut store, lines(20..25), 2);
        assert_eq!(
            store.read_lines(pane, 0, 100).expect("a read"),
            lines(0..25)
        );
        let segments = store.scrollback_segments(pane).expect("segments");
        assert_eq!(
            fs::metadata(&path).expect("metadata").len(),
            segments[0].bytes,
            "no garbage stays"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_commit_keeps_the_lines_for_the_next_flush() {
        let (mut store, dir) = temp_store("retry");
        let pane = add_pane(&store, 1);
        let mut writer = store.scrollback_writer(pane).expect("a writer");
        flush_all(&mut writer, &mut store, lines(0..10), 1);
        writer.push(lines(10..20), 2);
        store.inject_fault(Some(crate::Fault::IoError));
        assert!(matches!(
            writer.flush(&mut store, 3),
            Err(StoreError::Degraded { .. })
        ));
        assert_eq!(writer.buffered(), 10, "the lines wait");
        store.inject_fault(None);
        assert!(store.probe(4).expect("a probe"));
        writer.flush(&mut store, 5).expect("a flush");
        assert_eq!(
            store.read_lines(pane, 0, 100).expect("a read"),
            lines(0..20)
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_size_cap_deletes_the_oldest_segments() {
        let (mut store, dir) = temp_store("trim");
        let pane = add_pane(&store, 1);
        add_pane(&store, 2);
        let mut writer = store
            .scrollback_writer(pane)
            .expect("a writer")
            .with_limits(20, usize::MAX);
        flush_all(&mut writer, &mut store, lines(0..100), 1);
        assert_eq!(
            store.scrollback_panes().expect("panes"),
            [pane],
            "a pane with no lines has no archive"
        );
        let segments = store.scrollback_segments(pane).expect("segments");
        assert_eq!(segments.len(), 5);
        let keep = segments[3].bytes + segments[4].bytes;
        let deleted = store.trim_scrollback(pane, keep, 2).expect("a trim");
        assert_eq!(deleted, 3);
        let left = store.scrollback_segments(pane).expect("segments");
        assert_eq!(
            left.iter().map(|s| s.segment_no).collect::<Vec<_>>(),
            [3, 4]
        );
        assert!(
            !dir.join("scrollback")
                .join(pane.to_string())
                .join("0.zst")
                .exists()
        );
        assert!(
            dir.join("scrollback")
                .join(pane.to_string())
                .join("4.zst")
                .exists()
        );
        assert_eq!(
            store.read_lines(pane, 0, 1_000).expect("a read"),
            lines(60..100)
        );
        // The newest segment is never deleted, whatever the cap.
        assert_eq!(store.trim_scrollback(pane, 0, 3).expect("a trim"), 1);
        assert_eq!(store.scrollback_segments(pane).expect("segments").len(), 1);
        // The writer goes on in the newest segment.
        let mut again = store.scrollback_writer(pane).expect("a writer");
        flush_all(&mut again, &mut store, lines(100..110), 4);
        assert_eq!(store.read_lines(pane, 80, 100).expect("a read").len(), 30);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_page_has_cells_styles_and_graphemes_for_the_stream() {
        let wide = Style {
            width: CellWidth::Wide,
            ..Style::default()
        };
        let spacer = Style {
            width: CellWidth::Spacer,
            ..Style::default()
        };
        let page_lines = vec![
            ArchivedLine {
                line_no: 10,
                wrapped: false,
                text: "ab".to_owned(),
                runs: vec![ArchivedRun {
                    start: 1,
                    len: 1,
                    style: red(),
                }],
            },
            ArchivedLine {
                line_no: 11,
                wrapped: false,
                // A wide character, then "e" with a combining accent, then "x".
                text: "界e\u{301}x".to_owned(),
                runs: vec![
                    ArchivedRun {
                        start: 0,
                        len: 1,
                        style: wide,
                    },
                    ArchivedRun {
                        start: 1,
                        len: 1,
                        style: spacer,
                    },
                ],
            },
        ];
        let mut builder = PageBuilder::new();
        let page = builder.page(7, 10, &page_lines).expect("a page");
        assert_eq!((page.pane, page.from_line, page.lines.len()), (7, 10, 2));
        let cells = |row: &RowUpdate| -> Vec<(u32, u16)> {
            nh_proto::cells::unpack_row(&row.cells).collect()
        };
        let red_id = page
            .new_styles
            .iter()
            .find(|(_, s)| *s == red())
            .expect("red")
            .0;
        assert_eq!(
            cells(&page.lines[0]),
            [(u32::from('a'), 0), (u32::from('b'), red_id)]
        );
        assert_eq!((page.lines[0].y, page.lines[1].y), (0, 1));
        let second = cells(&page.lines[1]);
        assert_eq!(
            second.len(),
            4,
            "a wide character is two cells, so the row has four"
        );
        assert_eq!(second[0].0, '界' as u32);
        assert_eq!(second[1].0, u32::from(' '), "the spacer");
        assert_ne!(
            second[2].0 & GRAPHEME_FLAG,
            0,
            "e with an accent is a grapheme"
        );
        assert_eq!(second[3].0, u32::from('x'));
        let (id, text) = &page.new_graphemes[0];
        assert_eq!(text, "e\u{301}");
        assert_eq!(second[2].0, GRAPHEME_FLAG | id);
        // The next page of the stream sends only what is new.
        let next = builder.page(7, 12, &page_lines[..1]).expect("a page");
        assert!(next.new_styles.is_empty() && next.new_graphemes.is_empty());
        assert_eq!(
            cells(&next.lines[0])[1].1,
            red_id,
            "the id is the same on the whole stream"
        );
        assert_eq!(next.lines[0].y, 0, "a line before `from_line` has y 0");
    }

    #[test]
    fn a_page_of_lines_far_from_the_start_is_refused() {
        let mut builder = PageBuilder::new();
        let far = vec![line(70_000)];
        assert_eq!(builder.page(1, 0, &far), Err(PageError::TooManyLines));
    }

    /// A million lines through the writer: the exact lines come back from any range.
    #[test]
    fn a_million_lines_go_through_the_writer_and_come_back_exactly() {
        let (mut store, dir) = temp_store("million");
        let pane = add_pane(&store, 1);
        let mut writer = store.scrollback_writer(pane).expect("a writer");
        let started = std::time::Instant::now();
        for start in (0..1_000_000).step_by(5_000) {
            writer.push(lines(start..start + 5_000), start as i64);
            writer.flush(&mut store, start as i64).expect("a flush");
        }
        eprintln!("1,000,000 lines written in {:?}", started.elapsed());
        let segments = store.scrollback_segments(pane).expect("segments");
        assert_eq!(segments.len(), 100);
        assert_eq!(segments[99].last_line, 999_999);
        for (from, count) in [
            (0u64, 10usize),
            (9_995, 20),
            (123_456, 3_000),
            (500_000, 1),
            (999_990, 50),
            (777_700, 200),
        ] {
            let got = store.read_lines(pane, from, count).expect("a read");
            let expected = lines(from..(from + count as u64).min(1_000_000));
            assert_eq!(got, expected, "from {from}");
        }
        let _ = fs::remove_dir_all(dir);
    }
}
