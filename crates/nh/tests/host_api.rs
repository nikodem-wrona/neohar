//! What a host reads from the core without a method of the API: blobs, for `nh://blob/<hash>`.

mod core_support;

use core_support::Harness;
use nh_core::ErrorCode;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_blob_comes_back_with_its_type_and_a_wrong_name_is_nothing() {
    let h = Harness::start_layout();
    let hash = h
        .db()
        .put_blob(b"\x89PNG not really", "image/png", 1)
        .expect("a blob is stored");
    let (bytes, mime) = h
        .handle
        .blob(&hash)
        .await
        .expect("the store answers")
        .expect("the blob exists");
    assert_eq!(bytes, b"\x89PNG not really");
    assert_eq!(mime, "image/png");

    // A hash that no blob has, and names that are not hashes.
    let unknown = "0".repeat(64);
    assert!(h.handle.blob(&unknown).await.expect("answers").is_none());
    for name in ["", "../state.db", "short", &"G".repeat(64)] {
        assert!(
            h.handle.blob(name).await.expect("answers").is_none(),
            "{name:?}"
        );
    }
}

struct Collect(tokio::sync::mpsc::UnboundedSender<serde_json::Value>);

impl nh_core::MessageSink for Collect {
    fn send(&self, message: serde_json::Value) -> bool {
        self.0.send(message).is_ok()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_subscription_of_the_ui_has_an_id_and_the_page_ends_it_with_unsubscribe() {
    use std::time::Duration;

    use serde_json::json;
    let h = Harness::start_layout();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let id = h
        .handle
        .subscribe_tracked("tree", None, Box::new(Collect(tx)))
        .expect("a subscription");
    assert!(id.starts_with("tree#"), "{id}");
    // The snapshot first, then the patch of a change.
    let first = rx.recv().await.expect("a snapshot");
    assert_eq!(first["type"], "snapshot");
    h.call("workspace.create", json!({ "name": "A" })).await;
    let patch = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("a patch in time")
        .expect("a message");
    assert_eq!(patch["type"], "patch");

    // The UI ends it with the method of the API. The same call ends nothing twice.
    h.call("projection.unsubscribe", json!({ "subscription": id }))
        .await;
    h.call("workspace.create", json!({ "name": "B" })).await;
    // The hub dropped the sink, so the channel closes and nothing more comes.
    assert!(
        matches!(
            tokio::time::timeout(Duration::from_millis(300), rx.recv()).await,
            Ok(None)
        ),
        "no message after the unsubscribe"
    );
    let error = h
        .handle
        .rpc("projection.unsubscribe", json!({ "subscription": id }))
        .await
        .expect_err("the second unsubscribe");
    assert_eq!(error.code, ErrorCode::NotFound);
    let error = h
        .handle
        .rpc("projection.unsubscribe", json!({ "subscription": "nope" }))
        .await
        .expect_err("a wrong id");
    assert_eq!(error.code, ErrorCode::NotFound);
}
