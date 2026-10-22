//! Interactions that the screen opens.
//!
//! The screen is the fallback: when the hooks say nothing, a rule of the screen manifest can still
//! see a dialog. The interaction is provisional. It is open in the mode `observe`, and it ends
//! when the dialog is gone from the screen. A structured interaction of the same kind takes its
//! place. A client can answer it only with keys, and only when the rule read the dialog and the
//! run is bound with the capability `answer_keystroke`.

use nh_agents::{
    arbiter::{Input, Observation},
    screen::ScreenMatch,
};
use nh_proto::api::{
    common::{Risk, Source as ArbiterSource},
    ids::{InteractionId, RunId},
    model::{
        AnswerChannel, ApprovalPayload, GateMode, InteractionKind, InteractionPayload,
        InteractionState, PlanPayload, Question, QuestionOption, QuestionPayload,
    },
    rich::{Command, Markdown, Spans},
    time::Timestamp,
};

use super::{
    apply::{self, transition},
    model::{Interaction, OpenSpec, Source},
    project, rows,
};
use crate::{actor::Parts, agents::runs, ids::new_interaction_id};

/// The title and the payload of a dialog that the screen showed.
fn render(kind: InteractionKind, found: &ScreenMatch) -> (String, InteractionPayload) {
    let capture = |name: &str| found.captures.get(name).cloned();
    let labels: Vec<String> = found
        .dialog
        .as_ref()
        .map(|d| d.options.iter().map(|o| o.label.clone()).collect())
        .unwrap_or_default();
    match kind {
        InteractionKind::Approval => {
            let command = capture("command");
            let title = match (&command, capture("title")) {
                (Some(line), _) => format!("Run `{}`", one_line(line)),
                (None, Some(title)) => title,
                (None, None) => "The harness asks for approval".to_owned(),
            };
            let payload = InteractionPayload::Approval(Box::new(ApprovalPayload {
                tool: "screen".to_owned(),
                command: command.map(|line| Command {
                    argv: line.split_whitespace().map(str::to_owned).collect(),
                    display: Spans {
                        text: line,
                        spans: Vec::new(),
                    },
                    cwd: String::new(),
                    risk: Risk::Unknown,
                    reasons: vec!["neohar read this command from the screen.".to_owned()],
                    policy_hint: None,
                }),
                cwd: None,
                paths: Vec::new(),
                diff: None,
                input: None,
                peek: Vec::new(),
                policy_note: None,
            }));
            (title, payload)
        }
        InteractionKind::PlanReview => (
            "The harness asks you to review its plan".to_owned(),
            InteractionPayload::PlanReview(PlanPayload {
                plan: Markdown {
                    html: String::new(),
                    truncated: false,
                },
                plan_path: capture("plan_path"),
            }),
        ),
        _ => {
            let question = capture("hint")
                .or_else(|| capture("title"))
                .unwrap_or_else(|| "The harness asks a question".to_owned());
            let payload = InteractionPayload::Question(QuestionPayload {
                questions: vec![Question {
                    question: question.clone(),
                    header: capture("title"),
                    options: labels
                        .into_iter()
                        .map(|label| QuestionOption {
                            label,
                            description: None,
                        })
                        .collect(),
                    multi_select: found
                        .dialog
                        .as_ref()
                        .is_some_and(|d| d.multi_toggle.is_some()),
                }],
                context: None,
            });
            (question, payload)
        }
    }
}

fn one_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    let mut out: String = line.chars().take(80).collect();
    if line.chars().count() > 80 || text.trim().lines().count() > 1 {
        out.push('…');
    }
    out
}

/// The screen shows a dialog. If the run has no interaction of that kind open, a provisional one
/// opens.
pub(crate) fn opened(parts: &mut Parts<'_>, run_id: RunId, found: &ScreenMatch, at: Timestamp) {
    let Some(kind) = found.opens else {
        return;
    };
    let Some(run) = parts.state.runs.by_id.get(&run_id) else {
        return;
    };
    if !run.live() {
        return;
    }
    // An interaction of this kind is open: the screen shows the dialog of that one.
    if parts
        .state
        .interactions
        .open_of_run(run_id)
        .iter()
        .any(|i| i.kind == kind)
    {
        return;
    }
    let pane = run.pane;
    // A key can answer the dialog of a bound run whose screen manifest the live screen validates,
    // when a rule read the dialog.
    let run_keys = matches!(run.binding, nh_proto::api::model::Binding::Bound)
        && run.capabilities.answer_keystroke;
    let (title, payload) = render(kind, found);
    let spec = OpenSpec {
        kind,
        run: Some(run_id),
        pane,
        native_ref: None,
        title,
        risk: Risk::Unknown,
        payload,
        mode: GateMode::Observe,
        answerable: found.keystroke_answerable && run_keys,
        channels: if found.keystroke_answerable && run_keys {
            vec![AnswerChannel::Keystroke]
        } else {
            Vec::new()
        },
        source: Source::Screen,
        confidence: f64::from(found.confidence),
        deadline: None,
        facts: None,
        by_policy: false,
        keystroke: found.keystroke_answerable && run_keys,
    };
    let Ok(id) = new_interaction_id() else {
        return;
    };
    let stored = parts.commit(|tx| {
        let number = tx.next_handle("interaction")?;
        let (interaction, step) = Interaction::open(id, format!("i{number}"), spec, at);
        rows::write(tx, &interaction, &step.events)?;
        Ok(interaction)
    });
    let interaction = match stored {
        Ok(interaction) => interaction,
        Err(error) => {
            tracing::warn!(run = %run_id, %error, "a provisional interaction is not stored");
            return;
        }
    };
    let evicted = parts.state.interactions.put(interaction);
    if let Some(interaction) = parts.state.interactions.get(id) {
        project::publish(
            parts.projections,
            &parts.state.interactions,
            interaction,
            &evicted,
        );
    }
    crate::native::changed(parts.state, id, true);
    runs::input(
        parts,
        run_id,
        Input::new(ArbiterSource::Screen, at, Observation::Opened { id, kind })
            .with_confidence(found.confidence),
    );
    runs::publish(parts, run_id);
}

/// The screen shows no dialog any more: the provisional interactions of the run end.
pub(crate) fn gone(parts: &mut Parts<'_>, run_id: RunId, at: Timestamp) {
    let from_screen: Vec<(InteractionId, InteractionState)> = parts
        .state
        .interactions
        .open_of_run(run_id)
        .iter()
        .filter(|i| i.source == Source::Screen)
        .map(|i| (i.id, i.state))
        .collect();
    for (id, state) in from_screen {
        match state {
            InteractionState::Open => {
                transition(parts, id, |interaction| {
                    let step = interaction.cancel("screen_gone", at);
                    ((), step)
                });
            }
            // The keys went in and the dialog went: that is the resolution.
            InteractionState::Delivering => {
                transition(parts, id, |interaction| {
                    let step = interaction.screen_resolved(at);
                    ((), step)
                });
            }
            _ => continue,
        }
        apply::arbiter_closed_by_screen(parts, run_id, id, at);
    }
}
