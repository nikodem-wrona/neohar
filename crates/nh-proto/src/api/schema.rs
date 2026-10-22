//! The schema document of the API (`nh api schema`).
//!
//! One JSON document with every method, notification, projection, event and stream, and the
//! definitions they refer to. The generator of the TypeScript types reads it
//! (`tools/gen-ts`). The output is canonical: object keys are sorted, so two builds with different
//! `serde_json` features print the same text.

use schemars::SchemaGenerator;
use serde_json::{Map, Value, json};

use super::{
    envelope::{CallError, ErrorCode, Notification, Request, Response},
    events,
    methods::{self, Method},
    projections,
    streams::{InputStreamParams, PeekParams, RenderParams, ScrollbackParams, StreamKind},
    time::Timestamp,
};
use crate::version::{API_VERSION, FRAME_PROTO, HOLDER_PROTO};

/// Sorts the keys of every object. The order of an array is meaningful and stays.
fn canonical(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<String> = map.keys().cloned().collect();
            keys.sort();
            let mut sorted = Map::new();
            let mut map = map;
            for key in keys {
                if let Some(inner) = map.remove(&key) {
                    sorted.insert(key, canonical(inner));
                }
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(canonical).collect()),
        other => other,
    }
}

fn to_value(schema: schemars::Schema) -> Value {
    serde_json::to_value(schema).unwrap_or(Value::Null)
}

/// The whole contract as one JSON value.
pub fn document() -> Value {
    let mut generator = SchemaGenerator::default();

    let methods: Vec<Value> = Method::ALL
        .iter()
        .map(|method| {
            json!({
                "name": method.name(),
                "scopes": method.callers(),
                "params": to_value(method.params_schema(&mut generator)),
                "result": to_value(method.result_schema(&mut generator)),
            })
        })
        .collect();

    let notifications: Vec<Value> = methods::notifications()
        .into_iter()
        .map(|spec| json!({"name": spec.name, "params": to_value((spec.params)(&mut generator))}))
        .collect();

    let projections: Vec<Value> = projections::specs()
        .into_iter()
        .map(|spec| {
            json!({
                "name": spec.name,
                "key": spec.key,
                "singleton": spec.singleton,
                "parameter": spec.parameter,
                "value": to_value((spec.value)(&mut generator)),
            })
        })
        .collect();

    let events: Vec<Value> = events::specs()
        .into_iter()
        .map(|spec| {
            json!({
                "kind": spec.kind,
                "v": spec.v,
                "payload": to_value((spec.payload)(&mut generator)),
            })
        })
        .collect();

    let streams = json!([
        {"kind": "render", "direction": "core_to_ui", "params": to_value(generator.subschema_for::<RenderParams>())},
        {"kind": "peek", "direction": "core_to_ui", "params": to_value(generator.subschema_for::<PeekParams>())},
        {"kind": "scrollback", "direction": "core_to_ui", "params": to_value(generator.subschema_for::<ScrollbackParams>())},
        {"kind": "input", "direction": "ui_to_core", "params": to_value(generator.subschema_for::<InputStreamParams>())},
    ]);

    let error_codes: Vec<Value> = ErrorCode::ALL
        .iter()
        .map(|code| json!({"code": code.as_str(), "rpc_code": code.rpc_code()}))
        .collect();

    let envelope = json!({
        "request": to_value(generator.subschema_for::<Request>()),
        "response": to_value(generator.subschema_for::<Response>()),
        "notification": to_value(generator.subschema_for::<Notification>()),
        "call_error": to_value(generator.subschema_for::<CallError>()),
    });

    // Types that no method names directly, and that a client needs.
    let _ = generator.subschema_for::<StreamKind>();
    let _ = generator.subschema_for::<Timestamp>();

    let definitions = generator.take_definitions(true);

    canonical(json!({
        "api_version": API_VERSION,
        "protocols": {"frame": FRAME_PROTO, "holder": HOLDER_PROTO},
        "error_codes": error_codes,
        "envelope": envelope,
        "methods": methods,
        "notifications": notifications,
        "projections": projections,
        "events": events,
        "streams": streams,
        "$defs": definitions,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_document_lists_every_method_projection_event_and_stream() {
        let doc = document();
        assert_eq!(doc["api_version"], "neohar/1");
        assert_eq!(
            doc["methods"].as_array().map(Vec::len),
            Some(Method::ALL.len())
        );
        assert_eq!(
            doc["events"].as_array().map(Vec::len),
            Some(events::EventKind::ALL.len())
        );
        assert_eq!(
            doc["projections"].as_array().map(Vec::len),
            Some(projections::specs().len())
        );
        assert_eq!(doc["streams"].as_array().map(Vec::len), Some(4));
        assert_eq!(doc["notifications"].as_array().map(Vec::len), Some(4));
    }

    #[test]
    fn every_reference_in_the_document_resolves() {
        let doc = document();
        let defs = doc["$defs"].as_object().expect("definitions");
        let mut missing = Vec::new();
        fn walk(value: &Value, defs: &Map<String, Value>, missing: &mut Vec<String>) {
            match value {
                Value::Object(map) => {
                    if let Some(Value::String(reference)) = map.get("$ref") {
                        match reference.strip_prefix("#/$defs/") {
                            Some(name) if defs.contains_key(name) => {}
                            _ => missing.push(reference.clone()),
                        }
                    }
                    for inner in map.values() {
                        walk(inner, defs, missing);
                    }
                }
                Value::Array(items) => {
                    for inner in items {
                        walk(inner, defs, missing);
                    }
                }
                _ => {}
            }
        }
        walk(&doc, defs, &mut missing);
        assert!(missing.is_empty(), "unresolved: {missing:?}");
    }

    #[test]
    fn the_output_is_the_same_on_every_call() {
        assert_eq!(document().to_string(), document().to_string());
    }

    #[test]
    fn the_keys_are_sorted() {
        let doc = document();
        let keys: Vec<&String> = doc.as_object().expect("an object").keys().collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }
}
