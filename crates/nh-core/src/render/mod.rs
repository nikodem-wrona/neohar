//! The render server: frames from panes to windows, at the pace of the
//! window.
//!
//! A window has one render stream and a set of panes it shows. For each pane the window has a
//! client state (what the webview has seen) from `nh-term`, so any number of windows can follow
//! one pane at their own pace. A pacer task per window wakes on damage, waits until the next tick
//! of the window's refresh rate, and builds one frame for each pane that changed. It never builds
//! more than [`MAX_IN_FLIGHT`] unacknowledged frames for a pane, and it builds each frame against
//! the previous frame's revision, not the last acknowledged one. A slow webview therefore gets
//! fewer, larger frames and never a queue.

mod pacer;
mod peek;
mod scrollback;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex, Weak},
};

use nh_term::ClientState;
use serde::Deserialize;
use tokio::{runtime::Handle, sync::Notify, task::JoinHandle};
use ulid::Ulid;

use nh_proto::api::ids::WindowId;

use crate::{
    ErrorCode,
    error::RpcError,
    layout::model::Layout,
    pane::{Pane, lock},
};

/// Where the frames of a render stream go. The host implements it with a Tauri channel
/// of raw bodies, or a WebSocket.
pub trait FrameSink: Send + Sync + 'static {
    /// Sends one encoded frame (`postcard`, `nh_proto::frame::Frame`). Returns `false` when the
    /// receiver is gone. The stream then ends.
    fn send(&self, frame: Vec<u8>) -> bool;

    /// Whether the receiver is gone. A stream that has nothing to send checks it now and then, so
    /// that it ends when nobody listens. A sink that cannot tell says `false`.
    fn is_closed(&self) -> bool {
        false
    }

    /// The round-trip time of the link that this sink sends over, if it has one and measured it.
    /// A sink of this machine says `None`.
    fn round_trip(&self) -> Option<std::time::Duration> {
        None
    }
}

/// The most frames in flight for a pane in a window: built, sent, and not acknowledged.
pub const MAX_IN_FLIGHT: usize = 2;

/// The most frames in flight for a pane of a window that is reached over a link.
pub const MAX_IN_FLIGHT_LINK: usize = 8;

/// How many unacknowledged frames a pane of a window may have. A window of this machine has
/// [`MAX_IN_FLIGHT`]. Over a link the limit is `max(2, ceil(rtt / period) + 1)`, at most
/// [`MAX_IN_FLIGHT_LINK`], so that a round trip of 50 ms does not hold the rate of a display at
/// 40 frames a second. `period` is the display period of the window.
pub fn in_flight_limit(rtt: Option<std::time::Duration>, refresh_hz: f64) -> usize {
    let Some(rtt) = rtt else {
        return MAX_IN_FLIGHT;
    };
    let hz = if refresh_hz.is_finite() && refresh_hz > 0.0 {
        refresh_hz
    } else {
        60.0
    };
    // rtt / period, with a margin so that 50 ms at 120 Hz is 6 periods and not 7.
    let periods = (rtt.as_secs_f64() * hz - 1e-6).ceil().clamp(0.0, 16.0);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 0..=16 just above"
    )]
    let wanted = periods as usize + 1;
    wanted.clamp(MAX_IN_FLIGHT, MAX_IN_FLIGHT_LINK)
}

/// The refresh rate until the window reports its own.
const DEFAULT_HZ: f64 = 60.0;

/// The parameters of `stream("render", …)`. The window is a window of the layout.
#[derive(Debug, Deserialize)]
pub struct RenderParams {
    pub window: WindowId,
    /// The refresh rate of the display. `window.focus_report` updates it.
    #[serde(default)]
    pub viewport_hz: Option<f64>,
    /// The version of the frame format that the client decodes.
    #[serde(default)]
    pub proto: Option<u16>,
}

/// One acknowledged frame in `render.ack`.
#[derive(Debug)]
pub struct Ack {
    /// The render id of the pane (the `pane` field of the frame).
    pub pane: u32,
    pub rev: u64,
}

/// What `render.ack` tells the render server.
#[derive(Debug)]
pub struct AckParams {
    pub window: WindowId,
    pub acks: Vec<Ack>,
    /// Panes whose next frame must be a keyframe: the client discarded a frame.
    pub need_keyframe: Vec<u32>,
}

/// What `window.focus_report` tells the render server.
#[derive(Debug)]
pub struct FocusReport {
    pub window: WindowId,
    pub focused: bool,
    pub visible: bool,
    /// Without it the window keeps the rate that it has.
    pub refresh_hz: Option<f64>,
}

/// What a window knows about one pane it shows.
pub(crate) struct Entry {
    pub pane: Arc<Pane>,
    /// What the webview of this window has seen of the pane.
    pub client: ClientState,
    /// The revisions of the frames that were sent and not acknowledged, oldest first.
    pub inflight: VecDeque<u64>,
    /// The next frame must be a keyframe.
    pub force_keyframe: bool,
    /// Forwards the damage of the pane to the pacer of the window.
    watcher: JoinHandle<()>,
}

impl Entry {
    /// Starts the pane again from nothing, for a webview that may have forgotten it: the page
    /// forgets each pane that it stops drawing. The next frame is a keyframe with every style and
    /// grapheme of its cells, and no frame in flight counts.
    fn restart(&mut self) {
        self.client = ClientState::new();
        self.inflight.clear();
        self.force_keyframe = true;
    }
}

impl Drop for Entry {
    fn drop(&mut self) {
        self.watcher.abort();
    }
}

pub(crate) struct Stream {
    pub sink: Arc<dyn FrameSink>,
}

/// The panes that the layout wants a window to draw.
#[derive(Default)]
pub(crate) struct Wanted {
    pub panes: Vec<Ulid>,
    pub focused: Option<Ulid>,
}

pub(crate) struct WindowState {
    /// What the layout wants the window to draw. The entries follow it.
    pub wanted: Wanted,
    pub refresh_hz: f64,
    pub visible: bool,
    pub focused: bool,
    /// The render id of the pane that has keyboard focus in the window.
    pub focused_pane: Option<u32>,
    pub entries: Vec<Entry>,
    pub stream: Option<Stream>,
    /// Counts the render streams of the window. The pacer of an older stream sees a newer number
    /// and ends.
    pub generation: u64,
}

/// A window of the layout, as the render server knows it.
pub(crate) struct Window {
    id: WindowId,
    pub state: Mutex<WindowState>,
    /// Wakes the pacer: damage, an acknowledgement, a change of focus or visibility.
    pub wake: Notify,
}

impl Window {
    /// Gives up the geometry lease of `pane` for this window.
    fn release(&self, pane: &Pane) {
        pane.release_lease(self.id);
    }

    fn new(id: WindowId) -> Self {
        Self {
            id,
            state: Mutex::new(WindowState {
                wanted: Wanted::default(),
                refresh_hz: DEFAULT_HZ,
                visible: true,
                focused: true,
                focused_pane: None,
                entries: Vec::new(),
                stream: None,
                generation: 0,
            }),
            wake: Notify::new(),
        }
    }
}

/// The windows and their render streams.
pub struct RenderServer {
    runtime: Handle,
    /// The panes of the core. A window draws the ones that the layout names.
    panes: Arc<Mutex<HashMap<Ulid, Arc<Pane>>>>,
    /// The windows of the layout. [`RenderServer::sync_layout`] adds and removes them.
    windows: Mutex<HashMap<WindowId, Arc<Window>>>,
    /// The ids of the windows that the layout has.
    layout_windows: Mutex<HashSet<WindowId>>,
    /// The peek streams that run, by pane. `pane.peek_close` ends them.
    peeks: Mutex<HashMap<Ulid, Vec<Arc<Notify>>>>,
}

impl RenderServer {
    pub(crate) fn new(runtime: Handle, panes: Arc<Mutex<HashMap<Ulid, Arc<Pane>>>>) -> Self {
        Self {
            runtime,
            panes,
            windows: Mutex::new(HashMap::new()),
            layout_windows: Mutex::new(HashSet::new()),
            peeks: Mutex::new(HashMap::new()),
        }
    }

    /// The window with this id, if the layout has it.
    fn window(&self, id: WindowId) -> Option<Arc<Window>> {
        lock(&self.windows).get(&id).cloned()
    }

    /// Fails when the layout has no window with this id.
    pub(crate) fn require_window(&self, id: WindowId) -> Result<(), RpcError> {
        if lock(&self.layout_windows).contains(&id) {
            Ok(())
        } else {
            Err(no_window(id))
        }
    }

    /// A window measured the size of its cells in device pixels. Each pane that it draws learns
    /// it.
    pub(crate) fn report_metrics(&self, window: WindowId, cell_width: u16, cell_height: u16) {
        let Some(window) = self.window(window) else {
            return;
        };
        let panes: Vec<Arc<Pane>> = lock(&window.state)
            .entries
            .iter()
            .map(|entry| Arc::clone(&entry.pane))
            .collect();
        for pane in panes {
            pane.set_cell_px(cell_width, cell_height);
        }
    }

    /// An entry for `pane` in `window`: the first frame is a keyframe, and the damage of the pane
    /// wakes the pacer of the window.
    fn new_entry(&self, window: &Arc<Window>, pane: &Arc<Pane>) -> Entry {
        let mut rx = pane.subscribe_damage();
        let weak: Weak<Window> = Arc::downgrade(window);
        let watcher = self.runtime.spawn(async move {
            while rx.changed().await.is_ok() {
                match weak.upgrade() {
                    Some(window) => window.wake.notify_one(),
                    None => return,
                }
            }
        });
        Entry {
            pane: Arc::clone(pane),
            client: ClientState::new(),
            inflight: VecDeque::new(),
            force_keyframe: true,
            watcher,
        }
    }

    /// Brings the windows to the state of the layout: which panes each one draws, and which one
    /// has focus. A window that the layout lost is dropped.
    pub(crate) fn sync_layout(&self, layout: &Layout) {
        let mut ids = HashSet::new();
        for window in &layout.windows {
            let panes: Vec<Ulid> = layout
                .visible_panes(window)
                .iter()
                .map(|pane| pane.ulid())
                .collect();
            let focused = window.focused_pane.map(|pane| pane.ulid());
            let handle = Arc::clone(
                lock(&self.windows)
                    .entry(window.id)
                    .or_insert_with(|| Arc::new(Window::new(window.id))),
            );
            lock(&handle.state).wanted = Wanted { panes, focused };
            self.reconcile(&handle);
            ids.insert(window.id);
        }
        let gone: Vec<WindowId> = {
            let mut known = lock(&self.layout_windows);
            let gone = known.difference(&ids).copied().collect();
            *known = ids;
            gone
        };
        for id in gone {
            self.forget_window(id);
        }
    }

    /// A pane came or went in the map of the core. The windows that want it draw it, or stop.
    pub(crate) fn pane_changed(&self, pane: Ulid) {
        let windows: Vec<Arc<Window>> = lock(&self.windows).values().cloned().collect();
        for window in windows {
            let wants = lock(&window.state).wanted.panes.contains(&pane);
            if wants {
                self.reconcile(&window);
            }
        }
    }

    /// Makes the entries of a window of the layout match what it wants. An entry whose pane is
    /// another object now (a restart) starts again from a keyframe.
    fn reconcile(&self, window: &Arc<Window>) {
        let released = {
            let panes = lock(&self.panes);
            let mut state = lock(&window.state);
            self.reconcile_locked(window, &panes, &mut state)
        };
        // A pane that the window does not draw any more is not sized by it.
        for pane in released {
            window.release(&pane);
        }
        window.wake.notify_one();
    }

    /// The part of [`RenderServer::reconcile`] that holds the locks. Returns the panes that the
    /// window stopped drawing.
    fn reconcile_locked(
        &self,
        window: &Arc<Window>,
        panes: &HashMap<Ulid, Arc<Pane>>,
        state: &mut WindowState,
    ) -> Vec<Arc<Pane>> {
        let (wanted_panes, wanted_focus) = (state.wanted.panes.clone(), state.wanted.focused);
        let mut old = std::mem::take(&mut state.entries);
        let mut entries = Vec::with_capacity(wanted_panes.len());
        for id in &wanted_panes {
            let Some(pane) = panes.get(id) else {
                continue;
            };
            match old.iter().position(|entry| Arc::ptr_eq(&entry.pane, pane)) {
                Some(at) => entries.push(old.swap_remove(at)),
                None => entries.push(self.new_entry(window, pane)),
            }
        }
        // An entry that is left in `old` is dropped, and it stops its watcher.
        let released = old.iter().map(|entry| Arc::clone(&entry.pane)).collect();
        drop(old);
        state.entries = entries;
        state.focused_pane = wanted_focus
            .and_then(|id| panes.get(&id))
            .map(|pane| pane.render_id());
        released
    }

    /// A window of the layout is gone: its stream ends and its entries go.
    fn forget_window(&self, id: WindowId) {
        let Some(window) = lock(&self.windows).remove(&id) else {
            return;
        };
        let released: Vec<Arc<Pane>> = {
            let mut state = lock(&window.state);
            state.generation += 1;
            state.stream = None;
            state
                .entries
                .drain(..)
                .map(|e| Arc::clone(&e.pane))
                .collect()
        };
        window.wake.notify_waiters();
        for pane in released {
            window.release(&pane);
        }
    }

    /// Opens the peek stream of a pane: keyframes of its bottom rows and left columns, at
    /// most `max_hz` a second (and never more than 4).
    pub(crate) fn open_peek(
        &self,
        pane: Arc<Pane>,
        max_hz: Option<f32>,
        max_cols: u16,
        max_rows: u16,
        sink: Arc<dyn FrameSink>,
    ) {
        let cancel = Arc::new(Notify::new());
        {
            let mut peeks = lock(&self.peeks);
            let list = peeks.entry(pane.id()).or_default();
            // A stream that ended on its own leaves its handle here. Forget those that nobody holds.
            list.retain(|handle| Arc::strong_count(handle) > 1);
            list.push(Arc::clone(&cancel));
        }
        self.runtime.spawn(peek::run(
            pane,
            max_hz.unwrap_or(peek::MAX_PEEK_HZ),
            max_cols,
            max_rows,
            sink,
            cancel,
        ));
    }

    /// Ends the peek streams of a pane (`pane.peek_close`).
    pub(crate) fn close_peek(&self, pane: Ulid) {
        let handles = lock(&self.peeks).remove(&pane).unwrap_or_default();
        for handle in handles {
            handle.notify_one();
        }
    }

    /// Opens the scrollback stream of a pane: the lines `from..from + count` of the
    /// archive, in pages.
    pub(crate) fn open_scrollback(
        &self,
        pane: Arc<Pane>,
        from: u64,
        count: u64,
        sink: Arc<dyn FrameSink>,
    ) {
        self.runtime.spawn(scrollback::run(pane, from, count, sink));
    }

    /// Opens the render stream of a window (`stream("render", …)`). A second stream for the same
    /// window replaces the first one. Every pane of the window then starts again from a keyframe.
    pub(crate) fn open_stream(
        &self,
        params: &RenderParams,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError> {
        if let Some(proto) = params.proto
            && proto != nh_proto::version::FRAME_PROTO
        {
            return Err(RpcError::new(
                ErrorCode::ProtoUnsupported,
                format!(
                    "the client decodes frame format {proto}; this core sends {}",
                    nh_proto::version::FRAME_PROTO
                ),
            ));
        }
        let window = self
            .window(params.window)
            .ok_or_else(|| no_window(params.window))?;
        let generation = {
            let mut state = lock(&window.state);
            if let Some(hz) = params.viewport_hz {
                state.refresh_hz = clamp_hz(hz);
            }
            state.generation += 1;
            state.stream = Some(Stream { sink });
            for entry in &mut state.entries {
                entry.restart();
            }
            state.generation
        };
        // The pacer of the older stream ends at once, and not at the next damage.
        window.wake.notify_waiters();
        self.runtime.spawn(pacer::run(window, generation));
        Ok(())
    }

    /// `render.ack`: frames were painted, or the client needs a keyframe.
    pub(crate) fn ack(&self, params: &AckParams) {
        let Some(window) = self.window(params.window) else {
            return;
        };
        {
            let mut state = lock(&window.state);
            for ack in &params.acks {
                if let Some(entry) = state
                    .entries
                    .iter_mut()
                    .find(|e| e.pane.render_id() == ack.pane)
                {
                    while entry.inflight.front().is_some_and(|rev| *rev <= ack.rev) {
                        entry.inflight.pop_front();
                    }
                }
            }
            for pane in &params.need_keyframe {
                if let Some(entry) = state
                    .entries
                    .iter_mut()
                    .find(|e| e.pane.render_id() == *pane)
                {
                    // The client dropped the frames that follow the bad one too, and it will not
                    // acknowledge them. It can also be a pane that the page forgot and draws
                    // again, without the styles of the frames before.
                    entry.restart();
                }
            }
        }
        window.wake.notify_one();
    }

    /// `window.focus_report`: focus, visibility and refresh rate. A window that was hidden gets a
    /// keyframe for every pane when it is shown again.
    pub(crate) fn focus_report(&self, report: &FocusReport) {
        let Some(window) = self.window(report.window) else {
            return;
        };
        {
            let mut state = lock(&window.state);
            let revealed = report.visible && !state.visible;
            state.focused = report.focused;
            state.visible = report.visible;
            if let Some(hz) = report.refresh_hz {
                state.refresh_hz = clamp_hz(hz);
            }
            if revealed {
                // A view that covered the panes, such as the Git view, made the page forget them.
                for entry in &mut state.entries {
                    entry.restart();
                }
            }
        }
        window.wake.notify_one();
    }
}

/// The error for a window that the layout does not have.
fn no_window(id: WindowId) -> RpcError {
    RpcError::new(ErrorCode::NotFound, format!("no window {id}"))
}

/// A refresh rate that the pacer can use: 1 to 240 Hz. Anything else is a mistake of the client.
fn clamp_hz(hz: f64) -> f64 {
    if hz.is_finite() {
        hz.clamp(1.0, 240.0)
    } else {
        DEFAULT_HZ
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn a_window_over_a_link_has_frames_in_flight_for_its_round_trip() {
        let ms = Duration::from_millis;
        // No link: two frames, as before.
        assert_eq!(in_flight_limit(None, 60.0), 2);
        assert_eq!(in_flight_limit(None, 120.0), 2);
        // max(2, ceil(rtt / period) + 1), at most 8.
        for (rtt, at_60, at_120) in [(0, 2, 2), (16, 2, 3), (50, 4, 7), (200, 8, 8)] {
            assert_eq!(
                in_flight_limit(Some(ms(rtt)), 60.0),
                at_60,
                "{rtt} ms at 60 Hz"
            );
            assert_eq!(
                in_flight_limit(Some(ms(rtt)), 120.0),
                at_120,
                "{rtt} ms at 120 Hz"
            );
        }
        // A rate that is not a number gives the rate of 60 Hz.
        assert_eq!(in_flight_limit(Some(ms(50)), f64::NAN), 4);
        assert_eq!(in_flight_limit(Some(ms(50)), 0.0), 4);
    }

    #[test]
    fn a_refresh_rate_is_kept_in_range() {
        assert_eq!(clamp_hz(120.0), 120.0);
        assert_eq!(clamp_hz(0.0), 1.0);
        assert_eq!(clamp_hz(1000.0), 240.0);
        assert_eq!(clamp_hz(f64::NAN), DEFAULT_HZ);
    }
}
