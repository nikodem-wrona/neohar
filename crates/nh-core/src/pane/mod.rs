//! A pane: a holder, the VT engine that reads its output, and the frames of
//! its screen.
//!
//! [`PaneFactory::spawn`] starts a holder through the `nh` sidecar, connects to it and starts the
//! pane task. The task reads the events of the holder in the order of the journal, feeds the
//! output to the engine, applies the resizes at the markers where the holder recorded them, and
//! tells the render server through a `watch` channel that the screen changed.
//!
//! The task also makes the checkpoints (a snapshot of the engine at a cut point of the journal,
//! stored in `state.db`), and it follows the inputs that the holder did not acknowledge. After a
//! restart of the core, [`restore`] builds the pane again from the snapshot and the journal.

mod family;
mod inputs;
mod replay;
pub(crate) mod restore;
pub(crate) mod service;
mod task;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};

use nh_proto::{
    api::{
        calls::pane::ScrollTo,
        common::Urgency,
        events::{EventPayload, PaneBell, PaneInputUnconfirmed},
        ids::{PaneId, WindowId},
        model::{ClipboardDropped, ClipboardRequest},
    },
    frame::{Frame, FrameKind, Selection},
    holder::{ExitStatus, HoldConfig, HoldMode, HolderRequest},
    input::{LogicalInput, MouseKind},
    paths::Env,
    style::CellWidth,
    version::HOLDER_PROTO,
};
use nh_store::{PaneLiveness, PaneRecord};
use nh_term::{
    AlacrittyEngine, ClientState, FrameBuilder, MIN_HISTORY, VtEngine,
    input::{Delivery, InputModes, KeyConfig, encode_drop_files, encode_input, encode_paste},
    xterm_palette,
};
use serde_json::{Value, json};
use tokio::{
    runtime::Handle,
    sync::{mpsc, oneshot, watch},
};
use ulid::Ulid;

use self::inputs::Inputs;
pub use self::inputs::{Unconfirmed, UnconfirmedReason};
pub use self::restore::{PaneRestore, RestoreOutcome, RestoreReport};
use crate::{
    ErrorCode,
    actor::ActorHandle,
    error::{CoreError, RpcError},
    holder::{self, HolderTx, spawn::spawn_holder},
    host::{Host as _, HostNotification},
    layout::model::{Facts, Report, Runtime as LayoutRuntime},
    persist::{Persist, now_ms},
    projections::Projections,
    router::HostRouter,
    runtime::Tuning,
};

/// The input ids of the core itself (the replies to terminal queries) have this bit set, so that
/// they never equal an id that a client sends. The rest of the id is the fencing token of the core
/// shifted by 20 bits, plus a counter, so that the ids of two cores differ.
const INTERNAL_INPUT: u64 = 1 << 63;

/// The most unconfirmed inputs that the `panes` projection shows for one pane.
const UNCONFIRMED_SHOWN: usize = 32;

/// The largest grid a client can ask for. A bigger value is a mistake, and the engine allocates
/// the whole grid.
const MAX_DIMENSION: u16 = 1000;

/// The most bytes that one write to a pane may have. A holder message is at most 4 MiB, and a
/// bigger paste is a mistake of the caller.
const MAX_WRITE: usize = 1 << 20;

fn check_size(len: usize) -> Result<(), RpcError> {
    if len > MAX_WRITE {
        return Err(RpcError::invalid_params(format!(
            "{len} bytes are too many; a write to a pane has at most {MAX_WRITE}"
        )));
    }
    Ok(())
}

/// Locks a mutex. A panic in a pane task is reported by the supervisor; the data stays usable.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a pane runs and how big it is.
#[derive(Clone, Debug)]
pub struct PaneSpec {
    pub argv: Vec<String>,
    /// The working directory. The home directory when it is `None`.
    pub cwd: Option<String>,
    /// Variables that are set over the base environment of the host.
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
}

/// Whether the child of a pane runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Liveness {
    Running,
    /// The child ended. The pane keeps its last screen.
    Exited(ExitStatus),
    /// The connection to the holder ended while the child ran.
    HolderLost,
    /// The pane task panicked.
    Failed,
}

/// How a pane got its screen ("What is guaranteed").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recovery {
    /// A new pane. It read the journal from its start.
    Fresh,
    /// A snapshot of the engine and the journal after it.
    Snapshot,
    /// No snapshot, and the ring still holds the whole journal. The screen is exact.
    Journal,
    /// No snapshot covers the start of the ring. The pane read the ring only, so the screen may
    /// show artifacts. The UI says so once.
    RingOnly,
}

impl Recovery {
    /// The name in the API.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Snapshot => "snapshot",
            Self::Journal => "journal",
            Self::RingOnly => "ring_only",
        }
    }
}

/// The facts about a pane that the `panes` projection shows.
#[derive(Clone, Debug)]
pub struct PaneInfo {
    pub title: Option<String>,
    pub cols: u16,
    pub rows: u16,
    pub liveness: Liveness,
    /// The pane reads the journal after a restart and shows no final screen yet. It
    /// clears with the first revision after the replay.
    pub restoring: bool,
    pub recovery: Recovery,
    /// The newest inputs that the holder did not confirm, oldest first (`pane.input_unconfirmed`).
    pub input_unconfirmed: Vec<Unconfirmed>,
    /// An OSC 52 write that waits for the answer of the user.
    pub clipboard_request: Option<ClipboardRequest>,
    /// The last OSC 52 write that no client got.
    pub clipboard_dropped: Option<ClipboardDropped>,
    /// The window whose size the PTY follows.
    pub geometry_owner: Option<WindowId>,
    /// The working directory of the foreground group, from the walk of the watcher.
    /// `None` until the first walk reads it.
    pub cwd: Option<String>,
}

/// What the user decided about the clipboard writes of a pane, and the write that waits.
#[derive(Default)]
struct Clipboard {
    /// `Some(true)` allows the writes, `Some(false)` drops them, `None` asks.
    policy: Option<bool>,
    /// The text of the OSC 52 write that waits for an answer.
    pending: Option<String>,
}

/// The geometry lease. A PTY has one size. The window that sized the pane first, or
/// that sent input to it last, owns that size. The other windows draw the same grid letterboxed
/// or cropped, and they never resize the PTY.
#[derive(Default)]
struct Lease {
    owner: Option<WindowId>,
    /// When the owner changed.
    since: Option<Instant>,
    /// The size that each window asked for. A window that takes the lease gets this size.
    wanted: HashMap<WindowId, (u16, u16)>,
}

/// A lease changes owner at most this often, so that two windows that both get input do not
/// start a storm of `SIGWINCH`.
const LEASE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(500);

/// How long the input of other clients waits while a verified sequence of keys runs.
const KEYS_QUEUE_MAX: std::time::Duration = std::time::Duration::from_secs(2);

/// The state of a verified sequence of keys on a pane.
#[derive(Default)]
struct KeyLock {
    active: Option<KeySequence>,
}

struct KeySequence {
    since: Instant,
    /// The input of other clients that came during the sequence, in order.
    queue: Vec<(u64, Vec<u8>)>,
    /// A person typed in the pane, or input waited longer than the queue allows: the sequence
    /// must end without another key.
    interrupted: bool,
}

/// The lines that one notch of the wheel scrolls.
const WHEEL_LINES: i64 = 3;

/// The longest OSC 52 write that the core takes. A bigger one is dropped.
const MAX_CLIPBOARD: usize = 1 << 20;

/// The screen of a pane: the engine and the frame builder, which change together.
pub(crate) struct Screen {
    pub engine: AlacrittyEngine,
    pub frames: FrameBuilder,
    /// The journal offset after the last byte that the engine has read.
    pub fed_offset: u64,
    /// When the engine last changed its size.
    pub resized_at: Instant,
}

/// A request to the pane task.
pub(crate) enum Control {
    /// Something changed that moves the next deadline of the task. The task looks at its clocks
    /// again.
    Wake,
    /// Write the lines that wait to the archive, then answer. `pane.read` uses it.
    Flush(oneshot::Sender<()>),
    /// Make a checkpoint at the next cut point, then end the lease ("Quit"). The answer
    /// says whether the checkpoint is stored.
    Quit(oneshot::Sender<bool>),
}

/// One pane. Hosts and the render server hold it in an `Arc`.
pub struct Pane {
    id: Ulid,
    render_id: u32,
    holder_pid: i32,
    child_pid: i32,
    tx: HolderTx,
    screen: Mutex<Screen>,
    /// The revision of the newest change that a client can see.
    rev: AtomicU64,
    /// The newest revision whose damage was not minor.
    non_minor_rev: AtomicU64,
    damage: watch::Sender<u64>,
    info: Mutex<PaneInfo>,
    /// The size that the last `pane.resize` asked for. A second request for the same size does
    /// nothing, so that a client cannot cause a `SIGWINCH` without a change.
    asked_size: Mutex<(u16, u16)>,
    /// The inputs that wait for an ack, and the ones that got it.
    inputs: Mutex<Inputs>,
    /// The screen at the end of the replay, before the resize nudge, with its journal offset. Only
    /// the restart checks of `nh-chaos` read it: a nudge makes applications repaint, which hides a
    /// fault of the replay.
    #[cfg(feature = "debug")]
    restored_dump: Mutex<Option<(u64, nh_term::ScreenDump)>>,
    /// The pane has read its journal and the resize nudge is over. A new pane is
    /// settled at once. After a restart, applications redraw during the nudge, so a screen read
    /// before this is true may still change.
    settled: AtomicBool,
    /// Another pane object took the place of this one (a restart). It shows nothing in the layout
    /// and the log any more, because a late report of the old holder would show the new run as
    /// ended.
    retired: AtomicBool,
    internal_ids: AtomicU64,
    key_config: KeyConfig,
    projections: Arc<Projections>,
    /// The host router: notifications and the clipboard.
    host: Arc<HostRouter>,
    /// The client device that sent the last input to the pane. An OSC 52 write goes
    /// to its clipboard. An input without a device (the CLI) does not change it. The core does
    /// not keep it across a restart.
    input_device: Mutex<Option<Ulid>>,
    persist: Arc<Persist>,
    /// The OSC 52 writes of the program.
    clipboard: Mutex<Clipboard>,
    /// Which window the PTY follows, and what each window asked for.
    lease: Mutex<Lease>,
    /// A verified sequence of keys holds the input of the pane.
    keys: Mutex<KeyLock>,
    /// The size of a cell in device pixels, as a window measured it. The PTY reports it.
    cell_px: Mutex<(u16, u16)>,
    /// The process group in the foreground of the PTY, as the holder reports it.
    foreground: watch::Sender<Option<i32>>,
    /// The state actor, for a pane that belongs to the layout. Such a pane reports its state to
    /// the actor, and the `panes` projection shows it with the place from the layout. A pane
    /// without it (the panes of `debug.spawn_panes`) writes its own row.
    layout: Option<ActorHandle>,
    control: mpsc::UnboundedSender<Control>,
}

impl Pane {
    /// The id of the pane (a ULID).
    pub fn id(&self) -> Ulid {
        self.id
    }

    /// The number that frames use for this pane.
    pub fn render_id(&self) -> u32 {
        self.render_id
    }

    /// The pid of the holder.
    pub fn holder_pid(&self) -> i32 {
        self.holder_pid
    }

    /// The pid of the child, as the holder reported it at `Hello`.
    pub fn child_pid(&self) -> i32 {
        self.child_pid
    }

    /// The revision of the newest change that a client can see. It only grows.
    pub fn rev(&self) -> u64 {
        self.rev.load(Ordering::Acquire)
    }

    /// The newest revision with damage that is not minor.
    pub fn non_minor_rev(&self) -> u64 {
        self.non_minor_rev.load(Ordering::Acquire)
    }

    /// A receiver that wakes when the screen changed. The value is the new revision.
    pub fn subscribe_damage(&self) -> watch::Receiver<u64> {
        self.damage.subscribe()
    }

    /// The scrollback position of the pane: the absolute line at the top of the viewport, and the
    /// number of lines that the pane ever had (the archive included).
    pub fn viewport(&self) -> nh_proto::frame::Viewport {
        lock(&self.screen).engine.viewport()
    }

    /// Moves the viewport (`pane.scroll`). It stays inside the lines that the engine holds in
    /// memory; a line above them is only in the archive. A viewport at the end follows the output.
    /// Returns the new position. The frames of every window follow.
    pub fn scroll(&self, to: ScrollTo) -> nh_proto::frame::Viewport {
        let mut screen = lock(&self.screen);
        let now = screen.engine.viewport();
        let rows = u64::from(screen.engine.size().1);
        // The lines that left the screen. A viewport at or below this line shows the live screen.
        let scrolled = now.total_lines.saturating_sub(rows);
        let wanted = match to {
            ScrollTo::TopLine { top_line } => i128::from(top_line),
            ScrollTo::Delta { delta } => i128::from(now.top_line) + i128::from(delta),
        };
        let top =
            (wanted < i128::from(scrolled)).then(|| u64::try_from(wanted.max(0)).unwrap_or(0));
        let viewport = screen.engine.scroll_display(top);
        if viewport != now {
            task::publish(self, &mut screen, false);
        }
        viewport
    }

    /// Selects text (`pane.select`). The points are absolute lines, and the selection lives in
    /// the screen model, so every window of the pane shows it.
    pub fn select(&self, selection: Selection) {
        let mut screen = lock(&self.screen);
        screen.engine.set_selection(Some(selection));
        task::publish(self, &mut screen, false);
    }

    /// Clears the selection (`pane.select_clear`).
    pub fn select_clear(&self) {
        let mut screen = lock(&self.screen);
        screen.engine.set_selection(None);
        task::publish(self, &mut screen, false);
    }

    /// The selected text, if anything is selected.
    pub fn selection_text(&self) -> Option<String> {
        lock(&self.screen).engine.selection_text()
    }

    /// Writes the selection to the clipboard of the client that called (`pane.copy`). `None` is
    /// the host in the process. Returns the text. An error when nothing is selected.
    /// The text of the selection, written to the clipboard of the device when `write` is true. A
    /// phone writes its own clipboard in the tap, so the core writes none for it.
    pub fn copy(&self, device: Option<Ulid>, write: bool) -> Result<String, RpcError> {
        let text = self
            .selection_text()
            .filter(|text| !text.is_empty())
            .ok_or_else(|| {
                RpcError::new(
                    ErrorCode::NotFound,
                    format!("nothing is selected in pane {}", self.id),
                )
            })?;
        if write {
            self.write_clipboard(device, text.clone());
        }
        Ok(text)
    }

    /// A client sent input to the pane (`pane.input`, `pane.paste`, `pane.send_bytes`): an OSC 52
    /// write goes to its clipboard from now on.
    pub fn input_from_device(&self, device: Option<Ulid>) {
        if let Some(device) = device {
            *lock(&self.input_device) = Some(device);
        }
    }

    /// The client that sent the last input to the pane.
    pub fn input_device(&self) -> Option<Ulid> {
        *lock(&self.input_device)
    }

    /// Writes the clipboard of one client. When that client is not there, the text is dropped, and
    /// the `panes` projection shows `clipboard_dropped` until a later write arrives.
    fn write_clipboard(&self, device: Option<Ulid>, text: String) {
        let bytes = text.len() as u64;
        let written = self.host.clipboard_write_for(device, text);
        self.update_info(|info| {
            let dropped = (!written).then_some(ClipboardDropped {
                bytes,
                at: nh_proto::api::time::Timestamp(now_ms()),
            });
            if info.clipboard_dropped.is_none() && dropped.is_none() {
                return false;
            }
            info.clipboard_dropped = dropped;
            true
        });
    }

    /// The text of the rows that the viewport shows, without the blank rows at the end, and the
    /// absolute line of the first row (`pane.read` without a range).
    pub fn read_visible(&self) -> (String, u64) {
        let screen = lock(&self.screen);
        let top = screen.engine.viewport().top_line;
        let (_, rows) = screen.engine.size();
        let mut lines: Vec<String> = (0..rows).map(|y| row_text(&screen.engine, y)).collect();
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        (lines.join("\n"), top)
    }

    /// The lines of the live screen that a range names. It is empty while the viewport is
    /// scrolled back, because the engine then shows other rows.
    fn read_screen(&self, from: u64, to: u64) -> Vec<String> {
        let screen = lock(&self.screen);
        let viewport = screen.engine.viewport();
        let (_, rows) = screen.engine.size();
        let scrolled = viewport.total_lines.saturating_sub(u64::from(rows));
        if viewport.top_line != scrolled {
            return Vec::new();
        }
        (from.max(scrolled)..to.min(viewport.total_lines))
            .map(|line| {
                row_text(
                    &screen.engine,
                    u16::try_from(line - scrolled).unwrap_or(u16::MAX),
                )
            })
            .collect()
    }

    /// Asks the pane task to write the lines that wait to the archive, and waits for it.
    pub async fn flush_archive(&self) {
        let (done, answer) = oneshot::channel();
        if self.control.send(Control::Flush(done)).is_ok() {
            let _ = answer.await;
        }
    }

    /// Up to `count` lines of the archive from line `from` on. The archive has the lines up to
    /// the last flush, so a caller that needs the newest lines calls [`Pane::flush_archive`]
    /// first.
    pub async fn archived(
        &self,
        from: u64,
        count: usize,
    ) -> Result<Vec<nh_store::ArchivedLine>, RpcError> {
        let id = self.id;
        Ok(self
            .persist
            .run(move |store| store.read_lines(id, from, count))
            .await?)
    }

    /// Reads the lines `from..to` by absolute number (`pane.read {lines}`): the lines that
    /// left the screen from the archive, and the rows of the live screen. A soft wrap joins two
    /// lines. Returns the text and the number of the first line that it holds.
    pub async fn read_lines(&self, from: u64, to: u64) -> Result<(String, u64), RpcError> {
        self.flush_archive().await;
        let scrolled = {
            let screen = lock(&self.screen);
            screen
                .engine
                .viewport()
                .total_lines
                .saturating_sub(u64::from(screen.engine.size().1))
        };
        let archive_end = to.min(scrolled);
        let mut text = String::new();
        let mut first = None;
        if from < archive_end {
            let count = usize::try_from(archive_end - from).unwrap_or(usize::MAX);
            let lines = self.archived(from, count).await?;
            for line in lines {
                first.get_or_insert(line.line_no);
                text.push_str(&line.text);
                if !line.wrapped {
                    text.push('\n');
                }
            }
        }
        for line in self.read_screen(from, to) {
            first.get_or_insert(from.max(scrolled));
            text.push_str(&line);
            text.push('\n');
        }
        if text.ends_with('\n') {
            text.pop();
        }
        Ok((text, first.unwrap_or(from)))
    }

    /// A receiver for the process group in the foreground of the PTY. It is `None` until the
    /// holder reports one. The detection of agents reads it.
    pub fn foreground(&self) -> watch::Receiver<Option<i32>> {
        self.foreground.subscribe()
    }

    /// The program wrote to the clipboard with OSC 52. The first write of a pane waits
    /// for the answer of the user: the `panes` projection shows `clipboard_request`, and
    /// `pane.clipboard_allow` answers. A later write replaces the text that waits. A pane with a
    /// remembered answer follows it. When the last input came from a phone, an allowed write
    /// waits too, with `allowed`: the phone writes its clipboard in the tap on Copy.
    pub(crate) fn clipboard_store(&self, text: String) {
        if text.len() > MAX_CLIPBOARD {
            tracing::warn!(pane = %self.id, bytes = text.len(), "an OSC 52 write is too big");
            return;
        }
        let device = self.input_device();
        let phone = self.host.is_phone(device);
        let request = {
            let mut clipboard = lock(&self.clipboard);
            let allowed = match clipboard.policy {
                Some(true) if !phone => {
                    drop(clipboard);
                    self.write_clipboard(device, text);
                    return;
                }
                Some(true) => true,
                Some(false) => return,
                None => false,
            };
            let request = ClipboardRequest {
                bytes: text.len() as u64,
                preview: preview(&text),
                allowed,
            };
            clipboard.pending = Some(text);
            request
        };
        self.update_info(|info| {
            info.clipboard_request = Some(request);
            true
        });
    }

    /// The answer to the OSC 52 write that waits (`pane.clipboard_allow`). With `remember` the
    /// answer holds for later writes of the pane.
    /// The answer to the request of an OSC 52 write. With `allow`, the text goes to the clipboard
    /// of the client that typed last, or back to the caller when `write` is false (a phone).
    /// Returns the text that was allowed.
    pub async fn clipboard_answer(
        &self,
        allow: bool,
        remember: bool,
        write: bool,
    ) -> Result<Option<String>, RpcError> {
        let text = {
            let mut clipboard = lock(&self.clipboard);
            let text = clipboard.pending.take().ok_or_else(|| {
                RpcError::new(
                    ErrorCode::NotFound,
                    format!("no clipboard write waits in pane {}", self.id),
                )
            })?;
            if remember {
                clipboard.policy = Some(allow);
            }
            text
        };
        if remember {
            let id = self.id;
            let stored = self
                .persist
                .run(move |store| store.set_clipboard_policy(id, Some(allow), now_ms()))
                .await;
            if let Err(error) = stored {
                tracing::warn!(pane = %self.id, %error, "the clipboard answer is not stored");
            }
        }
        let allowed = allow.then_some(text);
        if write && let Some(text) = &allowed {
            self.write_clipboard(self.input_device(), text.clone());
        }
        self.update_info(|info| {
            info.clipboard_request = None;
            true
        });
        Ok(allowed)
    }

    /// The facts that the `panes` projection shows.
    pub fn info(&self) -> PaneInfo {
        lock(&self.info).clone()
    }

    /// The size of the grid, as `(cols, rows)`.
    pub fn size(&self) -> (u16, u16) {
        lock(&self.screen).engine.size()
    }

    /// The text of the visible rows, without the blank cells at the end of a row. The second cell
    /// of a wide character has no text of its own.
    pub fn text(&self) -> Vec<String> {
        let screen = lock(&self.screen);
        let (_, rows) = screen.engine.size();
        (0..rows).map(|y| row_text(&screen.engine, y)).collect()
    }

    /// What the screen shows now, and the journal offset after the last byte that it has read.
    /// Both are read under one lock, so they belong together.
    pub fn dump(&self) -> (nh_term::ScreenDump, u64) {
        let screen = lock(&self.screen);
        (
            nh_term::ScreenDump::of(&screen.engine, &xterm_palette()),
            screen.fed_offset,
        )
    }

    /// Some rows of the screen with their styles, for the screen detector. Only the
    /// rows in `rows` are copied.
    pub fn screen_rows(&self, rows: std::ops::RangeInclusive<u16>) -> nh_agents::screen::Screen {
        let screen = lock(&self.screen);
        let (_, total) = screen.engine.size();
        let first = *rows.start();
        let last = (*rows.end()).min(total.saturating_sub(1));
        let mut out = Vec::new();
        if first <= last {
            for y in first..=last {
                let mut cells = Vec::new();
                screen.engine.visit_row(y, &mut |view| {
                    cells.push(nh_agents::screen::Cell {
                        ch: view.ch,
                        style: view.style,
                    });
                });
                out.push(cells);
            }
        }
        let cursor = screen.engine.cursor();
        nh_agents::screen::Screen {
            total_rows: total,
            first_row: first,
            rows: out,
            cursor: (cursor.y, cursor.x),
            alt_screen: screen
                .engine
                .modes()
                .contains(nh_proto::modes::ModeFlags::ALT_SCREEN),
            title: screen.engine.title().map(str::to_owned),
        }
    }

    /// Starts a verified sequence of keys. The input of other clients waits until the
    /// guard ends. `None` when a sequence runs already.
    pub fn lock_keystrokes(self: &Arc<Self>) -> Option<KeystrokeLock> {
        let mut keys = lock(&self.keys);
        if keys.active.is_some() {
            return None;
        }
        keys.active = Some(KeySequence {
            since: Instant::now(),
            queue: Vec::new(),
            interrupted: false,
        });
        Some(KeystrokeLock {
            pane: Arc::clone(self),
        })
    }

    /// The journal offset after the last byte that the engine has read.
    pub fn fed_offset(&self) -> u64 {
        lock(&self.screen).fed_offset
    }

    /// The screen at the end of the replay and its journal offset, for a restored pane.
    #[cfg(feature = "debug")]
    pub fn restored_dump(&self) -> Option<(u64, nh_term::ScreenDump)> {
        lock(&self.restored_dump).clone()
    }

    /// The size that the last resize request asked for, as `(cols, rows)`.
    pub fn asked_size(&self) -> (u16, u16) {
        *lock(&self.asked_size)
    }

    /// Whether the pane has read its journal and the resize nudge is over. A pane that was not
    /// restored is settled from the start.
    pub fn settled(&self) -> bool {
        self.settled.load(Ordering::Acquire)
    }

    /// How long ago the engine last changed its size.
    pub fn since_resize(&self) -> std::time::Duration {
        lock(&self.screen).resized_at.elapsed()
    }

    /// Builds the next frame for a client.
    pub(crate) fn build_frame(&self, client: &mut ClientState, want: FrameKind) -> Frame {
        let mut screen = lock(&self.screen);
        let Screen { engine, frames, .. } = &mut *screen;
        frames.build(&*engine, client, want)
    }

    /// Encodes a logical input event for the modes of the pane now and sends the bytes to the
    /// holder (`pane.input`). Returns at once. [`Pane::acked_inputs`] lists the ack of the holder.
    pub fn input(&self, input_id: u64, input: &LogicalInput) -> Result<(), RpcError> {
        let mut bytes = Vec::new();
        let delivery = {
            let screen = lock(&self.screen);
            let modes = InputModes::new(screen.engine.modes());
            encode_input(input, &modes, &self.key_config, &mut bytes)
        };
        // A mouse event that the application did not ask for is for selection and scrolling. The
        // wheel scrolls the history. An empty encoding has nothing to write.
        if delivery == Delivery::Local {
            if let LogicalInput::Mouse(mouse) = input
                && mouse.kind == MouseKind::Wheel
                && mouse.wheel_y != 0
            {
                self.scroll(ScrollTo::Delta {
                    delta: i64::from(mouse.wheel_y) * WHEEL_LINES,
                });
            }
            return Ok(());
        }
        if bytes.is_empty() {
            return Ok(());
        }
        self.write(input_id, bytes)
    }

    /// Pastes text (`pane.paste`). The core applies bracketed paste when the
    /// application asked for it.
    pub fn paste(&self, input_id: u64, text: &str) -> Result<(), RpcError> {
        check_size(text.len())?;
        let mut bytes = Vec::with_capacity(text.len() + 16);
        encode_paste(text, &self.input_modes(), &mut bytes);
        self.write(input_id, bytes)
    }

    /// Writes raw bytes, the explicit exception to logical input (`pane.send_bytes`).
    pub fn send_bytes(&self, input_id: u64, bytes: Vec<u8>) -> Result<(), RpcError> {
        check_size(bytes.len())?;
        if bytes.is_empty() {
            return Ok(());
        }
        self.write(input_id, bytes)
    }

    /// Pastes the quoted paths of files that the user dropped on the pane (`pane.drop_files`).
    pub fn drop_files(&self, input_id: u64, paths: &[String]) -> Result<(), RpcError> {
        if paths.iter().any(|path| path.contains('\0')) {
            return Err(RpcError::invalid_params("a path has a NUL character"));
        }
        check_size(paths.iter().map(|path| path.len() + 4).sum())?;
        let mut bytes = Vec::new();
        encode_drop_files(paths, &self.input_modes(), &mut bytes);
        if bytes.is_empty() {
            return Ok(());
        }
        self.write(input_id, bytes)
    }

    /// The modes of the application now, for the encoders.
    fn input_modes(&self) -> InputModes {
        InputModes::new(lock(&self.screen).engine.modes())
    }

    /// Sends encoded bytes to the holder with the id of the input, and follows the ack. While a
    /// verified sequence of keys runs, the input waits in a queue for at most two seconds. An
    /// input that would wait longer goes through, and ends the sequence.
    fn write(&self, input_id: u64, bytes: Vec<u8>) -> Result<(), RpcError> {
        {
            let mut keys = lock(&self.keys);
            if let Some(sequence) = keys.active.as_mut() {
                if sequence.since.elapsed() < KEYS_QUEUE_MAX {
                    sequence.queue.push((input_id, bytes));
                    return Ok(());
                }
                sequence.interrupted = true;
            }
        }
        self.write_now(input_id, bytes)
    }

    /// Like [`Pane::write`], and nothing waits.
    fn write_now(&self, input_id: u64, bytes: Vec<u8>) -> Result<(), RpcError> {
        if lock(&self.inputs).sent(input_id, Instant::now()) {
            // The task sleeps until its next deadline. This input has the first one.
            let _ = self.control.send(Control::Wake);
        }
        self.tx
            .send(HolderRequest::Input { input_id, bytes })
            .map_err(|_| {
                lock(&self.inputs).unsent(input_id);
                RpcError::new(
                    ErrorCode::PaneGone,
                    format!("the holder of pane {} is gone", self.id),
                )
            })?;
        Ok(())
    }

    /// The input ids that the holder acknowledged with a sequence number of `since` or more, and
    /// the number to ask for next time. A client that keeps the number misses no id.
    pub fn acked_inputs(&self, since: u64) -> (Vec<u64>, u64) {
        lock(&self.inputs).acked_since(since)
    }

    /// How many inputs wait for the ack of the holder.
    pub fn pending_inputs(&self) -> usize {
        lock(&self.inputs).pending_len()
    }

    /// Asks the holder to resize the PTY (`pane.resize`). The engine changes size when the holder
    /// reports the resize marker, so that the output before the resize is drawn at the old size.
    /// A request for the size that was asked last does nothing.
    pub fn resize(
        &self,
        cols: u16,
        rows: u16,
        px: (u16, u16),
        cell_px: Option<(u16, u16)>,
    ) -> Result<(), RpcError> {
        let cols = cols.clamp(2, MAX_DIMENSION);
        let rows = rows.clamp(1, MAX_DIMENSION);
        if let Some((w, h)) = cell_px {
            lock(&self.screen).engine.set_cell_size(w, h);
        }
        {
            let mut asked = lock(&self.asked_size);
            if *asked == (cols, rows) {
                return Ok(());
            }
            *asked = (cols, rows);
        }
        self.tx
            .send(HolderRequest::Resize {
                cols,
                rows,
                px_width: px.0,
                px_height: px.1,
            })
            .map_err(|_| {
                RpcError::new(
                    ErrorCode::PaneGone,
                    format!("the holder of pane {} is gone", self.id),
                )
            })
    }

    /// A window reports the size of its surface for this pane (`pane.resize`). The PTY follows the
    /// size of the window that owns the lease. The first window that reports a size owns it. A
    /// window that does not own it only has its size remembered, for the time it takes the lease.
    pub fn resize_for(&self, window: WindowId, cols: u16, rows: u16) -> Result<(), RpcError> {
        let (cols, rows) = (cols.clamp(2, MAX_DIMENSION), rows.clamp(1, MAX_DIMENSION));
        let (follows, changed) = {
            let mut lease = lock(&self.lease);
            lease.wanted.insert(window, (cols, rows));
            let changed = lease.owner.is_none();
            if changed {
                lease.owner = Some(window);
                lease.since = Some(Instant::now());
            }
            (lease.owner == Some(window), changed)
        };
        if changed {
            self.show_lease(Some(window));
        }
        if follows {
            self.resize_pty(cols, rows)?;
        }
        Ok(())
    }

    /// A window sent input to the pane. It takes the lease from another window, unless the lease
    /// changed less than half a second ago. The PTY then gets the size that the window asked for.
    pub fn input_from(&self, window: WindowId) {
        // A person typing during a verified sequence ends it.
        if let Some(sequence) = lock(&self.keys).active.as_mut() {
            sequence.interrupted = true;
        }
        let size = {
            let mut lease = lock(&self.lease);
            if lease.owner == Some(window)
                || lease
                    .since
                    .is_some_and(|since| since.elapsed() < LEASE_DEBOUNCE)
            {
                return;
            }
            lease.owner = Some(window);
            lease.since = Some(Instant::now());
            lease.wanted.get(&window).copied()
        };
        self.show_lease(Some(window));
        if let Some((cols, rows)) = size
            && let Err(error) = self.resize_pty(cols, rows)
        {
            tracing::debug!(pane = %self.id, %error, "the new owner cannot size the pane");
        }
    }

    /// The window does not draw the pane any more. It gives up its size, and its lease if it had
    /// one. The next window that sizes or types takes the lease.
    pub fn release_lease(&self, window: WindowId) {
        let released = {
            let mut lease = lock(&self.lease);
            lease.wanted.remove(&window);
            let owned = lease.owner == Some(window);
            if owned {
                lease.owner = None;
                lease.since = None;
            }
            owned
        };
        if released {
            self.show_lease(None);
        }
    }

    /// Puts the owner of the lease in the `panes` row.
    fn show_lease(&self, owner: Option<WindowId>) {
        self.update_info(|info| {
            info.geometry_owner = owner;
            true
        });
    }

    /// Asks the PTY for a size, with the pixel size that the windows measured.
    fn resize_pty(&self, cols: u16, rows: u16) -> Result<(), RpcError> {
        let (cell_w, cell_h) = *lock(&self.cell_px);
        let px = (cols.saturating_mul(cell_w), rows.saturating_mul(cell_h));
        self.resize(cols, rows, px, None)
    }

    /// A window measured the size of a cell, in device pixels. The engine answers `CSI 14 t` with
    /// it, and the PTY reports it.
    pub fn set_cell_px(&self, width: u16, height: u16) {
        if width == 0 || height == 0 {
            return;
        }
        *lock(&self.cell_px) = (width, height);
        lock(&self.screen).engine.set_cell_size(width, height);
    }

    /// Asks the child to end, as closing a terminal does: `Term` to the foreground group, and
    /// `Hup` to the child (a shell ignores `Term` and obeys `Hup`). The pane task then tells the
    /// holder to exit.
    pub fn terminate(&self) {
        use nh_proto::holder::{SignalKind, SignalTarget};
        for (target, signal) in [
            (SignalTarget::Foreground, SignalKind::Term),
            (SignalTarget::Child, SignalKind::Hup),
        ] {
            let _ = self.tx.send(HolderRequest::Signal { target, signal });
        }
    }

    /// Kills the child. The pane task then tells the holder to exit.
    pub fn kill(&self) {
        use nh_proto::holder::{SignalKind, SignalTarget};
        for target in [SignalTarget::Foreground, SignalTarget::Child] {
            let _ = self.tx.send(HolderRequest::Signal {
                target,
                signal: SignalKind::Kill,
            });
        }
    }

    /// Kills the child and every process that it started: the jobs of its session, the programs
    /// that left the session, and their descendants. A pane that closes ends so. The pane
    /// task then tells the holder to exit.
    ///
    /// The walk of the processes blocks for a few milliseconds, so a task of the runtime calls it
    /// through `spawn_blocking`.
    pub fn kill_all(&self) {
        let killed = family::kill_family(self.child_pid, self.holder_pid);
        tracing::debug!(pane = %self.id, killed, "the processes of the pane are killed");
        self.kill();
    }

    /// Makes a checkpoint at the next cut point and ends the lease on the holder, so that the next
    /// core starts from this screen ("Quit"). The child keeps running. Waits at most
    /// [`Tuning::quit_wait`] for the cut point. Returns whether the checkpoint is stored.
    ///
    /// The pane is not usable afterwards. Call it when the core ends.
    pub async fn quit(&self) -> bool {
        let (done, answer) = oneshot::channel();
        if self.control.send(Control::Quit(done)).is_err() {
            return false;
        }
        answer.await.unwrap_or(false)
    }

    fn row_of(id: Ulid, render_id: u32, info: &PaneInfo) -> Value {
        let (liveness, exit) = match info.liveness {
            Liveness::Running => ("running", Value::Null),
            Liveness::Exited(ExitStatus::Code(c)) => ("exited", json!({ "code": c })),
            Liveness::Exited(ExitStatus::Signal(s)) => ("exited", json!({ "signal": s })),
            Liveness::Exited(ExitStatus::ExecFailed(e)) => ("exited", json!({ "exec_failed": e })),
            Liveness::HolderLost => ("lost", Value::Null),
            Liveness::Failed => ("failed", Value::Null),
        };
        let unconfirmed: Vec<Value> = info
            .input_unconfirmed
            .iter()
            .map(|u| json!({ "input_id": u.input_id, "reason": u.reason.as_str() }))
            .collect();
        json!({
            "id": id.to_string(),
            "render_id": render_id,
            "title": info.title,
            "cols": info.cols,
            "rows": info.rows,
            "liveness": liveness,
            "exit": exit,
            "restoring": info.restoring,
            "recovery": info.recovery.as_str(),
            "input_unconfirmed": unconfirmed,
        })
    }

    /// Updates the facts of the pane. The closure returns whether it changed something. Then the
    /// row of the `panes` projection is published.
    pub(crate) fn update_info(&self, change: impl FnOnce(&mut PaneInfo) -> bool) {
        let (changed, info) = {
            let mut info = lock(&self.info);
            (change(&mut info), info.clone())
        };
        if changed {
            self.publish(&info);
        }
    }

    /// Ends what this pane object shows in the layout and the log. The pane object of a restart
    /// calls it on the one that it replaces.
    pub(crate) fn retire(&self) {
        self.retired.store(true, Ordering::Release);
    }

    /// Shows the facts of the pane: through the state actor for a pane of the layout, else in the
    /// row that the pane writes itself.
    fn publish(&self, info: &PaneInfo) {
        if self.retired.load(Ordering::Acquire) {
            return;
        }
        match &self.layout {
            Some(actor) => {
                let (id, report) = (PaneId::new(self.id), report_of(info));
                actor.update_detached(move |parts| {
                    crate::actor::layout::apply_report(parts, id, report)
                });
            }
            None => self.projections.set_pane(
                &self.id.to_string(),
                Self::row_of(self.id, self.render_id, info),
            ),
        }
    }

    /// Appends an event of the pane to the log. A pane that is not in the layout (a pane of
    /// `debug.spawn_panes` or of a test) has none.
    fn emit(&self, events: Vec<EventPayload>) {
        let Some(actor) = &self.layout else {
            return;
        };
        if self.retired.load(Ordering::Acquire) {
            return;
        }
        let subject = self.id.to_string();
        actor.update_detached(move |mut parts| {
            let stored = parts.commit(|tx| {
                for payload in &events {
                    tx.append(Some(&subject), payload)?;
                }
                Ok(())
            });
            if let Err(error) = stored {
                tracing::warn!(pane = %subject, %error, "a pane event is not stored");
            }
        });
    }

    /// The program asked for a notification with OSC 9 or OSC 777. It is a note from the terminal,
    /// not an interaction of an agent, so the host shows it and nothing waits for an answer.
    pub(crate) fn notify(&self, title: String, body: String) {
        let title = if title.trim().is_empty() {
            lock(&self.info)
                .title
                .clone()
                .unwrap_or_else(|| "neohar".to_owned())
        } else {
            title
        };
        self.host.notify(HostNotification {
            title,
            body,
            urgency: Urgency::Low,
            interaction: None,
            pane: None,
            actions: Vec::new(),
        });
    }

    /// The program rang the bell: an event for the notification logic and the marks of the UI.
    pub(crate) fn bell(&self) {
        self.emit(vec![EventPayload::PaneBell(PaneBell {
            pane: PaneId::new(self.id),
        })]);
    }

    /// Reports inputs as unconfirmed.
    pub(crate) fn report_unconfirmed(&self, ids: &[u64], reason: UnconfirmedReason) {
        if ids.is_empty() {
            return;
        }
        self.emit(
            ids.iter()
                .map(|&input_id| {
                    EventPayload::PaneInputUnconfirmed(PaneInputUnconfirmed {
                        pane: PaneId::new(self.id),
                        input_id,
                        reason: reason.event_reason(),
                    })
                })
                .collect(),
        );
        tracing::warn!(pane = %self.id, count = ids.len(), reason = reason.as_str(), "inputs are unconfirmed");
        self.update_info(|info| {
            for &input_id in ids {
                if info.input_unconfirmed.len() == UNCONFIRMED_SHOWN {
                    info.input_unconfirmed.remove(0);
                }
                info.input_unconfirmed
                    .push(Unconfirmed { input_id, reason });
            }
            true
        });
    }
}

/// The first characters of an OSC 52 write, for the card that asks about it. Control characters
/// become spaces, so that the program cannot draw in the card.
fn preview(text: &str) -> String {
    text.chars()
        .take(80)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// What the layout is told about a pane.
fn report_of(info: &PaneInfo) -> Report {
    let runtime = match info.liveness {
        Liveness::Running => LayoutRuntime::Running,
        Liveness::Exited(ExitStatus::Code(code)) => LayoutRuntime::Exited(Some(code)),
        Liveness::Exited(_) => LayoutRuntime::Exited(None),
        Liveness::HolderLost | Liveness::Failed => LayoutRuntime::Lost,
    };
    Report {
        runtime,
        cols: info.cols,
        rows: info.rows,
        facts: Facts {
            title: info.title.clone(),
            restoring: info.restoring,
            input_unconfirmed: info.input_unconfirmed.iter().map(|u| u.input_id).collect(),
            clipboard_request: info.clipboard_request.clone(),
            clipboard_dropped: info.clipboard_dropped,
            geometry_owner: info.geometry_owner,
            cwd: info.cwd.clone(),
        },
    }
}

/// What a new pane is made of. A pane that starts and a pane that a restart brings back both come
/// through [`PaneFactory::assemble`].
pub(crate) struct PaneParts {
    pub id: Ulid,
    pub render_id: u32,
    pub holder_pid: i32,
    pub child_pid: i32,
    pub tx: HolderTx,
    pub engine: AlacrittyEngine,
    /// The journal offset after the last byte that `engine` has read.
    pub fed_offset: u64,
    /// The size that the PTY has now. It is the size of the engine, except in a pane that is
    /// still reading the journal after a restart.
    pub asked_size: (u16, u16),
    pub restoring: bool,
    pub recovery: Recovery,
    pub liveness: Liveness,
}

/// How a new holder gets into the row of its pane.
pub(crate) enum Registration {
    /// A new row, for a pane that is not in the layout.
    Insert,
    /// The row of a pane of the layout gets the holder and the token.
    Attach { token: String },
}

/// What every pane needs from the core. The runtime builds it once.
pub(crate) struct PaneFactory {
    pub session: String,
    pub paths: Env,
    pub nh_path: PathBuf,
    /// The environment that panes start from.
    pub base_env: Vec<(String, String)>,
    /// The directory of the shims. A pane gets it first in its `PATH` when `agents.shims` is on.
    pub shims_dir: Option<PathBuf>,
    /// The fencing token of this core. It is above the start time in milliseconds and above every
    /// token in the database, so a core that starts later has a higher one.
    pub fencing_token: u64,
    pub projections: Arc<Projections>,
    pub runtime: Handle,
    pub persist: Arc<Persist>,
    pub tuning: Tuning,
    /// Hears which protocol each holder speaks.
    pub health: Arc<crate::health::Health>,
    /// The host router of the core.
    pub host: Arc<HostRouter>,
    /// The state actor: the panes of the layout report to it.
    pub actor: ActorHandle,
    /// The panes that belong to the layout. A pane here reports through the actor. A pane stays in
    /// the set after it closes, so that its last reports find no pane and change nothing.
    pub bound: Mutex<std::collections::HashSet<Ulid>>,
}

impl PaneFactory {
    /// Starts a holder with the program of `spec` and the pane that reads it. The pane is not in
    /// the layout (`debug.spawn_panes`, a test), and it writes its own row.
    pub(crate) async fn spawn(&self, spec: PaneSpec) -> Result<Arc<Pane>, CoreError> {
        let mut random = [0u8; 10];
        getrandom::fill(&mut random).map_err(CoreError::Random)?;
        let mut tail = [0u8; 16];
        tail[6..].copy_from_slice(&random);
        let ms = u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis()),
        )
        .unwrap_or(0);
        let id = Ulid::from_parts(ms, u128::from_be_bytes(tail));
        // The numbers come from the counter of the layout, so that no two panes share one.
        let render_id = self
            .actor
            .update(|mut parts| parts.commit(|tx| tx.next_handle("render")))
            .await?
            .map_err(CoreError::from)?;
        self.spawn_with(id, render_id, spec, Registration::Insert)
            .await
    }

    /// Starts the holder of a pane that the layout made. The row of the pane exists. The holder is
    /// added to it, with the token of the pane.
    pub(crate) async fn spawn_slot(
        &self,
        id: Ulid,
        render_id: u32,
        spec: PaneSpec,
        token: String,
    ) -> Result<Arc<Pane>, CoreError> {
        lock(&self.bound).insert(id);
        self.spawn_with(id, render_id, spec, Registration::Attach { token })
            .await
    }

    async fn spawn_with(
        &self,
        id: Ulid,
        render_id: u32,
        spec: PaneSpec,
        registration: Registration,
    ) -> Result<Arc<Pane>, CoreError> {
        let mut secret = [0u8; 32];
        getrandom::fill(&mut secret).map_err(CoreError::Random)?;

        self.paths.ensure_runtime_root()?;
        let socket = self.paths.holder_socket(&self.session, id)?;
        clear_stale_socket(&socket)?;
        let logs = self.paths.data_dir(&self.session)?.join("logs");
        make_private_dir(&logs)?;
        let cwd = spec
            .cwd
            .clone()
            .or_else(|| self.home())
            .unwrap_or_else(|| "/".to_owned());
        let (cols, rows) = (
            spec.cols.clamp(2, MAX_DIMENSION),
            spec.rows.clamp(1, MAX_DIMENSION),
        );
        let config = HoldConfig {
            proto: HOLDER_PROTO,
            pane: id.to_bytes(),
            secret,
            mode: HoldMode::Pty {
                cols,
                rows,
                px_width: 0,
                px_height: 0,
            },
            argv: spec.argv.clone(),
            env: child_env(&self.base_env, &spec.env),
            cwd: cwd.clone(),
            socket_path: socket.display().to_string(),
            log_path: logs.join(format!("holder-{id}.log")).display().to_string(),
            ring_bytes: self.tuning.ring_bytes,
            checkpoint_idle_ms: millis(self.tuning.checkpoint_idle),
            checkpoint_max_interval_ms: millis(self.tuning.checkpoint_interval),
            query_fresh_ms: millis(self.tuning.query_fresh),
        };
        let holder_pid = spawn_holder(&self.nh_path, &config).await?;
        tokio::time::sleep(self.tuning.attach_delay).await;
        let mut link = match holder::connect(&socket, secret, self.fencing_token, "nh-core").await {
            Ok(link) => link,
            Err(error) => {
                // Nobody can reach this holder, and nobody will ever. It would run for ever.
                kill_holder(holder_pid);
                return Err(error.into());
            }
        };
        self.health.holder_protocol(link.hello.proto);
        let fresh_ms = i64::try_from(self.tuning.query_fresh.as_millis()).unwrap_or(i64::MAX);
        let (replay, backlog) = match restore::pre_attach_replay(&mut link, fresh_ms).await {
            Ok(found) => found,
            Err(error) => {
                kill_holder(holder_pid);
                return Err(error.into());
            }
        };
        if let Err(error) = link.tx.send(HolderRequest::Attach { from_offset: 0 }) {
            kill_holder(holder_pid);
            return Err(error.into());
        }
        let record = PaneRecord {
            id,
            render_id,
            socket: socket.display().to_string(),
            holder_pid,
            secret,
            fencing_token: self.fencing_token,
            argv: spec.argv,
            cwd,
            cols,
            rows,
            liveness: PaneLiveness::Running,
            exit: None,
        };
        let registered = self
            .persist
            .run(move |store| {
                let now = now_ms();
                match registration {
                    Registration::Insert => store.insert_pane(&record, now).map(|()| true),
                    Registration::Attach { token } => {
                        store.attach_holder(&record, Some(&token), now)
                    }
                }
            })
            .await;
        match registered {
            Ok(true) => {}
            // The layout closed the pane while the holder started, and its row is gone.
            Ok(false) => {
                kill_holder(holder_pid);
                return Err(CoreError::PaneGone(id));
            }
            Err(error) => {
                // A pane that the database does not know cannot come back after a restart.
                kill_holder(holder_pid);
                return Err(error);
            }
        }

        let engine = AlacrittyEngine::new(cols, rows, MIN_HISTORY, xterm_palette());
        let (pane, control) = self.assemble(PaneParts {
            id,
            render_id,
            holder_pid,
            child_pid: link.hello.child_pid,
            tx: link.tx,
            engine,
            fed_offset: 0,
            asked_size: (cols, rows),
            restoring: false,
            recovery: Recovery::Fresh,
            liveness: Liveness::Running,
        });
        self.start_task(
            &pane,
            link.events,
            control,
            task::Init {
                backlog,
                replay,
                last_checkpoint: 0,
                restore: None,
                at_cut: true,
                nudge: false,
            },
        );
        Ok(pane)
    }

    /// Makes the pane and publishes its row. The task is not running yet.
    pub(in crate::pane) fn assemble(
        &self,
        parts: PaneParts,
    ) -> (Arc<Pane>, mpsc::UnboundedReceiver<Control>) {
        let PaneParts {
            id,
            render_id,
            holder_pid,
            child_pid,
            tx,
            engine,
            fed_offset,
            asked_size,
            restoring,
            recovery,
            liveness,
        } = parts;
        let (cols, rows) = engine.size();
        let bound = lock(&self.bound).contains(&id);
        let frames = FrameBuilder::new(render_id, &engine);
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let pane = Arc::new(Pane {
            id,
            render_id,
            holder_pid,
            child_pid,
            tx,
            screen: Mutex::new(Screen {
                engine,
                frames,
                fed_offset,
                resized_at: Instant::now(),
            }),
            rev: AtomicU64::new(1),
            non_minor_rev: AtomicU64::new(1),
            damage: watch::channel(1).0,
            info: Mutex::new(PaneInfo {
                title: None,
                cols,
                rows,
                liveness,
                restoring,
                recovery,
                input_unconfirmed: Vec::new(),
                clipboard_request: None,
                clipboard_dropped: None,
                geometry_owner: None,
                cwd: None,
            }),
            asked_size: Mutex::new(asked_size),
            inputs: Mutex::new(Inputs::default()),
            #[cfg(feature = "debug")]
            restored_dump: Mutex::new(None),
            settled: AtomicBool::new(!restoring),
            retired: AtomicBool::new(false),
            // The holder keeps the last 4,096 input ids across the lifetimes of cores and drops
            // a repeated id. So the ids of the replies of one core must differ from the
            // ids of the next core: each core counts from its own fencing token.
            internal_ids: AtomicU64::new(self.fencing_token.wrapping_shl(20)),
            key_config: KeyConfig::default(),
            projections: Arc::clone(&self.projections),
            host: Arc::clone(&self.host),
            input_device: Mutex::new(None),
            persist: Arc::clone(&self.persist),
            clipboard: Mutex::new(Clipboard::default()),
            lease: Mutex::new(Lease::default()),
            keys: Mutex::new(KeyLock::default()),
            cell_px: Mutex::new((0, 0)),
            foreground: watch::channel(None).0,
            layout: bound.then(|| self.actor.clone()),
            control: control_tx,
        });
        pane.publish(&lock(&pane.info).clone());
        (pane, control_rx)
    }

    /// Runs the pane task and a supervisor that reports a panic (a panic never takes the core
    /// down, and it is never silent).
    pub(in crate::pane) fn start_task(
        &self,
        pane: &Arc<Pane>,
        events: mpsc::Receiver<nh_proto::holder::HolderEvent>,
        control: mpsc::UnboundedReceiver<Control>,
        init: task::Init,
    ) {
        let task = self.runtime.spawn(task::run(
            Arc::clone(pane),
            events,
            control,
            init,
            Arc::clone(&self.persist),
            self.tuning.clone(),
        ));
        let pane = Arc::clone(pane);
        self.runtime.spawn(async move {
            if let Err(error) = task.await
                && error.is_panic()
            {
                tracing::error!(pane = %pane.id, "the pane task panicked");
                pane.update_info(|info| {
                    info.liveness = Liveness::Failed;
                    info.restoring = false;
                    true
                });
            }
        });
    }

    fn home(&self) -> Option<String> {
        self.base_env
            .iter()
            .find(|(k, _)| k == "HOME")
            .map(|(_, v)| v.clone())
    }
}

/// A duration as the milliseconds that `HoldConfig` carries.
fn millis(duration: std::time::Duration) -> u32 {
    u32::try_from(duration.as_millis()).unwrap_or(u32::MAX)
}

/// The environment of the program in a pane: the host's base, the terminal variables, then the
/// variables of the spec.
fn child_env(base: &[(String, String)], extra: &[(String, String)]) -> Vec<(String, String)> {
    let mut env = base.to_vec();
    let mut set = |key: &str, value: &str| match env.iter_mut().find(|(k, _)| k == key) {
        Some(entry) => entry.1 = value.to_owned(),
        None => env.push((key.to_owned(), value.to_owned())),
    };
    set("TERM", "xterm-256color");
    set("COLORTERM", "truecolor");
    set("TERM_PROGRAM", "neohar");
    for (key, value) in extra {
        set(key, value);
    }
    env
}

/// Removes the socket file that a holder left when it died without a clean exit. A pane that
/// starts again uses the path of its last holder, and a holder cannot bind a path that exists. If
/// a holder still answers at the path, something is wrong and nothing is removed.
fn clear_stale_socket(socket: &std::path::Path) -> Result<(), CoreError> {
    use std::os::unix::net::UnixStream;
    if !socket.exists() {
        return Ok(());
    }
    if UnixStream::connect(socket).is_ok() {
        return Err(CoreError::Io {
            path: socket.to_owned(),
            source: std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "a holder still answers at this path",
            ),
        });
    }
    std::fs::remove_file(socket).map_err(|source| CoreError::Io {
        path: socket.to_owned(),
        source,
    })
}

/// Kills a holder that the core started and could not connect to.
pub(crate) fn kill_holder(pid: i32) {
    use rustix::process::{Pid, Signal, kill_process};
    if let Some(pid) = Pid::from_raw(pid) {
        let _ = kill_process(pid, Signal::KILL);
    }
}

/// Creates a directory tree with mode 0700.
pub(crate) fn make_private_dir(path: &std::path::Path) -> Result<(), CoreError> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(|source| CoreError::Io {
            path: path.to_owned(),
            source,
        })
}

/// The text of row `y` of the viewport, without the blank cells at the end. The second cell of a
/// wide character has no text of its own.
fn row_text<E: VtEngine + ?Sized>(engine: &E, y: u16) -> String {
    let mut line = String::new();
    engine.visit_row(y, &mut |cell| {
        if cell.style.width == CellWidth::Spacer {
            return;
        }
        line.push(cell.ch);
        line.extend(cell.zerowidth);
    });
    line.trim_end().to_owned()
}

/// The right to type in a pane for a verified sequence of keys. Dropping it gives back the input
/// that waited.
pub struct KeystrokeLock {
    pane: Arc<Pane>,
}

impl KeystrokeLock {
    fn next_id(&self) -> u64 {
        INTERNAL_INPUT | self.pane.internal_ids.fetch_add(1, Ordering::Relaxed)
    }

    /// Whether a person typed, or other input waited too long. No key may follow.
    pub fn interrupted(&self) -> bool {
        lock(&self.pane.keys)
            .active
            .as_ref()
            .is_none_or(|sequence| sequence.interrupted)
    }

    /// Types a key as an input method does, for the modes of the pane now.
    pub fn send(&self, key: nh_agents::keystroke::Key) -> Result<(), RpcError> {
        use nh_agents::keystroke::Key;
        let (code, name, text): (String, String, Option<String>) = match key {
            Key::Up => ("ArrowUp".into(), "ArrowUp".into(), None),
            Key::Down => ("ArrowDown".into(), "ArrowDown".into(), None),
            Key::Enter => ("Enter".into(), "Enter".into(), None),
            Key::Space => ("Space".into(), " ".into(), Some(" ".into())),
            Key::Char(c) => (
                if c.is_ascii_digit() {
                    format!("Digit{c}")
                } else {
                    format!("Key{}", c.to_ascii_uppercase())
                },
                c.to_string(),
                Some(c.to_string()),
            ),
        };
        self.send_event(&LogicalInput::Key(nh_proto::input::LogicalKey {
            code,
            key: name,
            mods: nh_proto::input::Mods::default(),
            kind: nh_proto::input::KeyKind::default(),
            text,
            location: 0,
        }))
    }

    /// Types an input event, encoded for the modes of the pane now.
    pub fn send_event(&self, event: &LogicalInput) -> Result<(), RpcError> {
        let mut bytes = Vec::new();
        {
            let screen = lock(&self.pane.screen);
            let modes = InputModes::new(screen.engine.modes());
            encode_input(event, &modes, &self.pane.key_config, &mut bytes);
        }
        if bytes.is_empty() {
            return Ok(());
        }
        self.pane.write_now(self.next_id(), bytes)
    }

    /// Pastes text (bracketed when the application asked for it).
    pub fn paste(&self, text: &str) -> Result<(), RpcError> {
        self.paste_as(self.next_id(), text)
    }

    /// Pastes text with the id of an input that a client gave, so that the holder drops a repeat
    /// of it (`agent.reply`).
    pub fn paste_as(&self, input_id: u64, text: &str) -> Result<(), RpcError> {
        check_size(text.len())?;
        let mut bytes = Vec::with_capacity(text.len() + 16);
        encode_paste(text, &self.pane.input_modes(), &mut bytes);
        self.pane.write_now(input_id, bytes)
    }
}

impl Drop for KeystrokeLock {
    fn drop(&mut self) {
        let queue = lock(&self.pane.keys)
            .active
            .take()
            .map(|sequence| sequence.queue)
            .unwrap_or_default();
        for (input_id, bytes) in queue {
            let _ = self.pane.write_now(input_id, bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_child_gets_terminal_variables_over_the_base_and_the_spec_wins() {
        let base = vec![
            ("HOME".to_owned(), "/home/ada".to_owned()),
            ("TERM".to_owned(), "dumb".to_owned()),
        ];
        let extra = vec![("COLORTERM".to_owned(), "24bit".to_owned())];
        let env = child_env(&base, &extra);
        let get = |k: &str| {
            env.iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("HOME"), Some("/home/ada"));
        assert_eq!(get("TERM"), Some("xterm-256color"));
        assert_eq!(get("COLORTERM"), Some("24bit"));
        assert_eq!(get("TERM_PROGRAM"), Some("neohar"));
        assert_eq!(env.iter().filter(|(k, _)| k == "TERM").count(), 1);
    }

    #[test]
    fn a_duration_larger_than_the_wire_field_is_capped() {
        assert_eq!(millis(std::time::Duration::from_secs(2)), 2000);
        assert_eq!(
            millis(std::time::Duration::from_secs(u64::MAX / 2)),
            u32::MAX
        );
    }
}
