//! `interaction.list`, `.show`, `.next`, `.answer` and `.dismiss`.
//!
//! The reads come from the table in memory. The two writes follow the pattern of a command: the
//! handler lets the model make the transition, writes the rows and the events in its transaction,
//! and returns a mutation that puts the result in the state, the projections and the gates after
//! the commit.

use nh_proto::api::{
    calls::agent::{
        Behavior, Decision, InteractionAnswered, InteractionListResult, InteractionNextResult,
        InteractionRevealed, InteractionShown,
    },
    common::Via,
    events::{EventPayload, PolicyEvent},
    ids::{InteractionId, PaneId, Ref, RunId, WindowId},
    methods::{Call, Method},
    model::Reveal,
};
use nh_store::Tx;
use serde_json::{Value, json, to_value};

use super::{
    apply,
    model::{Interaction, Refusal},
    rows,
};
use crate::{
    ErrorCode,
    actor::{Ctx, Handled, HandlerTable, Origin, Scope},
    error::RpcError,
    persist::now_ms,
};

fn internal(error: impl std::fmt::Display) -> RpcError {
    RpcError::internal(error.to_string())
}

fn not_found(what: impl std::fmt::Display) -> RpcError {
    RpcError::new(ErrorCode::NotFound, format!("no interaction {what}"))
}

/// Who answers, for the delivery record: the window of the app, a client of the CLI, or a paired
/// phone.
fn client_name(origin: &Origin) -> String {
    match origin.scope {
        Scope::Ui => origin
            .window
            .map_or_else(|| "ui".to_owned(), |window| window.to_string()),
        Scope::Cli | Scope::Dev => format!("cli:{}", origin.client),
        Scope::Pane => "pane".to_owned(),
        Scope::Phone => origin
            .device
            .map_or_else(|| "phone".to_owned(), |device| format!("phone:{device}")),
    }
}

/// The surface of an answer for the delivery record. The server sets it from the scope of the
/// caller: a client of the CLI is `cli` and a phone is `phone`, whatever it says. Only the UI says
/// which of its surfaces it was (a card, a notification, the tray or the palette). No client is
/// `policy`: only a rule of the policy is.
fn via_of(origin: &Origin, claimed: Via) -> Via {
    match origin.scope {
        Scope::Ui => match claimed {
            Via::Card | Via::Notification | Via::Tray | Via::Palette => claimed,
            Via::Cli | Via::Policy | Via::Phone => Via::Card,
        },
        Scope::Phone => Via::Phone,
        Scope::Cli | Scope::Dev | Scope::Pane => Via::Cli,
    }
}

fn find(ctx: &Ctx<'_>, reference: &Ref<InteractionId>) -> Result<InteractionId, RpcError> {
    ctx.state.interactions.resolve(reference).ok_or_else(|| {
        not_found(match &reference.0 {
            nh_proto::api::ids::RefValue::Id(id) => id.to_string(),
            nh_proto::api::ids::RefValue::Handle(_) => "with this handle".to_owned(),
            nh_proto::api::ids::RefValue::Name(name) => name.clone(),
        })
    })
}

/// The error for a request that the interaction refuses.
pub(crate) fn refuse(refusal: Refusal, interaction: &Interaction) -> RpcError {
    match refusal {
        Refusal::AlreadyAnswered { by, via, decision } => {
            let mut error = RpcError::new(
                ErrorCode::AlreadyAnswered,
                format!("{} has a decision already", interaction.handle),
            );
            error.details = json!({"by": by, "via": via, "decision": decision});
            error
        }
        Refusal::Closed(state) => {
            let mut error = RpcError::new(
                ErrorCode::InteractionClosed,
                format!("{} ended: {}", interaction.handle, rows::name(&state)),
            );
            error.details = json!({"state": rows::name(&state)});
            error
        }
        Refusal::NotAnswerable => RpcError::new(
            ErrorCode::NotAnswerable,
            format!(
                "{} cannot be answered: the run is not bound to its pane, or no channel reaches it",
                interaction.handle
            ),
        ),
        Refusal::Invalid(message) => RpcError::invalid_params(message),
    }
}

fn list(ctx: &Ctx<'_>, _: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::InteractionList(p) = call else {
        return Err(internal("not interaction.list"));
    };
    let run: Option<RunId> = match &p.run {
        Some(reference) => match &reference.0 {
            nh_proto::api::ids::RefValue::Id(id) => Some(*id),
            _ => return Err(RpcError::new(ErrorCode::NotFound, "no such run")),
        },
        None => None,
    };
    let pane: Option<PaneId> = match &p.pane {
        Some(reference) => Some(
            ctx.state
                .layout
                .resolve_pane(reference)
                .map_err(|e| RpcError::new(ErrorCode::NotFound, e.to_string()))?,
        ),
        None => None,
    };
    let table = &ctx.state.interactions;
    let all: Vec<&Interaction> = if p.open_only {
        table.open()
    } else {
        table.open().into_iter().chain(table.ended()).collect()
    };
    let limit = p
        .limit
        .map_or(usize::MAX, |n| usize::try_from(n).unwrap_or(usize::MAX));
    let interactions = all
        .into_iter()
        .filter(|i| run.is_none_or(|r| i.run == Some(r)))
        .filter(|i| pane.is_none_or(|p| i.pane == Some(p)))
        .take(limit)
        .map(Interaction::value)
        .collect();
    let result = to_value(InteractionListResult { interactions }).map_err(internal)?;
    Ok(Handled::result(result))
}

fn show(ctx: &Ctx<'_>, _: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::InteractionShow(p) = call else {
        return Err(internal("not interaction.show"));
    };
    let id = find(ctx, &p.interaction)?;
    let interaction = ctx
        .state
        .interactions
        .get(id)
        .ok_or_else(|| not_found(id))?
        .value();
    let result = to_value(InteractionShown { interaction }).map_err(internal)?;
    Ok(Handled::result(result))
}

fn next(ctx: &Ctx<'_>, _: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::InteractionNext(p) = call else {
        return Err(internal("not interaction.next"));
    };
    let after = match &p.after {
        Some(reference) => Some(find(ctx, reference)?),
        None => None,
    };
    let found = ctx.state.interactions.next(after);
    let result = to_value(InteractionNextResult {
        interaction: found.map(|i| i.id),
        handle: found.map(|i| i.handle.clone()),
    })
    .map_err(internal)?;
    Ok(Handled::result(result))
}

fn answer(ctx: &Ctx<'_>, tx: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::InteractionAnswer(p) = call else {
        return Err(internal("not interaction.answer"));
    };
    let id = find(ctx, &p.interaction)?;
    let before = ctx
        .state
        .interactions
        .get(id)
        .ok_or_else(|| not_found(id))?;
    let mut interaction = before.clone();
    let via = via_of(ctx.origin, p.via);
    let step = interaction
        .answer(p.decision.clone(), &client_name(ctx.origin), via)
        .map_err(|refusal| {
            // A click on a notification that came too late shows who answered.
            if let Some(host) = &ctx.state.host {
                crate::native::late_action(host, ctx.origin.device, before, &refusal, via);
            }
            refuse(refusal, before)
        })?;
    rows::write(tx, &interaction, &step.events).map_err(internal)?;
    // The answered approval gets its fingerprint, and "Allow always" makes a session rule.
    let at = nh_proto::api::time::Timestamp(ctx.now_ms);
    if let Some(row) = apply::fingerprint_row(&interaction, at) {
        tx.put_fingerprint(&row).map_err(internal)?;
    }
    let always = match &interaction.delivery.decision {
        Some(Decision::Approval(d)) => d.always && d.behavior == Behavior::Allow,
        _ => false,
    };
    let session = if always {
        interaction
            .facts
            .as_ref()
            .and_then(|facts| crate::policy::session_rule(facts, interaction.risk, at))
    } else {
        None
    };
    if let Some(rule) = &session {
        tx.put_policy_rule(&crate::policy::to_row(rule))
            .map_err(internal)?;
        tx.append(
            Some(&rule.id),
            &EventPayload::PolicyAdded(PolicyEvent {
                id: rule.id.clone(),
                scope: rule.scope,
            }),
        )
        .map_err(internal)?;
    }
    let result = to_value(InteractionAnswered {
        rev: interaction.delivery.rev,
        state: interaction.state,
    })
    .map_err(internal)?;
    Ok(Handled {
        result,
        mutation: Some(Box::new(move |state, hub| {
            apply::in_state(state, hub, interaction, &step);
            if let Some(rule) = session {
                state.policy.put(rule);
            }
        })),
    })
}

fn dismiss(ctx: &Ctx<'_>, tx: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::InteractionDismiss(p) = call else {
        return Err(internal("not interaction.dismiss"));
    };
    let id = find(ctx, &p.interaction)?;
    let before = ctx
        .state
        .interactions
        .get(id)
        .ok_or_else(|| not_found(id))?;
    let mut interaction = before.clone();
    let step = interaction
        .dismiss(nh_proto::api::time::Timestamp(now_ms()))
        .map_err(|refusal| refuse(refusal, before))?;
    rows::write(tx, &interaction, &step.events).map_err(internal)?;
    let run = interaction.run;
    Ok(Handled {
        result: Value::Object(serde_json::Map::new()),
        mutation: Some(Box::new(move |state, hub| {
            let gates = state.gates.clone();
            apply::in_state(state, hub, interaction, &step);
            // The arbiter learns that the interaction ended, on the actor, after this command.
            if let (Some(gates), Some(run)) = (gates, run) {
                gates.post(move |mut parts| apply::arbiter_closed(&mut parts, run, id));
            }
        })),
    })
}

/// The window that shows a card when the caller names none: the one with the focus of the OS,
/// else the first one.
/// The window that shows a revealed interaction: a window of the client that asked when it has
/// one, the focused one first.
fn default_window(ctx: &Ctx<'_>) -> Option<WindowId> {
    let all = &ctx.state.layout.windows;
    let mine: Vec<&crate::layout::model::Window> = match ctx.origin.device {
        Some(device) => all.iter().filter(|w| w.client == Some(device)).collect(),
        None => Vec::new(),
    };
    let windows: Vec<&crate::layout::model::Window> = if mine.is_empty() {
        all.iter().collect()
    } else {
        mine
    };
    windows
        .iter()
        .find(|w| w.facts.focused)
        .or_else(|| windows.first())
        .map(|w| w.id)
}

fn reveal(ctx: &Ctx<'_>, _: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::InteractionReveal(p) = call else {
        return Err(internal("not interaction.reveal"));
    };
    let id = match &p.interaction {
        Some(reference) => Some(find(ctx, reference)?),
        None => ctx.state.interactions.next(None).map(|i| i.id),
    };
    let window = p.window.or_else(|| default_window(ctx));
    if let Some(window) = window
        && ctx.state.layout.window_index(window).is_none()
    {
        return Err(RpcError::new(ErrorCode::NotFound, "no such window"));
    }
    let result = to_value(InteractionRevealed {
        interaction: id,
        window,
    })
    .map_err(internal)?;
    let (Some(id), Some(window)) = (id, window) else {
        return Ok(Handled::result(result));
    };
    Ok(Handled {
        result,
        mutation: Some(Box::new(move |state, hub| {
            let seq = state.native.next_reveal();
            if let Some(index) = state.layout.window_index(window) {
                state.layout.windows[index].reveal = Some(Reveal {
                    interaction: id,
                    seq,
                });
                crate::layout::project::publish_window(hub, &state.layout, window);
            }
            if let Some(host) = state.host.clone() {
                crate::host::Host::focus_window(host.as_ref(), window);
            }
        })),
    })
}

/// The handlers of this module.
pub(crate) fn handlers(table: &mut HandlerTable) {
    table.insert(Method::InteractionList, list);
    table.insert(Method::InteractionShow, show);
    table.insert(Method::InteractionNext, next);
    table.insert(Method::InteractionReveal, reveal);
    table.insert(Method::InteractionAnswer, answer);
    table.insert(Method::InteractionDismiss, dismiss);
}
