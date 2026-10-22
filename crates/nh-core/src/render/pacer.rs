//! The pacer of a window: when to build frames, and which.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use nh_proto::frame::FrameKind;
use tokio::time::sleep_until;

use super::{Window, in_flight_limit};
use crate::pane::lock;

/// Minor damage on a visible pane without focus goes out at most once a second. All
/// the panes of a window share the clock of this period, so their frames go out in one tick, and
/// the page paints and acknowledges them once.
///
/// A repaint of a visible window costs about 20 ms of CPU in the system web view, whatever its
/// size, because the web view runs its display link for about half a second after each one. The
/// cost grows with the number of repaints a second until the display link never stops, at 4% of a
/// core. At a period of 250 ms, 20 panes cost 5% of a core. At 1 s they cost 2%.
const MINOR_PERIOD: Duration = Duration::from_secs(1);

/// What a tick found.
enum Tick {
    /// The stream ended, or a newer stream replaced it. The pacer ends.
    Closed,
    /// Nothing is left to send until the next wake-up.
    Idle,
    /// A pane waits for its slot at 4 Hz. Wake up at this time.
    RetryAt(Instant),
}

/// Runs the pacer of one render stream of `window`.
///
/// It sleeps until something wakes it. Then it waits until one period of the refresh rate has
/// passed since the last tick that **sent a frame**, so that damage that arrives within one period
/// becomes one frame. Damage that arrives after a quiet time goes out at once. A tick that sends
/// nothing, for example the one after an acknowledgement, does not start a period. Between
/// wake-ups it uses no CPU.
///
/// A new stream starts its pacer with a tick at once, because its panes start from keyframes and
/// nothing else would wake it. An older pacer of the same window ends when it sees that its stream
/// was replaced. All pacers of a window wait on one `Notify`, so an older pacer can take a wake-up
/// that was meant for the new one. It hands the wake-up on before it ends.
pub(super) async fn run(window: Arc<Window>, generation: u64) {
    let mut last_tick: Option<Instant> = None;
    let mut retry_at: Option<Instant> = None;
    // When the panes that have minor damage and no focus may send their next frames.
    let mut minor_due: Option<Instant> = None;
    let mut first = true;
    loop {
        if !first {
            let woken = window.wake.notified();
            tokio::pin!(woken);
            // Registered from here on, so that the call of `notify_waiters` for a replaced stream
            // cannot pass between the check below and the wait.
            woken.as_mut().enable();
            if replaced(&window, generation) {
                return hand_over(&window);
            }
            match retry_at {
                Some(at) => {
                    tokio::select! {
                        () = &mut woken => {}
                        () = sleep_until(at.into()) => {}
                    }
                }
                None => woken.await,
            }
        }
        first = false;
        let hz = {
            let state = lock(&window.state);
            if state.generation != generation {
                drop(state);
                return hand_over(&window);
            }
            state.refresh_hz
        };
        if let Some(previous) = last_tick {
            let due = previous + Duration::from_secs_f64(1.0 / hz);
            if due > Instant::now() {
                sleep_until(due.into()).await;
            }
        }
        let tick_time = Instant::now();
        let mut sent = false;
        let outcome = tick(&window, generation, &mut minor_due, &mut sent);
        if sent {
            last_tick = Some(tick_time);
        }
        match outcome {
            Tick::Closed => {
                if replaced(&window, generation) {
                    hand_over(&window);
                }
                return;
            }
            Tick::Idle => retry_at = None,
            Tick::RetryAt(at) => retry_at = Some(at),
        }
    }
}

/// Whether a newer stream of the window has replaced this one.
fn replaced(window: &Window, generation: u64) -> bool {
    lock(&window.state).generation != generation
}

/// A pacer that ends because its stream was replaced may have taken a wake-up that was meant for
/// the pacer of the new stream. It gives the wake-up back.
fn hand_over(window: &Window) {
    window.wake.notify_one();
}

/// Builds and sends the frames that are due. `sent` tells the caller whether a frame went out.
/// `minor_due` is the time at which the panes with minor damage and no focus may send again.
fn tick(
    window: &Window,
    generation: u64,
    minor_due: &mut Option<Instant>,
    sent: &mut bool,
) -> Tick {
    let mut due: Vec<Vec<u8>> = Vec::new();
    let mut retry: Option<Instant> = None;
    let mut sent_minor = false;
    let sink = {
        let mut state = lock(&window.state);
        if state.generation != generation {
            return Tick::Closed;
        }
        let Some(sink) = state.stream.as_ref().map(|s| Arc::clone(&s.sink)) else {
            return Tick::Closed;
        };
        // A sink whose receiver is gone ends the stream, also when the window gets no frames:
        // the sink of a link that is down holds the link.
        if sink.is_closed() {
            state.stream = None;
            return Tick::Closed;
        }
        // A hidden window gets no frames. The panes keep their damage and the window gets
        // keyframes when it is shown.
        if !state.visible {
            return Tick::Idle;
        }
        let now = Instant::now();
        let window_focused = state.focused;
        let focused_pane = state.focused_pane;
        // A window over a link may have more frames in flight, as its round trip is longer.
        let limit = in_flight_limit(sink.round_trip(), state.refresh_hz);
        for entry in &mut state.entries {
            if entry.inflight.len() >= limit {
                continue;
            }
            if !entry.force_keyframe && entry.pane.rev() <= entry.client.rev() {
                continue;
            }
            let has_focus = window_focused && focused_pane == Some(entry.pane.render_id());
            let minor_only = !has_focus
                && !entry.force_keyframe
                && entry.pane.non_minor_rev() <= entry.client.rev();
            if minor_only
                && let Some(at) = *minor_due
                && now < at
            {
                retry = Some(retry.map_or(at, |r| r.min(at)));
                continue;
            }
            let want = if entry.force_keyframe {
                FrameKind::Keyframe
            } else {
                FrameKind::Delta
            };
            let frame = entry.pane.build_frame(&mut entry.client, want);
            entry.force_keyframe = false;
            let bytes = match postcard::to_stdvec(&frame) {
                Ok(bytes) => bytes,
                Err(error) => {
                    tracing::error!(%error, pane = frame.pane, "cannot encode a frame");
                    continue;
                }
            };
            entry.inflight.push_back(frame.rev);
            sent_minor |= minor_only;
            due.push(bytes);
        }
        // The panes that wait for their turn all send when the clock of the window says so.
        if sent_minor {
            *minor_due = Some(now + MINOR_PERIOD);
        }
        sink
    };
    *sent = !due.is_empty();
    for bytes in due {
        if !sink.send(bytes) {
            // A newer stream may have replaced this one while the frame was sent, for example
            // after a reconnect of the client: it is not ours to end.
            let mut state = lock(&window.state);
            if state.generation == generation {
                state.stream = None;
            }
            return Tick::Closed;
        }
    }
    retry.map_or(Tick::Idle, Tick::RetryAt)
}
