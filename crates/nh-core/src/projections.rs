//! The projections of the core as the panes and the hosts use them.
//!
//! [`Projections`] is a thin front for the [`ProjectionHub`] (`projection/`). The hub does the
//! work: versions, coalescing, the ring for `since`. The panes call `set_pane`, and a host calls
//! `subscribe` with the name of a projection.

use std::sync::Arc;

use nh_proto::api::projections::Projection;
use serde_json::Value;

use crate::{
    ErrorCode,
    error::RpcError,
    projection::{MessageSink, ProjectionHub, Start, SubscriptionId},
};

/// All projections of the core.
#[derive(Default)]
pub struct Projections {
    hub: Arc<ProjectionHub>,
}

impl Projections {
    /// The hub, for the ticker and for the code that feeds other projections.
    pub fn hub(&self) -> &Arc<ProjectionHub> {
        &self.hub
    }

    /// Adds a pane or changes its row.
    pub(crate) fn set_pane(&self, key: &str, value: Value) {
        self.hub.set(Projection::Panes, key, value);
    }

    /// Subscribes to a projection by name. Without `since` the sink gets the snapshot before this
    /// returns. With `since` it gets the patches after that version, or `resync`. No patch is lost
    /// between the first message and the first live patch.
    pub fn subscribe(
        &self,
        name: &str,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Result<(SubscriptionId, Start), RpcError> {
        let projection: Projection = name
            .parse()
            .map_err(|_| RpcError::new(ErrorCode::NotFound, format!("no projection {name:?}")))?;
        Ok(self.hub.subscribe(projection, since, sink))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, PoisonError};

    use serde_json::json;

    use super::*;

    #[derive(Clone, Default)]
    struct Collector(Arc<Mutex<Vec<Value>>>);

    impl MessageSink for Collector {
        fn send(&self, message: Value) -> bool {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(message);
            true
        }
    }

    #[test]
    fn the_panes_projection_goes_through_the_hub() {
        let projections = Projections::default();
        projections.set_pane("a", json!({"title": "one"}));
        projections.hub().flush();
        let sink = Collector::default();
        projections
            .subscribe("panes", None, Box::new(sink.clone()))
            .expect("the projection exists");
        projections.set_pane("b", json!({"title": "two"}));
        projections.hub().flush();
        let got = sink
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        assert_eq!(got[0]["type"], "snapshot");
        assert_eq!(got[0]["data"]["order"], json!(["a"]));
        assert_eq!(got[1]["type"], "patch");
        assert_eq!(got[1]["ops"][1], json!({"op": "order", "keys": ["a", "b"]}));
    }

    #[test]
    fn an_unknown_projection_is_an_error() {
        let projections = Projections::default();
        let error = projections
            .subscribe("nope", None, Box::new(Collector::default()))
            .expect_err("no such projection");
        assert_eq!(error.code, ErrorCode::NotFound);
    }
}
