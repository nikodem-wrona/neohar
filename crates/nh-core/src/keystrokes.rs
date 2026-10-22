//! Answers by verified keystrokes.
//!
//! The verifier of `nh-agents` decides each key. This module is the I/O around it: it reads the
//! screen of the pane, sends the key, waits for the screen to change, and reads again. The
//! sequence holds the input of the pane. A person who types in the pane ends it, and so does
//! other input that waits too long.

use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex, OnceLock, PoisonError},
    time::{Duration, Instant},
};

use nh_agents::{
    keystroke::{
        CONFIRM_WAIT_MS, Choice, DAMAGE_WAIT_MS, Failure, Key, Next, Outcome, Reading, Request,
        Verifier,
    },
    screen::{self, Detector, Dialog},
};
use nh_proto::api::{
    calls::agent::{Behavior, Decision},
    ids::InteractionId,
    model::{InteractionPayload, QuestionPayload},
};
use tokio::{runtime::Handle, sync::watch};
use ulid::Ulid;

use crate::{
    actor::ActorHandle,
    interaction::{apply, model::Interaction},
    pane::{KeystrokeLock, Pane},
};

/// What the driver needs from a terminal.
pub(crate) trait Terminal {
    /// The dialog on the screen now, when a rule reads it and keys may answer it.
    fn dialog(&mut self) -> Option<Dialog>;
    fn send(&mut self, key: Key) -> Result<(), String>;
    fn paste(&mut self, text: &str) -> Result<(), String>;
    /// A person typed, or other input waited too long.
    fn interrupted(&self) -> bool;
    /// Waits for the screen to change for at most `max`. Says whether it changed. A change that
    /// came before the call does not count.
    fn damage(&mut self, max: Duration) -> impl Future<Output = bool> + Send;
}

/// Whether a key can commit an answer: Enter, and a digit that picks an option.
fn may_commit(key: Key) -> bool {
    matches!(key, Key::Enter | Key::Char(_))
}

/// Runs the sequence of a request on a terminal. `on_commit` is called before a key that can
/// commit the answer, so that the state of the interaction knows before the program does.
pub(crate) async fn run<T: Terminal>(
    term: &mut T,
    request: Request,
    mut on_commit: impl FnMut() + Send,
) -> Result<Outcome, Failure> {
    let mut verifier = Verifier::new(request);
    let mut reading = Reading {
        dialog: term.dialog(),
        damaged: true,
    };
    let confirm = Duration::from_millis(CONFIRM_WAIT_MS);
    let mut confirm_by: Option<Instant> = None;
    // A sequence has a limit of its own, so that a program that keeps changing cannot hold the
    // pane for ever.
    let started = Instant::now();
    loop {
        if term.interrupted() {
            return Err(Failure::UserTyped);
        }
        if started.elapsed() > Duration::from_secs(10) {
            return Err(Failure::StepFailed);
        }
        let wait = match verifier.observe(&reading) {
            Next::Done(end) => return end,
            Next::Send(keys) => {
                for key in keys {
                    if may_commit(key) {
                        on_commit();
                    }
                    term.send(key).map_err(|_| Failure::StepFailed)?;
                }
                Duration::from_millis(DAMAGE_WAIT_MS)
            }
            Next::Paste(text) => {
                term.paste(&text).map_err(|_| Failure::StepFailed)?;
                Duration::from_millis(DAMAGE_WAIT_MS)
            }
            Next::Commit => {
                on_commit();
                term.send(Key::Enter).map_err(|_| Failure::StepFailed)?;
                let by = Instant::now() + confirm;
                confirm_by = Some(by);
                by - Instant::now()
            }
            Next::Wait => {
                confirm_by.map_or(confirm, |by| by.saturating_duration_since(Instant::now()))
            }
        };
        let changed = term.damage(wait).await;
        let over = confirm_by.is_some_and(|by| Instant::now() >= by);
        reading = Reading {
            dialog: term.dialog(),
            // After the last time of the wait, a screen that did not change ends the sequence.
            damaged: changed && !over,
        };
    }
}

// --- the pane -----------------------------------------------------------------------------------

/// The pane as a terminal: the lock on its input, the detector, and the damage channel.
struct PaneTerminal {
    pane: Arc<Pane>,
    lock: KeystrokeLock,
    detector: Detector,
    damage: watch::Receiver<u64>,
}

impl Terminal for PaneTerminal {
    fn dialog(&mut self) -> Option<Dialog> {
        let (_, rows) = self.pane.size();
        let screen = self.pane.screen_rows(self.detector.rows_needed(rows));
        let found = self.detector.evaluate(&screen, Instant::now());
        found
            .dialog()
            .filter(|m| m.keystroke_answerable)
            .and_then(|m| m.dialog.clone())
    }

    fn send(&mut self, key: Key) -> Result<(), String> {
        self.damage.mark_unchanged();
        self.lock.send(key).map_err(|e| e.to_string())
    }

    fn paste(&mut self, text: &str) -> Result<(), String> {
        self.damage.mark_unchanged();
        self.lock.paste(text).map_err(|e| e.to_string())
    }

    fn interrupted(&self) -> bool {
        self.lock.interrupted()
    }

    async fn damage(&mut self, max: Duration) -> bool {
        matches!(
            tokio::time::timeout(max, self.damage.changed()).await,
            Ok(Ok(()))
        )
    }
}

// --- the request of an interaction -----------------------------------------------------------

/// What to type for the decision of an interaction, or why nothing can be typed.
pub(crate) fn request_for(interaction: &Interaction) -> Result<Request, &'static str> {
    let named = |name: &str| {
        Ok(Request {
            expected: Vec::new(),
            choice: Choice::Named(name.to_owned()),
        })
    };
    match (&interaction.payload, &interaction.delivery.decision) {
        (InteractionPayload::Approval(_), Some(Decision::Approval(d))) => {
            match (d.behavior, d.always) {
                (Behavior::Allow, true) => named("allow_always"),
                (Behavior::Allow, false) => named("allow"),
                (Behavior::Deny, _) => named("deny"),
            }
        }
        (InteractionPayload::PlanReview(_), Some(Decision::PlanReview(d))) => {
            match (d.approve, &d.feedback) {
                (true, _) => named("allow"),
                (false, Some(text)) if !text.trim().is_empty() => Ok(Request {
                    expected: Vec::new(),
                    choice: Choice::Text {
                        option: "@deny".to_owned(),
                        text: text.clone(),
                    },
                }),
                (false, _) => named("deny"),
            }
        }
        (InteractionPayload::Question(payload), Some(Decision::Question(d))) => {
            question_request(payload, d)
        }
        _ => Err("no_decision"),
    }
}

fn question_request(
    payload: &QuestionPayload,
    decision: &nh_proto::api::calls::agent::QuestionDecision,
) -> Result<Request, &'static str> {
    // The dialog of a harness shows one question at a time.
    let ([question], [answer]) = (payload.questions.as_slice(), decision.answers.as_slice()) else {
        return Err("several_questions");
    };
    let choice = match (&answer.text, answer.selected.as_slice()) {
        (Some(text), []) => Choice::Text {
            option: "@other".to_owned(),
            text: text.clone(),
        },
        (None, [one]) if !question.multi_select => Choice::Option(one.clone()),
        (None, many) if question.multi_select && !many.is_empty() => Choice::Toggle(many.to_vec()),
        _ => return Err("answer_not_typable"),
    };
    Ok(Request {
        expected: Vec::new(),
        choice,
    })
}

// --- the service ---------------------------------------------------------------------------------

/// Starts the sequences of keys. One task for each sequence.
pub struct Keystrokes {
    handle: Handle,
    panes: Arc<Mutex<HashMap<Ulid, Arc<Pane>>>>,
    actor: OnceLock<ActorHandle>,
}

impl Keystrokes {
    pub fn new(handle: Handle, panes: Arc<Mutex<HashMap<Ulid, Arc<Pane>>>>) -> Arc<Self> {
        Arc::new(Self {
            handle,
            panes,
            actor: OnceLock::new(),
        })
    }

    pub fn bind(&self, actor: ActorHandle) {
        let _ = self.actor.set(actor);
    }

    /// Types the answer of an interaction into its pane. The end goes to the actor.
    pub(crate) fn start(
        self: &Arc<Self>,
        id: InteractionId,
        pane: Ulid,
        manifest: String,
        request: Request,
    ) {
        let Some(actor) = self.actor.get().cloned() else {
            return;
        };
        let service = Arc::clone(self);
        self.handle.spawn(async move {
            let end = service.sequence(&actor, id, pane, &manifest, request).await;
            let confirm = matches!(end, Ok(Outcome::DialogGone));
            actor.update_detached(move |mut parts| apply::keystrokes_ended(&mut parts, id, end));
            if confirm {
                // The dialog is gone. A hook or the screen must say that the answer took effect.
                tokio::time::sleep(Duration::from_millis(CONFIRM_WAIT_MS)).await;
                actor.update_detached(move |mut parts| {
                    apply::keystrokes_unconfirmed(&mut parts, id)
                });
            }
        });
    }

    async fn sequence(
        &self,
        actor: &ActorHandle,
        id: InteractionId,
        pane: Ulid,
        manifest: &str,
        request: Request,
    ) -> Result<Outcome, Failure> {
        let found = self
            .panes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&pane)
            .cloned();
        let Some(pane) = found else {
            return Err(Failure::NoDialog);
        };
        let Some(compiled) = screen::builtin(manifest) else {
            return Err(Failure::Unsupported);
        };
        let Some(lock) = pane.lock_keystrokes() else {
            return Err(Failure::StepFailed);
        };
        let damage = pane.subscribe_damage();
        let mut terminal = PaneTerminal {
            pane,
            lock,
            detector: Detector::new(Arc::new(compiled)),
            damage,
        };
        let actor = actor.clone();
        run(&mut terminal, request, move || {
            actor.update_detached(move |mut parts| apply::keystrokes_committed(&mut parts, id));
        })
        .await
    }
}

#[cfg(test)]
mod tests;
