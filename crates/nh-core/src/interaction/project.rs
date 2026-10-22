//! The projection `interactions`.
//!
//! The rows are the interactions as the API shows them. The order is the order of `interaction.next`
//! (the more urgent first) for the open ones, and then the ones that ended, the newest first.

use nh_proto::api::{ids::InteractionId, projections::Projection};

use super::{Interactions, model::Interaction};
use crate::projection::ProjectionHub;

/// Publishes one interaction and puts the projection in order. `evicted` are the interactions
/// that left the table.
pub(crate) fn publish(
    hub: &ProjectionHub,
    table: &Interactions,
    interaction: &Interaction,
    evicted: &[InteractionId],
) {
    if let Ok(value) = serde_json::to_value(interaction.value()) {
        hub.set(Projection::Interactions, &interaction.id.to_string(), value);
    }
    for id in evicted {
        hub.remove(Projection::Interactions, &id.to_string());
    }
    publish_order(hub, table);
}

pub(crate) fn publish_order(hub: &ProjectionHub, table: &Interactions) {
    let order: Vec<String> = table
        .open()
        .into_iter()
        .chain(table.ended())
        .map(|interaction| interaction.id.to_string())
        .collect();
    hub.set_order(Projection::Interactions, &order);
}

/// Publishes every interaction, after a start of the core.
pub(crate) fn publish_all(hub: &ProjectionHub, table: &Interactions) {
    for interaction in table.open().into_iter().chain(table.ended()) {
        if let Ok(value) = serde_json::to_value(interaction.value()) {
            hub.set(Projection::Interactions, &interaction.id.to_string(), value);
        }
    }
    publish_order(hub, table);
}
