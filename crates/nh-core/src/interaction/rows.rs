//! The interaction as a row of the store, and back.

use nh_proto::api::{
    calls::agent::Decision,
    common::{Risk, Via},
    events::EventPayload,
    ids::{InteractionId, PaneId, RunId},
    model::{AnswerChannel, GateMode, InteractionKind, InteractionState},
    time::Timestamp,
};
use nh_store::{DeliveryRow, InteractionRow, StoreError, Tx};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use super::model::{Attempt, Delivery, Interaction, Source};

/// The name of a value of the API in the words of the store (`snake_case`).
pub(crate) fn name<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(Value::String(text)) => text,
        _ => String::new(),
    }
}

fn parse<T: DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_value(Value::String(text.to_owned())).ok()
}

/// The outcomes of an attempt that a row can hold. The model keeps them as static words.
fn outcome(text: &str) -> &'static str {
    match text {
        "sent" => "sent",
        "acked" => "acked",
        "not_applied" => "not_applied",
        "resumed" => "resumed",
        "lost" => "lost",
        "released" => "released",
        "deadline" => "deadline",
        _ => "unknown",
    }
}

fn attempts_json(attempts: &[Attempt]) -> Value {
    Value::Array(
        attempts
            .iter()
            .map(|attempt| {
                json!({
                    "at": attempt.at.0,
                    "channel": name(&attempt.channel),
                    "via": attempt.via.map(|via| name(&via)),
                    "outcome": attempt.outcome,
                })
            })
            .collect(),
    )
}

fn attempts_from(value: &Value) -> Vec<Attempt> {
    value
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|item| {
                    Some(Attempt {
                        at: Timestamp(item.get("at")?.as_i64()?),
                        channel: parse(item.get("channel")?.as_str()?)?,
                        via: item.get("via").and_then(Value::as_str).and_then(parse),
                        outcome: outcome(item.get("outcome")?.as_str()?),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The rows of an interaction.
pub(crate) fn to_rows(interaction: &Interaction) -> (InteractionRow, DeliveryRow) {
    let delivery = &interaction.delivery;
    let row = InteractionRow {
        id: interaction.id.ulid(),
        handle: interaction.handle.clone(),
        run: interaction.run.map(RunId::ulid),
        pane: interaction.pane.map(PaneId::ulid),
        kind: name(&interaction.kind),
        native_ref: interaction.native_ref.clone(),
        title: interaction.title.clone(),
        risk: name(&interaction.risk),
        payload: serde_json::to_value(&interaction.payload).unwrap_or(Value::Null),
        state: name(&interaction.state),
        answerable: interaction.answerable,
        gate_mode: name(&interaction.mode),
        source: interaction.source.as_str().to_owned(),
        confidence: interaction.confidence,
        opened_at: interaction.opened_at.0,
        deadline: interaction.deadline.map(|t| t.0),
        resolved_at: interaction.resolved_at.map(|t| t.0),
        // The channels are part of how the interaction was opened. They go with the resolution.
        resolution: Some(json!({
            "channels": interaction.channels.iter().map(name).collect::<Vec<_>>(),
            "keystroke": interaction.keystroke,
            "fail_reason": delivery.fail_reason,
        })),
        // The column holds the facts of the approval as JSON: the fingerprint and what it stands for.
        fingerprint: interaction
            .facts
            .as_ref()
            .and_then(|facts| serde_json::to_string(facts).ok()),
    };
    let record = DeliveryRow {
        interaction: interaction.id.ulid(),
        native_ref: interaction.native_ref.clone(),
        rev: delivery.rev,
        decision: delivery
            .decision
            .as_ref()
            .and_then(|d| serde_json::to_value(d).ok()),
        decided_by: delivery.decided_by.clone(),
        via: delivery.via.map(|via| name(&via)),
        key: delivery.key.clone(),
        channel: delivery.channel.map(|channel| name(&channel)),
        lease: delivery.lease.map(|lease| {
            json!({
                "holder": lease.holder.as_str(),
                "conn": lease.conn,
                "pid": lease.pid,
                "expires_at": lease.expires_at.0,
            })
        }),
        deadline: interaction.deadline.map(|t| t.0),
        state: name(&interaction.state),
        attempts: attempts_json(&delivery.attempts),
    };
    (row, record)
}

/// An interaction from its rows, or `None` for a row that this version cannot read. A lease does
/// not outlive the core, so the interaction comes back without one.
pub(crate) fn from_rows(row: &InteractionRow, record: Option<&DeliveryRow>) -> Option<Interaction> {
    let kind: InteractionKind = parse(&row.kind)?;
    let channels: Vec<AnswerChannel> = row
        .resolution
        .as_ref()
        .and_then(|r| r.get("channels"))
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .filter_map(parse)
                .collect()
        })
        .unwrap_or_default();
    let mut delivery = Delivery::default();
    if let Some(record) = record {
        delivery.rev = record.rev;
        delivery.decision = record
            .decision
            .clone()
            .and_then(|d| serde_json::from_value::<Decision>(d).ok());
        delivery.decided_by = record.decided_by.clone();
        delivery.via = record.via.as_deref().and_then(parse::<Via>);
        delivery.key = record.key.clone();
        delivery.channel = record.channel.as_deref().and_then(parse);
        delivery.attempts = attempts_from(&record.attempts);
    }
    delivery.fail_reason = row
        .resolution
        .as_ref()
        .and_then(|r| r.get("fail_reason"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    Some(Interaction {
        id: InteractionId::new(row.id),
        handle: row.handle.clone(),
        kind,
        run: row.run.map(RunId::new),
        pane: row.pane.map(PaneId::new),
        native_ref: row.native_ref.clone(),
        title: row.title.clone(),
        risk: parse::<Risk>(&row.risk)?,
        payload: serde_json::from_value(row.payload.clone()).ok()?,
        state: parse::<InteractionState>(&row.state)?,
        mode: parse::<GateMode>(&row.gate_mode)?,
        answerable: row.answerable,
        channels,
        source: Source::parse(&row.source)?,
        confidence: row.confidence,
        opened_at: Timestamp(row.opened_at),
        deadline: row.deadline.map(Timestamp),
        resolved_at: row.resolved_at.map(Timestamp),
        delivery,
        facts: row
            .fingerprint
            .as_deref()
            .and_then(|text| serde_json::from_str(text).ok()),
        keystroke: row
            .resolution
            .as_ref()
            .and_then(|r| r.get("keystroke"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Writes the interaction and its events in the transaction.
pub(crate) fn write(
    tx: &mut Tx<'_>,
    interaction: &Interaction,
    events: &[EventPayload],
) -> Result<(), StoreError> {
    let (row, record) = to_rows(interaction);
    tx.put_interaction(&row)?;
    tx.put_delivery(&record)?;
    let subject = interaction.id.to_string();
    for payload in events {
        tx.append(Some(&subject), payload)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
