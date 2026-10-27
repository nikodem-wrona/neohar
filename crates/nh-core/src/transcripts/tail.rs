//! Reads the new lines of one transcript file.
//!
//! A [`Tail`] holds the place in the file, the ledger of the count and, when a window follows the
//! timeline of the run, the items of that timeline. [`Tail::poll`] does blocking file I/O and
//! touches no state of the core. It gives a [`Batch`], and the actor applies the batch.

use std::{
    collections::VecDeque,
    fs::File,
    io::{ErrorKind, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use nh_agents::transcript::{
    Billing, Block, Builder, Entry, Format, Kind, Ledger, LimitWindow, Line, LineBuffer, Prices,
    Saved, UsageDelta, parse_line,
};
use nh_proto::api::{common::Harness, model::TimelineItem, time::Timestamp};

/// The most bytes that one poll reads from a file. The rest comes with the next poll, so a very
/// large transcript does not hold the service for long.
const MAX_PASS: u64 = 8 * 1024 * 1024;
/// The bytes of one read.
const CHUNK: usize = 64 * 1024;
/// The items that `timeline:<run>` holds.
pub(crate) const RING: usize = 500;

pub(crate) fn format_of(harness: Harness) -> Format {
    match harness {
        Harness::Claude => Format::Claude,
        Harness::Codex => Format::Codex,
    }
}

/// The end of a tool call that the transcript shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ToolEnd {
    pub call: String,
    pub ok: bool,
    pub at: Timestamp,
}

/// What a poll changes in the timeline projection.
#[derive(Debug, PartialEq)]
pub(crate) enum TimelineUpdate {
    /// Nobody follows the timeline now.
    Drop,
    Set {
        /// The projection starts again from these items.
        reset: bool,
        upserts: Vec<TimelineItem>,
        evicted: Vec<String>,
        /// The ids of all the items, oldest first.
        order: Vec<String>,
    },
}

/// What one poll found.
#[derive(Debug, Default)]
pub(crate) struct Batch {
    /// The first byte that no whole line has used. The actor saves it.
    pub offset: u64,
    /// The state of the ledger to save with the offset. `None` when nothing changed.
    pub ledger_json: Option<String>,
    pub deltas: Vec<UsageDelta>,
    pub tool_ends: Vec<ToolEnd>,
    /// The windows of the latest rate limit line.
    pub limits: Option<Vec<LimitWindow>>,
    pub timeline: Option<TimelineUpdate>,
}

/// The items of a timeline, for the latest [`RING`] of them.
struct TimelineState {
    builder: Builder,
    ring: VecDeque<TimelineItem>,
}

impl TimelineState {
    fn new() -> Self {
        Self {
            builder: Builder::default(),
            ring: VecDeque::new(),
        }
    }

    /// Takes the items of an entry. It gives the items to publish and the ids that left the ring.
    fn feed(
        &mut self,
        entry: &Entry,
        now: Timestamp,
        upserts: &mut Vec<TimelineItem>,
        evicted: &mut Vec<String>,
    ) {
        for item in self.builder.feed(entry, now) {
            match self.ring.iter_mut().rev().find(|old| old.id == item.id) {
                Some(old) => *old = item.clone(),
                None => {
                    self.ring.push_back(item.clone());
                    if self.ring.len() > RING
                        && let Some(old) = self.ring.pop_front()
                    {
                        evicted.push(old.id);
                    }
                }
            }
            upserts.push(item);
        }
    }

    fn order(&self) -> Vec<String> {
        self.ring.iter().map(|item| item.id.clone()).collect()
    }
}

/// The count of one transcript.
pub(crate) struct Tail {
    pub path: PathBuf,
    format: Format,
    buffer: LineBuffer,
    /// The next byte to read. It is ahead of the buffer when a line is unfinished.
    read_pos: u64,
    ledger: Ledger,
    billing: Billing,
    prices: &'static Prices,
    timeline: Option<TimelineState>,
    /// A run that ended and was read to the end of its file needs no more polls.
    pub finished: bool,
}

impl Tail {
    pub fn new(
        harness: Harness,
        path: PathBuf,
        offset: u64,
        ledger: Ledger,
        prices: &'static Prices,
    ) -> Self {
        Self {
            path,
            format: format_of(harness),
            buffer: LineBuffer::at(offset),
            read_pos: offset,
            ledger,
            billing: Billing::Api,
            prices,
            timeline: None,
            finished: false,
        }
    }

    /// The saved state of a ledger, or an empty ledger when the text does not parse.
    pub fn ledger_from(json: Option<&str>) -> Ledger {
        json.and_then(|text| serde_json::from_str::<Saved>(text).ok())
            .map(Ledger::restore)
            .unwrap_or_default()
    }

    /// The ledger, for a tail that moves to another file of the same run.
    pub fn into_ledger(self) -> Ledger {
        self.ledger
    }

    pub fn set_billing(&mut self, billing: Billing) {
        self.billing = billing;
    }

    /// Reads what the file got since the last poll. `None` when nothing changed or the file is
    /// not there yet.
    pub fn poll(&mut self, wants_timeline: bool, now: Timestamp) -> Option<Batch> {
        let len = std::fs::metadata(&self.path).ok()?.len();
        let mut batch = Batch::default();
        let mut reset = false;
        if len < self.read_pos {
            // The file is shorter: it was replaced. Read it again from its start.
            self.buffer.reset();
            self.read_pos = 0;
            self.timeline = None;
            reset = true;
        }
        match (wants_timeline, self.timeline.is_some()) {
            (true, false) => {
                let state = scan(&self.path, self.format, self.buffer.offset());
                batch.timeline = Some(TimelineUpdate::Set {
                    reset: true,
                    upserts: state.ring.iter().cloned().collect(),
                    evicted: Vec::new(),
                    order: state.order(),
                });
                self.timeline = Some(state);
            }
            (false, true) => {
                self.timeline = None;
                batch.timeline = Some(TimelineUpdate::Drop);
            }
            _ => {}
        }
        let lines = if len > self.read_pos {
            self.read_new(len)
        } else {
            Vec::new()
        };
        let mut upserts = Vec::new();
        let mut evicted = Vec::new();
        for line in &lines {
            for entry in parse_line(self.format, &line.text, line.offset) {
                self.apply(&entry, now, &mut batch);
                if let Some(timeline) = &mut self.timeline {
                    timeline.feed(&entry, now, &mut upserts, &mut evicted);
                }
            }
        }
        if let Some(timeline) = &self.timeline
            && (!upserts.is_empty() || !evicted.is_empty())
        {
            match &mut batch.timeline {
                // The timeline was just built: the new items join the ones of the scan.
                Some(TimelineUpdate::Set {
                    upserts: first,
                    evicted: gone,
                    order,
                    ..
                }) => {
                    first.extend(upserts);
                    gone.extend(evicted);
                    *order = timeline.order();
                }
                _ => {
                    batch.timeline = Some(TimelineUpdate::Set {
                        reset: false,
                        upserts,
                        evicted,
                        order: timeline.order(),
                    });
                }
            }
        }
        let changed = reset || !lines.is_empty();
        if changed {
            batch.ledger_json = serde_json::to_string(&self.ledger.save()).ok();
        } else if batch.timeline.is_none() {
            return None;
        }
        batch.offset = self.buffer.offset();
        Some(batch)
    }

    fn apply(&mut self, entry: &Entry, now: Timestamp, batch: &mut Batch) {
        batch
            .deltas
            .extend(self.ledger.apply(entry, self.prices, self.billing, now));
        match &entry.kind {
            Kind::ToolResult {
                tool_use, failed, ..
            } => batch.tool_ends.push(ToolEnd {
                call: tool_use.clone(),
                ok: !failed,
                at: entry.at.unwrap_or(now),
            }),
            Kind::Limits { windows } => batch.limits = Some(windows.clone()),
            _ => {}
        }
    }

    /// Reads the bytes after `read_pos`, up to `len` and at most [`MAX_PASS`] of them, and returns
    /// the lines that they complete.
    fn read_new(&mut self, len: u64) -> Vec<Line> {
        let Ok(mut file) = File::open(&self.path) else {
            return Vec::new();
        };
        if file.seek(SeekFrom::Start(self.read_pos)).is_err() {
            return Vec::new();
        }
        let want = (len - self.read_pos).min(MAX_PASS);
        self.read_from(&mut file, want)
    }

    /// Reads at most `want` bytes from `reader`, which stands at `read_pos`. A read error ends the
    /// pass early, and the lines before it stay: `read_pos` is past their bytes, so no later pass
    /// reads them again.
    pub(super) fn read_from(&mut self, reader: &mut impl Read, want: u64) -> Vec<Line> {
        let mut lines = Vec::new();
        let mut chunk = vec![0u8; CHUNK];
        let mut left = want;
        while left > 0 {
            let size = usize::try_from(left).unwrap_or(CHUNK).min(CHUNK);
            let read = match reader.read(&mut chunk[..size]) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break,
            };
            self.read_pos += read as u64;
            left -= read as u64;
            lines.extend(self.buffer.push(&chunk[..read]));
        }
        lines
    }
}

/// The timeline of a file up to a byte: the place where the count stands. The lines after it
/// come with the next reads.
fn scan(path: &Path, format: Format, limit: u64) -> TimelineState {
    let mut state = TimelineState::new();
    let Ok(file) = File::open(path) else {
        return state;
    };
    let mut file = file.take(limit);
    let mut buffer = LineBuffer::at(0);
    let mut chunk = vec![0u8; CHUNK];
    let now = Timestamp(crate::persist::now_ms());
    let (mut upserts, mut evicted) = (Vec::new(), Vec::new());
    while let Ok(read) = file.read(&mut chunk) {
        if read == 0 {
            break;
        }
        for line in buffer.push(&chunk[..read]) {
            for entry in parse_line(format, &line.text, line.offset) {
                state.feed(&entry, now, &mut upserts, &mut evicted);
                upserts.clear();
                evicted.clear();
            }
        }
    }
    state
}

/// All the items of a transcript, in order, for `timeline.page`. A file that is not there gives
/// nothing.
/// The full text of a `prompt` item or an `assistant` item of the timeline (`timeline.item`).
/// The ids are those of [`Builder`]: the id of the line for a prompt, and the id of the line, a
/// dot and the index of the block for a text of the model. A later line with the same id
/// wins, as in the timeline. `None` when no such item is in the file.
pub(crate) fn item_text(path: &Path, format: Format, item: &str) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut buffer = LineBuffer::at(0);
    let mut chunk = vec![0u8; CHUNK];
    let mut found = None;
    while let Ok(read) = file.read(&mut chunk) {
        if read == 0 {
            break;
        }
        for line in buffer.push(&chunk[..read]) {
            for entry in parse_line(format, &line.text, line.offset) {
                if entry.sidechain {
                    continue;
                }
                match &entry.kind {
                    Kind::Prompt { text } if entry.id == item => found = Some(text.clone()),
                    Kind::Assistant { blocks, .. } => {
                        let Some(index) = item
                            .strip_prefix(entry.id.as_str())
                            .and_then(|rest| rest.strip_prefix('.'))
                            .and_then(|rest| rest.parse::<usize>().ok())
                        else {
                            continue;
                        };
                        if let Some(Block::Text(text)) = blocks.get(index) {
                            found = Some(text.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    found
}

pub(crate) fn all_items(path: &Path, format: Format) -> Vec<TimelineItem> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let mut file = file;
    let mut buffer = LineBuffer::at(0);
    let mut builder = Builder::default();
    let mut items: Vec<TimelineItem> = Vec::new();
    let mut index = std::collections::HashMap::<String, usize>::new();
    let now = Timestamp(crate::persist::now_ms());
    let mut chunk = vec![0u8; CHUNK];
    while let Ok(read) = file.read(&mut chunk) {
        if read == 0 {
            break;
        }
        for line in buffer.push(&chunk[..read]) {
            for entry in parse_line(format, &line.text, line.offset) {
                for item in builder.feed(&entry, now) {
                    match index.get(&item.id) {
                        Some(at) => items[*at] = item,
                        None => {
                            index.insert(item.id.clone(), items.len());
                            items.push(item);
                        }
                    }
                }
            }
        }
    }
    items
}
