//! What the host does over a link, as one trait.
//!
//! [`LinkClient`] is one link: when it ends, everything on it ends. [`crate::PersistentLink`]
//! keeps the subscriptions, the streams and the input in flight across links, and it opens a new
//! link when one is lost. The host of the app uses the trait, so a machine behind ssh and a
//! daemon behind `link.sock` use the same code.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use nh_core::{FrameSink, MessageSink, RpcError};
use nh_proto::api::ids::WindowId;
use serde_json::Value;

use crate::{Device, LinkClient, Welcome, client::BlobResult};

#[async_trait]
pub trait Link: Send + Sync + 'static {
    /// What the core said in the last `Welcome`.
    fn welcome(&self) -> Welcome;
    /// The last round-trip time of a ping.
    fn rtt(&self) -> Option<Duration>;
    /// Whether a link is up now.
    fn is_up(&self) -> bool;
    /// Waits until the link is down for good and returns the reason.
    async fn closed(&self) -> String;
    /// Ends the link with `Bye`.
    fn close(&self, reason: &str);
    /// The windows that this client owns.
    fn set_owned_windows(&self, windows: Vec<WindowId>);
    fn own_window(&self, window: WindowId, owned: bool);
    async fn call(
        &self,
        window: Option<WindowId>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError>;
    async fn subscribe(
        &self,
        name: &str,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Result<String, RpcError>;
    fn unsubscribe(&self, subscription: &str) -> bool;
    async fn stream(
        &self,
        kind: &str,
        params: Value,
        window: Option<WindowId>,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError>;
    async fn blob(&self, hash: &str) -> BlobResult;
    /// Puts a file in the blob store of the core and returns its hash.
    async fn put_blob(&self, name: &str, bytes: Vec<u8>) -> Result<String, RpcError>;
    /// A call that the host relays for a phone: the core of the machine
    /// puts the device of the phone in the origin.
    async fn relay_call(
        &self,
        device: &Device,
        window: Option<WindowId>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError>;
    /// A stream that the host relays for a phone. The core of the machine attaches the phone as a
    /// client while a stream of it is open.
    async fn relay_stream(
        &self,
        device: &Device,
        kind: &str,
        params: Value,
        window: Option<WindowId>,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError>;
}

#[async_trait]
impl Link for LinkClient {
    fn welcome(&self) -> Welcome {
        LinkClient::welcome(self).clone()
    }

    fn rtt(&self) -> Option<Duration> {
        LinkClient::rtt(self)
    }

    fn is_up(&self) -> bool {
        LinkClient::is_up(self)
    }

    async fn closed(&self) -> String {
        LinkClient::closed(self).await
    }

    fn close(&self, reason: &str) {
        LinkClient::close(self, reason);
    }

    fn set_owned_windows(&self, windows: Vec<WindowId>) {
        LinkClient::set_owned_windows(self, windows);
    }

    fn own_window(&self, window: WindowId, owned: bool) {
        LinkClient::own_window(self, window, owned);
    }

    async fn call(
        &self,
        window: Option<WindowId>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        LinkClient::call(self, window, method, params).await
    }

    async fn subscribe(
        &self,
        name: &str,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Result<String, RpcError> {
        LinkClient::subscribe(self, name, since, sink).await
    }

    fn unsubscribe(&self, subscription: &str) -> bool {
        LinkClient::unsubscribe(self, subscription)
    }

    async fn stream(
        &self,
        kind: &str,
        params: Value,
        window: Option<WindowId>,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError> {
        LinkClient::stream(self, kind, params, window, sink).await
    }

    async fn blob(&self, hash: &str) -> BlobResult {
        LinkClient::blob(self, hash).await
    }

    async fn put_blob(&self, name: &str, bytes: Vec<u8>) -> Result<String, RpcError> {
        LinkClient::put_blob(self, name, &bytes).await
    }

    async fn relay_call(
        &self,
        device: &Device,
        window: Option<WindowId>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        LinkClient::call_for(self, Some(device.clone()), window, method, params).await
    }

    async fn relay_stream(
        &self,
        device: &Device,
        kind: &str,
        params: Value,
        window: Option<WindowId>,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError> {
        LinkClient::stream_for(self, Some(device.clone()), kind, params, window, sink).await
    }
}
