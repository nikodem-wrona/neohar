use nh_proto::api::common::Harness;
use serde_json::{Value, json};

use super::*;

fn health() -> (Arc<Health>, Arc<ProjectionHub>) {
    let hub = ProjectionHub::new();
    (Health::new(Arc::clone(&hub)), hub)
}

/// The single row, as the last flush left it, and the version of the projection.
fn shown(hub: &ProjectionHub) -> (u64, Value) {
    hub.flush();
    let published = hub.snapshot(Projection::Health);
    (published.version, published.items["_"].clone())
}

#[test]
fn a_new_core_is_healthy_and_the_row_exists_at_once() {
    let (health, hub) = health();
    let (_, row) = shown(&hub);
    assert_eq!(
        row,
        json!({
            "storage": {"state": "ok"},
            "holder_protocols": [],
            "unvalidated_harnesses": [],
            "warnings": []
        })
    );
    assert_eq!(health.value().storage, StorageHealth::Ok);
}

#[test]
fn storage_and_renderer_follow_their_reports() {
    let (health, hub) = health();
    health.set_storage(StorageHealth::Degraded {
        reason: "the disk is full".to_owned(),
    });
    health.set_renderer(Some(RendererKind::Canvas2d));
    let (_, row) = shown(&hub);
    assert_eq!(
        row["storage"],
        json!({"state": "degraded", "reason": "the disk is full"})
    );
    assert_eq!(row["renderer"], "canvas2d");
    health.set_storage(StorageHealth::Ok);
    health.set_renderer(None);
    let (_, row) = shown(&hub);
    assert_eq!(row["storage"], json!({"state": "ok"}));
    assert!(row.get("renderer").is_none());
}

#[test]
fn holder_protocols_are_a_sorted_set() {
    let (health, hub) = health();
    for version in [2, 1, 2, 1] {
        health.holder_protocol(version);
    }
    assert_eq!(shown(&hub).1["holder_protocols"], json!([1, 2]));
}

#[test]
fn unvalidated_harnesses_are_listed() {
    let (health, hub) = health();
    health.set_unvalidated_harnesses(vec![UnvalidatedHarness {
        harness: Harness::Codex,
        version: "0.99.0".to_owned(),
    }]);
    assert_eq!(
        shown(&hub).1["unvalidated_harnesses"],
        json!([{"harness": "codex", "version": "0.99.0"}])
    );
}

#[test]
fn a_warning_is_set_by_id_and_cleared() {
    let (health, hub) = health();
    health.warn(
        "link",
        "The link at ~/.local/bin/nh is a file.",
        Some("Move it.".to_owned()),
    );
    health.warn("a", "First", None);
    health.warn("link", "The link is a directory.", None);
    let (_, row) = shown(&hub);
    assert_eq!(
        row["warnings"],
        json!([
            {"id": "a", "text": "First"},
            {"id": "link", "text": "The link is a directory."}
        ])
    );
    health.clear("a");
    health.clear("nothing");
    assert_eq!(shown(&hub).1["warnings"].as_array().map(Vec::len), Some(1));
}

#[test]
fn a_group_of_warnings_is_replaced_as_a_set() {
    let (health, hub) = health();
    health.warn("task:ticker", "A task stopped.", None);
    health.set_warnings(
        "config",
        [
            "config.toml: line 3".to_owned(),
            "approvals.mood is not known".to_owned(),
        ],
        Some("The last good configuration is in force."),
    );
    let (_, row) = shown(&hub);
    let ids: Vec<&str> = row["warnings"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|w| w["id"].as_str().expect("an id"))
        .collect();
    assert_eq!(ids, ["config:0", "config:1", "task:ticker"]);
    assert_eq!(
        row["warnings"][0]["hint"],
        "The last good configuration is in force."
    );
    // One text now: the second warning of the group is gone, the other group is not touched.
    health.set_warnings("config", ["config.toml: line 9".to_owned()], None);
    let (_, row) = shown(&hub);
    assert_eq!(row["warnings"].as_array().map(Vec::len), Some(2));
    assert_eq!(row["warnings"][0]["text"], "config.toml: line 9");
    assert!(row["warnings"][0].get("hint").is_none());
    health.set_warnings("config", [], None);
    let (_, row) = shown(&hub);
    assert_eq!(
        row["warnings"],
        json!([{"id": "task:ticker", "text": "A task stopped."}])
    );
}

#[test]
fn a_report_that_changes_nothing_makes_no_patch() {
    let (health, hub) = health();
    health.holder_protocol(1);
    health.warn("a", "A", None);
    let (version, _) = shown(&hub);
    health.holder_protocol(1);
    health.warn("a", "A", None);
    health.set_storage(StorageHealth::Ok);
    health.set_warnings("config", [], None);
    health.clear("nothing");
    assert_eq!(shown(&hub).0, version);
    health.warn("a", "B", None);
    assert_eq!(shown(&hub).0, version + 1);
}
