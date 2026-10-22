//! The text mirror of a pane for accessibility (`pane_text:<pane>`).
//!
//! The projection has one key for each visible row, the row number, and the text of the row as the
//! value. A reader of the screen follows it. It costs nothing while nobody follows it: the task
//! looks at a pane only when the projection of that pane has a subscriber, and only when the pane
//! changed. It publishes at most four times a second.

use std::{
    collections::HashMap,
    sync::{Arc, Weak},
    time::Duration,
};

use nh_proto::api::{ids::PaneId, projections::Projection};
use serde_json::Value;
use tokio::time::{MissedTickBehavior, interval};
use ulid::Ulid;

use crate::{pane::lock, runtime::Inner};

/// The time between two looks at the panes: four times a second.
const PERIOD: Duration = Duration::from_millis(250);

/// Publishes the rows of the panes that have a follower, until the core ends.
pub(crate) async fn run(inner: Weak<Inner>) {
    // The revision of each pane that the projection shows, and how many rows it has.
    let mut shown: HashMap<Ulid, (u64, u16)> = HashMap::new();
    let mut tick = interval(PERIOD);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        let Some(inner) = inner.upgrade() else {
            return;
        };
        publish_all(&inner, &mut shown);
    }
}

fn publish_all(inner: &Arc<Inner>, shown: &mut HashMap<Ulid, (u64, u16)>) {
    let panes: Vec<_> = lock(&inner.panes).values().cloned().collect();
    let hub = inner.projections.hub();
    shown.retain(|id, _| panes.iter().any(|pane| pane.id() == *id));
    for pane in panes {
        let projection = Projection::PaneText(PaneId::new(pane.id()));
        if hub.subscriber_count(projection) == 0 {
            // Nobody follows the pane now. The next follower gets the rows at the next look.
            shown.remove(&pane.id());
            continue;
        }
        let rev = pane.rev();
        let rows = pane.size().1;
        if shown.get(&pane.id()) == Some(&(rev, rows)) {
            continue;
        }
        let text = pane.text();
        let keys: Vec<String> = (0..text.len()).map(|row| row.to_string()).collect();
        for (key, line) in keys.iter().zip(text) {
            hub.set(projection, key, Value::String(line));
        }
        // A pane that got smaller has fewer rows.
        let snapshot = hub.snapshot(projection);
        for key in snapshot.items.keys() {
            if !keys.contains(key) {
                hub.remove(projection, key);
            }
        }
        hub.set_order(projection, &keys);
        shown.insert(pane.id(), (rev, rows));
    }
}
