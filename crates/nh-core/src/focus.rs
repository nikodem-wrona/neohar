//! Which panes have the focus of the user.
//!
//! A pane is focused when all of these hold: it is the active element of a window, that window has
//! the focus of the OS and can be seen, and the pane is one of the panes that the window shows.
//! The windows report the facts with `window.focus_report`. A change counts only after it stood
//! for [`DEBOUNCE`], so that a switch between applications does not make the focus flap. A peek at
//! a pane (a thumbnail, the strip of a card) is not the active element of a window, so it never
//! changes the focus.
//!
//! The settled set is what the gate reads: a new interaction in the mode `auto` is held only when
//! its pane is not in the set, and a pane that enters the set releases the hooks that are held for
//! it.
//!
//! A phone window counts for the focus of a pane like any window. The push to a
//! phone reads a coarser fact: whether any window of a desktop client has the
//! focus of the OS, with the same debounce. A phone window does not count there. A locked screen or a sleeping display
//! of the host in the process counts as no focus for the windows of that host. The host reports
//! the screen with [`crate::CoreHandle::report_screen`], and a lock counts at once.

use std::{
    collections::BTreeSet,
    sync::{Arc, OnceLock},
    time::Duration,
};

use nh_proto::api::ids::PaneId;
use tokio::runtime::Handle;
use ulid::Ulid;

use crate::{
    actor::{ActorHandle, Parts},
    interaction::apply,
    layout::model::Layout,
};

/// How long a change of the focus must stand before it counts.
pub const DEBOUNCE: Duration = Duration::from_millis(500);

/// The panes that the facts of the windows say are focused now, without the debounce.
pub fn raw(layout: &Layout) -> BTreeSet<PaneId> {
    layout
        .windows
        .iter()
        .filter(|w| w.facts.focused && w.facts.visible && !w.facts.minimized)
        .filter_map(|w| {
            w.facts
                .active_pane
                .filter(|pane| layout.visible_panes(w).contains(pane))
        })
        .collect()
}

/// Whether a window of a desktop client has the focus of the OS now, without the debounce.
/// The windows of the device `away` do not count: their screen is locked or asleep.
/// A window that no client has adopted counts as a window of that device. A window of a device
/// for which `phone` is true is a phone window, and it never counts.
pub fn desk_raw(layout: &Layout, away: Option<Ulid>, phone: impl Fn(Ulid) -> bool) -> bool {
    layout
        .windows
        .iter()
        .filter(|w| w.facts.focused && w.facts.visible && !w.facts.minimized)
        .filter(|w| !w.client.is_some_and(&phone))
        .any(|w| away.is_none() || (w.client.is_some() && w.client != away))
}

/// The screen of the host in the process.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Screen {
    pub locked: bool,
    pub display_asleep: bool,
}

impl Screen {
    /// The person is not at the screen: it is locked, or its display sleeps.
    pub fn away(self) -> bool {
        self.locked || self.display_asleep
    }
}

/// The timer that wakes the actor when a change of the focus has stood long enough.
pub struct FocusClock {
    handle: Handle,
    debounce: Duration,
    actor: OnceLock<ActorHandle>,
}

impl FocusClock {
    /// A clock whose changes stand for `debounce` before they count. The product uses
    /// [`DEBOUNCE`].
    pub fn new(handle: Handle, debounce: Duration) -> Arc<Self> {
        Arc::new(Self {
            handle,
            debounce,
            actor: OnceLock::new(),
        })
    }

    /// Gives the clock the actor that it wakes.
    pub fn bind(&self, actor: ActorHandle) {
        let _ = self.actor.set(actor);
    }

    /// After the debounce, settles the focus of the pane.
    fn arm(&self, pane: PaneId) {
        let Some(actor) = self.actor.get().cloned() else {
            return;
        };
        let debounce = self.debounce;
        self.handle.spawn(async move {
            tokio::time::sleep(debounce).await;
            actor.update_detached(move |mut parts| settle(&mut parts, pane));
        });
    }

    /// After the debounce, settles whether a desktop window has the focus.
    fn arm_desk(&self) {
        let Some(actor) = self.actor.get().cloned() else {
            return;
        };
        let debounce = self.debounce;
        self.handle.spawn(async move {
            tokio::time::sleep(debounce).await;
            actor.update_detached(|mut parts| settle_desk(&mut parts));
        });
    }

    /// Asks the actor to look at the facts again, for a change that a command made to the layout.
    pub fn nudge(&self) {
        let Some(actor) = self.actor.get().cloned() else {
            return;
        };
        actor.update_detached(|mut parts| refresh(&mut parts));
    }
}

/// The settled focus, and the timers that are armed.
#[derive(Clone, Default)]
pub struct Focus {
    settled: BTreeSet<PaneId>,
    armed: BTreeSet<PaneId>,
    clock: Option<Arc<FocusClock>>,
    /// Whether a window of a desktop client has the focus, after the debounce.
    desk: bool,
    desk_armed: bool,
    /// The screen of the host in the process.
    screen: Screen,
}

impl Focus {
    /// A focus whose changes wait for the debounce. Without a clock a change counts at once.
    pub fn with_clock(clock: Arc<FocusClock>) -> Self {
        Self {
            clock: Some(clock),
            ..Self::default()
        }
    }

    /// Whether the pane has the focus of the user, after the debounce.
    pub fn is_focused(&self, pane: PaneId) -> bool {
        self.settled.contains(&pane)
    }

    /// Whether a window of a desktop client has the focus of the OS, after the debounce.
    /// With no desktop client connected, none has.
    pub fn desk_focused(&self) -> bool {
        self.desk
    }

    /// Looks at the facts of the windows again, from a command of the layout.
    pub fn nudge(&self) {
        if let Some(clock) = &self.clock {
            clock.nudge();
        }
    }
}

/// The facts of a window or the layout changed: the panes whose focus differs from the settled
/// one get a timer.
pub(crate) fn refresh(parts: &mut Parts<'_>) {
    let now = raw(&parts.state.layout);
    let changed: Vec<PaneId> = now
        .symmetric_difference(&parts.state.focus.settled)
        .copied()
        .collect();
    for pane in changed {
        match parts.state.focus.clock.clone() {
            None => settle(parts, pane),
            Some(clock) => {
                if parts.state.focus.armed.insert(pane) {
                    clock.arm(pane);
                }
            }
        }
    }
    refresh_desk(parts);
}

/// The device whose windows do not count for the desk: the host in the process, while its screen
/// is locked or asleep.
fn away_device(parts: &Parts<'_>) -> Option<Ulid> {
    if !parts.state.focus.screen.away() {
        return None;
    }
    parts
        .state
        .host
        .as_ref()
        .and_then(|host| host.local_device())
}

fn desk_now(parts: &Parts<'_>) -> bool {
    let away = away_device(parts);
    // A core without a device of its host cannot tell which windows are on that screen: none
    // counts while it is locked.
    if parts.state.focus.screen.away() && away.is_none() {
        return false;
    }
    // A phone that a link relays is no paired phone here, but its window is a phone window too.
    let phones = &parts.state.phones;
    let host = parts.state.host.as_ref();
    desk_raw(&parts.state.layout, away, |device| {
        phones.is_phone(device) || host.is_some_and(|host| host.is_phone(Some(device)))
    })
}

/// Whether a desktop window has the focus differs from the settled value: a timer settles it.
fn refresh_desk(parts: &mut Parts<'_>) {
    if desk_now(parts) == parts.state.focus.desk {
        return;
    }
    match parts.state.focus.clock.clone() {
        None => settle_desk(parts),
        Some(clock) => {
            if !parts.state.focus.desk_armed {
                parts.state.focus.desk_armed = true;
                clock.arm_desk();
            }
        }
    }
}

/// The debounce of the desk is over: it has the focus when the facts say so now.
pub(crate) fn settle_desk(parts: &mut Parts<'_>) {
    parts.state.focus.desk_armed = false;
    parts.state.focus.desk = desk_now(parts);
}

/// The host in the process reports its screen. A lock or a display that sleeps counts
/// at once, with no debounce: it is an event, not a fact that flaps. When it takes the focus from
/// the desk, each open row of `attention` that had no push gets one.
pub(crate) fn screen(parts: &mut Parts<'_>, screen: Screen) {
    let was_away = parts.state.focus.screen.away();
    parts.state.focus.screen = screen;
    if !screen.away() {
        refresh_desk(parts);
        return;
    }
    let had_focus = parts.state.focus.desk;
    parts.state.focus.desk = desk_now(parts);
    if !was_away && !parts.state.focus.desk {
        tracing::debug!(had_focus, "the screen is locked or asleep");
        crate::native::push::away(parts.state);
    }
}

/// The debounce of a pane is over: its focus is what the facts say now.
pub(crate) fn settle(parts: &mut Parts<'_>, pane: PaneId) {
    parts.state.focus.armed.remove(&pane);
    let focused = raw(&parts.state.layout).contains(&pane);
    let focus = &mut parts.state.focus;
    let changed = if focused {
        focus.settled.insert(pane)
    } else {
        focus.settled.remove(&pane)
    };
    if changed && focused {
        gained(parts, pane);
    }
}

/// The pane has the focus: the hooks that are held for its interactions go, and the harness shows
/// its own dialog.
fn gained(parts: &mut Parts<'_>, pane: PaneId) {
    let held = parts.state.interactions.held_for(pane);
    for id in held {
        apply::release(parts, id);
    }
    crate::native::focus_gained(parts.state, pane);
    // The person looks at the pane: its run does not wait after its turn.
    crate::agents::turn::seen(parts, pane);
}

#[cfg(test)]
mod tests;
