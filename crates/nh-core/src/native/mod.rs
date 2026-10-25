//! What the core shows outside the windows: notifications, the badge and the tray.
//!
//! The core decides what these surfaces say and when. The [`Host`] only draws them and reports a
//! click. The rules:
//!
//! - An interaction that opens on a pane without the focus of the user gets one notification. A
//!   pane has the focus when a window has the focus of the OS, shows the pane, and has it as the
//!   active element. The focus of a pane after the debounce decides.
//! - A run has at most one notification at a time, and a run posts at most one in each window of
//!   [`COALESCE`]. A second interaction of the run changes the text of the notification that
//!   shows. If the run has none and posted one lately, the post waits for the end of the window.
//! - The notification goes when the interaction ends, and when its pane gets the focus. The
//!   latter is the release of a held hook: the harness shows its own dialog.
//! - The badge is the number of rows of `attention`. The tray lists the open interactions
//!   that a client can answer, in the order of `interaction.next`.
//! - A run whose turn ended while nobody looked gets one notification with Open, unless the
//!   setting `agents.turn_end_notice` is off. An interaction of the run comes
//!   first. The notification goes when the run stops waiting.
//! - A paired phone gets a push for a row of `attention` when no desktop window has the focus
//!   ([`push`]).
//!
//! All of this runs on the state actor, after the change that causes it. Each function is a
//! diff: it looks at the table of interactions and the facts that the host has, and it calls the
//! host only for what differs from the last call.

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    future::Future,
    sync::{Arc, OnceLock},
    time::Duration,
};

use nh_proto::api::{
    common::{Harness, Urgency, Via},
    ids::{InteractionId, NotificationId, PaneId, RunId},
    model::{InteractionKind, InteractionState},
};
use tokio::runtime::Handle;

use crate::{
    actor::{ActorHandle, Parts, State},
    agents::project::{attention_value, interaction_urgency},
    host::{
        Host, HostNotification, NotificationAction, NotificationCapabilities, TrayEntry, TrayModel,
    },
    interaction::model::{Interaction, Refusal},
    persist::now_ms,
    router::HostRouter,
};

/// How long a run waits between two posts of a notification.
pub const COALESCE: Duration = Duration::from_secs(10);

/// The timer that wakes the actor when a post that waited can go.
pub struct NativeClock {
    handle: Handle,
    actor: OnceLock<ActorHandle>,
}

impl NativeClock {
    pub fn new(handle: Handle) -> Arc<Self> {
        Arc::new(Self {
            handle,
            actor: OnceLock::new(),
        })
    }

    /// Gives the clock the actor that it wakes.
    pub fn bind(&self, actor: ActorHandle) {
        let _ = self.actor.set(actor);
    }

    fn after(&self, delay: Duration, job: impl FnOnce(Parts<'_>) + Send + 'static) {
        let Some(actor) = self.actor.get().cloned() else {
            return;
        };
        self.handle.spawn(async move {
            tokio::time::sleep(delay).await;
            actor.update_detached(job);
        });
    }

    /// Runs `work` on the runtime, then `then` with its result on the actor.
    fn run<T: Send + 'static>(
        &self,
        work: impl Future<Output = T> + Send + 'static,
        then: impl FnOnce(Parts<'_>, T) + Send + 'static,
    ) {
        let Some(actor) = self.actor.get().cloned() else {
            return;
        };
        self.handle.spawn(async move {
            let result = work.await;
            actor.update_detached(move |parts| then(parts, result));
        });
    }
}

/// The notification that a run shows now.
#[derive(Clone, Debug)]
struct Notice {
    id: NotificationId,
    /// What the host shows. A change of the text is an update, and the same text is not sent again.
    shown: HostNotification,
}

/// What the core knows about its surfaces.
#[derive(Clone)]
pub(crate) struct Native {
    clock: Option<Arc<NativeClock>>,
    window: Duration,
    /// The interactions that opened on a pane without the focus, and have not ended or been
    /// released. Only these get a notification.
    noticed: BTreeSet<InteractionId>,
    notices: HashMap<RunId, Notice>,
    /// When the last notification of each run went out, in unix milliseconds.
    posted: HashMap<RunId, i64>,
    /// The runs whose post waits for a timer.
    armed: HashSet<RunId>,
    badge: Option<u32>,
    tray: Option<TrayModel>,
    /// The number of the last `interaction.reveal`.
    reveals: u64,
    /// How long the execution of a run stays `idle` before its turn counts as ended.
    turn_settle: Duration,
    /// The rule of the push to a phone.
    pub(crate) push: push::Pushes,
}

impl Default for Native {
    fn default() -> Self {
        Self {
            clock: None,
            window: COALESCE,
            noticed: BTreeSet::new(),
            notices: HashMap::new(),
            posted: HashMap::new(),
            armed: HashSet::new(),
            badge: None,
            tray: None,
            reveals: 0,
            turn_settle: crate::agents::turn::SETTLE,
            push: push::Pushes::default(),
        }
    }
}

impl Native {
    /// Surfaces whose clock wakes the actor for the posts that wait.
    pub fn with_clock(clock: Arc<NativeClock>, window: Duration) -> Self {
        Self {
            clock: Some(clock),
            window,
            ..Self::default()
        }
    }

    /// The number for the next request to reveal an interaction.
    pub fn next_reveal(&mut self) -> u64 {
        self.reveals += 1;
        self.reveals
    }

    /// How long the execution of a run stays `idle` before its turn counts as ended.
    pub fn turn_settle(&self) -> Duration {
        self.turn_settle
    }

    /// Sets the settle time of a turn end (`Tuning::turn_settle`).
    pub fn set_turn_settle(&mut self, settle: Duration) {
        self.turn_settle = settle;
    }

    /// The coalesce window of a run.
    pub(crate) fn window(&self) -> Duration {
        self.window
    }

    /// Runs `work` on the runtime and `then` with its result on the actor. Nothing runs when the
    /// surfaces have no clock.
    pub(crate) fn run<T: Send + 'static>(
        &self,
        work: impl Future<Output = T> + Send + 'static,
        then: impl FnOnce(Parts<'_>, T) + Send + 'static,
    ) {
        if let Some(clock) = &self.clock {
            clock.run(work, then);
        }
    }

    /// Runs `job` on the actor after `delay`. Returns false when the surfaces have no clock, and
    /// then nothing runs.
    pub(crate) fn after(
        &self,
        delay: Duration,
        job: impl FnOnce(Parts<'_>) + Send + 'static,
    ) -> bool {
        match &self.clock {
            Some(clock) => {
                clock.after(delay, job);
                true
            }
            None => false,
        }
    }
}

/// An interaction was put in the table, new or changed. `new` is true the first time.
pub(crate) fn changed(state: &mut State, id: InteractionId, new: bool) {
    let Some(host) = state.host.clone() else {
        return;
    };
    let Some(interaction) = state.interactions.get(id) else {
        return;
    };
    let (run, pane, open) = (
        interaction.run,
        interaction.pane,
        interaction.state == InteractionState::Open,
    );
    if !open {
        state.native.noticed.remove(&id);
    } else if new && !pane.is_some_and(|pane| state.focus.is_focused(pane)) {
        state.native.noticed.insert(id);
    }
    if let Some(run) = run {
        refresh_run(state, host.as_ref(), run);
    }
    summary(state, host.as_ref());
}

/// The pane has the focus of the user: the notifications of its interactions go.
pub(crate) fn focus_gained(state: &mut State, pane: PaneId) {
    let Some(host) = state.host.clone() else {
        return;
    };
    let runs: BTreeSet<RunId> = state
        .native
        .noticed
        .iter()
        .filter_map(|id| state.interactions.get(*id))
        .filter(|i| i.pane == Some(pane))
        .filter_map(|i| i.run)
        .collect();
    let seen: Vec<InteractionId> = state
        .native
        .noticed
        .iter()
        .copied()
        .filter(|id| {
            state
                .interactions
                .get(*id)
                .is_some_and(|i| i.pane == Some(pane))
        })
        .collect();
    for id in seen {
        state.native.noticed.remove(&id);
    }
    for run in runs {
        refresh_run(state, host.as_ref(), run);
    }
}

/// The turn of a run ended while nobody looked, or the run stopped waiting: its
/// notification and the badge follow.
pub(crate) fn run_changed(state: &mut State, run: RunId) {
    let Some(host) = state.host.clone() else {
        return;
    };
    refresh_run(state, host.as_ref(), run);
    summary(state, host.as_ref());
}

/// The badge and the tray follow `attention` and the open interactions. A call to the host goes
/// out only when the value differs from the last one.
pub(crate) fn summary(state: &mut State, host: &dyn Host) {
    let badge = u32::try_from(
        state
            .runs
            .by_id
            .values()
            .filter(|run| attention_value(run, &state.interactions).is_some())
            .count(),
    )
    .unwrap_or(u32::MAX);
    if state.native.badge != Some(badge) {
        state.native.badge = Some(badge);
        host.set_badge(badge);
    }
    let tray = TrayModel {
        entries: state
            .interactions
            .open()
            .into_iter()
            .filter(|i| i.answerable && i.state == InteractionState::Open)
            .map(|i| TrayEntry {
                label: tray_label(state, i),
                interaction: Some(i.id),
                approval: i.kind == InteractionKind::Approval,
            })
            .collect(),
        badge,
    };
    if state.native.tray.as_ref() != Some(&tray) {
        state.native.tray = Some(tray.clone());
        host.set_tray(tray);
    }
    push::follow(state);
}

/// The surfaces as they are at the start of the core: the interactions that the last core left
/// open count for the badge and the tray, and they get no notification.
pub(crate) fn started(state: &mut State) {
    if let Some(host) = state.host.clone() {
        summary(state, host.as_ref());
    }
}

/// A click on an action of a notification came too late: the interaction has its answer. The
/// person sees who answered, on the client where they clicked.
pub(crate) fn late_action(
    host: &HostRouter,
    device: Option<ulid::Ulid>,
    interaction: &Interaction,
    refusal: &Refusal,
    clicked: Via,
) {
    if clicked != Via::Notification {
        return;
    }
    let by = match refusal {
        Refusal::AlreadyAnswered { via, .. } => via_name(*via),
        Refusal::Closed(_) => "the agent".to_owned(),
        _ => return,
    };
    host.notify_for(
        device,
        HostNotification {
            title: "Already answered".to_owned(),
            body: format!("{} was answered by {by}.", interaction.title),
            urgency: Urgency::Low,
            interaction: None,
            pane: None,
            actions: Vec::new(),
        },
    );
}

fn via_name(via: Option<Via>) -> String {
    match via {
        Some(Via::Card) => "a card",
        Some(Via::Notification) => "a notification",
        Some(Via::Tray) => "the tray",
        Some(Via::Palette) => "the palette",
        Some(Via::Cli) => "the command line",
        Some(Via::Policy) => "a rule",
        Some(Via::Phone) => "the phone",
        None => "another client",
    }
    .to_owned()
}

/// The longest label of a line of the tray, in characters. A command can be long.
const LABEL_MAX: usize = 80;

fn tray_label(state: &State, interaction: &Interaction) -> String {
    let label = match interaction
        .run
        .and_then(|run| state.runs.by_id.get(&run))
        .map(|run| run.harness)
    {
        Some(harness) => format!("{}: {}", harness_name(harness), interaction.title),
        None => interaction.title.clone(),
    };
    if label.chars().count() <= LABEL_MAX {
        return label;
    }
    let cut: String = label.chars().take(LABEL_MAX - 1).collect();
    format!("{cut}…")
}

fn harness_name(harness: Harness) -> &'static str {
    match harness {
        Harness::Claude => "Claude Code",
        Harness::Codex => "Codex",
    }
}

/// What the run shows after a change: the most urgent interaction that waits with a notice, else
/// the end of a turn that nobody saw.
fn refresh_run(state: &mut State, host: &dyn Host, run: RunId) {
    let waiting: Vec<&Interaction> = state
        .interactions
        .open()
        .into_iter()
        .filter(|i| i.run == Some(run) && i.state == InteractionState::Open)
        .filter(|i| state.native.noticed.contains(&i.id))
        .collect();
    let current = state.native.notices.get(&run).cloned();
    let text = match waiting.first() {
        Some(top) => Some(describe(
            state,
            top,
            waiting.len(),
            host.notification_capabilities(),
        )),
        None => turn_ended(state, run),
    };
    let Some(text) = text else {
        if let Some(notice) = state.native.notices.remove(&run) {
            host.withdraw_notification(notice.id);
        }
        return;
    };
    match current {
        Some(notice) => {
            if notice.shown != text {
                host.update_notification(notice.id, text.clone());
                state.native.notices.insert(
                    run,
                    Notice {
                        id: notice.id,
                        shown: text,
                    },
                );
            }
        }
        None => post_or_wait(state, host, run, text),
    }
}

fn post_or_wait(state: &mut State, host: &dyn Host, run: RunId, text: HostNotification) {
    let now = now_ms();
    let window = i64::try_from(state.native.window.as_millis()).unwrap_or(i64::MAX);
    let wait = state
        .native
        .posted
        .get(&run)
        .map_or(0, |at| (at + window - now).max(0));
    if wait > 0 {
        if state.native.armed.insert(run)
            && let Some(clock) = state.native.clock.clone()
        {
            let delay = Duration::from_millis(u64::try_from(wait).unwrap_or(0));
            clock.after(delay, move |parts| {
                parts.state.native.armed.remove(&run);
                if let Some(host) = parts.state.host.clone() {
                    refresh_run(parts.state, host.as_ref(), run);
                }
            });
        }
        return;
    }
    let id = host.notify(text.clone());
    state.native.posted.insert(run, now);
    state.native.notices.insert(run, Notice { id, shown: text });
}

/// The notification of a run whose turn ended while nobody looked: the harness and
/// the work of the run, with Open. `None` when the run does not wait or the setting is off.
fn turn_ended(state: &State, run: RunId) -> Option<HostNotification> {
    let run = state.runs.by_id.get(&run)?;
    run.turn.ended?;
    if !state.config.config().agents.turn_end_notice {
        return None;
    }
    Some(HostNotification {
        title: format!("{} finished", harness_name(run.harness)),
        body: work(state, run),
        urgency: Urgency::Normal,
        interaction: None,
        pane: run.pane,
        actions: Vec::new(),
    })
}

/// What a run works on, in the words of the monitor: the title of its pane, else the name of its
/// harness.
fn work(state: &State, run: &crate::agents::runs::Run) -> String {
    run.pane
        .and_then(|pane| state.layout.panes.get(&pane))
        .and_then(|entry| entry.facts.title.as_deref())
        .filter(|title| !title.is_empty())
        .map_or_else(
            || harness_name(run.harness).to_owned(),
            |title| nh_rich::visible(title).into_owned(),
        )
}

/// The words of a notification. The urgency follows the risk. Only an
/// approval that a client can answer has the buttons, and only when the platform offers them.
fn describe(
    state: &State,
    top: &Interaction,
    waiting: usize,
    caps: NotificationCapabilities,
) -> HostNotification {
    let who = top
        .run
        .and_then(|run| state.runs.by_id.get(&run))
        .map_or("An agent", |run| harness_name(run.harness));
    let what = match top.kind {
        InteractionKind::Approval => "asks for permission",
        InteractionKind::Question => "has a question",
        InteractionKind::PlanReview => "has a plan to review",
        InteractionKind::Notice => "has a notice",
    };
    let body = if waiting > 1 {
        format!("{} (and {} more)", top.title, waiting - 1)
    } else {
        top.title.clone()
    };
    let actions = if caps.actions && top.kind == InteractionKind::Approval && top.answerable {
        vec![NotificationAction::Allow, NotificationAction::Deny]
    } else {
        Vec::new()
    };
    HostNotification {
        title: format!("{who} {what}"),
        body,
        urgency: interaction_urgency(top.risk),
        interaction: Some(top.id),
        pane: None,
        actions,
    }
}

pub(crate) mod push;

#[cfg(test)]
mod tests;
