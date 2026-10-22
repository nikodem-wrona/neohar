//! The dev WebSocket served from a link client: the shell tests of
//! `tests/ui` then run against a core behind `link.sock`. The calls have no window, as on the dev
//! WebSocket of the core.

use std::sync::Arc;

use nh_core::{FrameSink, MessageSink, RpcError, dev_ws::DevWsBackend, ws_rpc::RpcBackend};
use serde_json::Value;

use crate::LinkClient;

#[async_trait::async_trait]
impl RpcBackend for LinkClient {
    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        LinkClient::call(self, None, method, params).await
    }

    async fn subscribe(
        &self,
        name: &str,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Result<String, RpcError> {
        LinkClient::subscribe(self, name, since, sink).await
    }

    async fn unsubscribe(&self, subscription: &str) -> bool {
        LinkClient::unsubscribe(self, subscription)
    }
}

#[async_trait::async_trait]
impl DevWsBackend for LinkClient {
    async fn stream(
        &self,
        kind: &str,
        params: Value,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError> {
        LinkClient::stream(self, kind, params, None, sink).await
    }
}

/// Any link, as the backend of the dev WebSocket: a client of a daemon on this Mac or a
/// link that comes back. A link that comes back also serves the projection `link`, as the
/// host of the app does, so the page shows its banner.
pub struct DevLink {
    link: Arc<dyn crate::link::Link>,
    feed: Option<Arc<crate::LinkFeed>>,
}

impl DevLink {
    /// A link with no projection `link`: the page is local and the link does not come back.
    pub fn new(link: Arc<dyn crate::link::Link>) -> Self {
        Self { link, feed: None }
    }

    /// A link that comes back. The projection `link` follows its status. The machine is `local`,
    /// so a link that is up says `local`, as the app does for the core of this Mac.
    pub fn persistent(link: crate::PersistentLink, runtime: &tokio::runtime::Handle) -> Self {
        use nh_proto::api::model::MachineState;

        let feed = Arc::new(crate::LinkFeed::new("local"));
        let (watched, mut status) = (Arc::clone(&feed), link.status());
        let rtt = link.clone();
        runtime.spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
            loop {
                let now = status.borrow_and_update().clone();
                let (state, rtt_ms) = match &now {
                    crate::LinkStatus::Up => (
                        MachineState::Local,
                        crate::Link::rtt(&rtt).map(|d| d.as_secs_f32() * 1000.0),
                    ),
                    crate::LinkStatus::Reconnecting { trying: true, .. } => {
                        (MachineState::Connecting, None)
                    }
                    _ => (MachineState::Disconnected, None),
                };
                watched.update(state, rtt_ms, now.reason_text());
                tokio::select! {
                    changed = status.changed() => {
                        if changed.is_err() {
                            return;
                        }
                    }
                    _ = tick.tick() => {}
                }
            }
        });
        Self {
            link: Arc::new(link),
            feed: Some(feed),
        }
    }
}

#[async_trait::async_trait]
impl RpcBackend for DevLink {
    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.link.call(None, method, params).await
    }

    async fn subscribe(
        &self,
        name: &str,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Result<String, RpcError> {
        match (&self.feed, name) {
            (Some(feed), "link") => Ok(feed.subscribe(sink)),
            _ => self.link.subscribe(name, since, sink).await,
        }
    }

    async fn unsubscribe(&self, subscription: &str) -> bool {
        if let Some(feed) = &self.feed
            && feed.unsubscribe(subscription)
        {
            return true;
        }
        self.link.unsubscribe(subscription)
    }
}

#[async_trait::async_trait]
impl DevWsBackend for DevLink {
    async fn stream(
        &self,
        kind: &str,
        params: Value,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError> {
        self.link.stream(kind, params, None, sink).await
    }
}
