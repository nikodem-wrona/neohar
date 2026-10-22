//! Compares two recordings of one scenario.
//!
//! Two recordings of a scenario are the same when a person would see the same thing and the
//! hooks and requests came in the same order. What differs from run to run is hidden:
//!
//! - The timing: `session.cast` times, `at` in `signals.jsonl`, `duration_ms` and `at_ms`.
//! - The ids: a hook payload is compared by the names of its fields, not by their values.
//! - The text that the scenario masks (`[[mask]]`): a clock, a session id, a tip of the day, a
//!   file name that the harness picks at random. The masks are applied to each screen and to the
//!   description of each request.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use regex::Regex;
use serde_json::Value;

use crate::{GoldenError, meta::Meta, scenario::Scenario};

/// One thing that differs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    /// Where: `meta`, `screen <name>`, `signals`, `llm-requests`.
    pub what: String,
    pub detail: String,
}

fn read(dir: &Path, name: &str) -> Result<String, GoldenError> {
    let path = dir.join(name);
    fs::read_to_string(&path).map_err(GoldenError::io(format!("read {}", path.display())))
}

/// Hides the text that a scenario masks.
pub fn mask(text: &str, masks: &[(Regex, String)]) -> String {
    let mut text = text.to_owned();
    for (pattern, with) in masks {
        text = pattern.replace_all(&text, with.as_str()).into_owned();
    }
    text
}

/// The first row where two screens differ.
fn first_row_difference(a: &str, b: &str) -> Option<String> {
    let (a_rows, b_rows): (Vec<_>, Vec<_>) = (a.lines().collect(), b.lines().collect());
    for at in 0..a_rows.len().max(b_rows.len()) {
        let (left, right) = (a_rows.get(at), b_rows.get(at));
        if left != right {
            return Some(format!(
                "row {}:\n  first:  {}\n  second: {}",
                at + 1,
                left.map_or("(no row)", |s| s),
                right.map_or("(no row)", |s| s),
            ));
        }
    }
    None
}

/// The names of the fields of a payload, which is what stays the same from run to run.
fn field_names(payload: &Value) -> BTreeSet<String> {
    payload
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

fn signal_shapes(text: &str) -> Vec<(String, String, BTreeSet<String>)> {
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|v| {
            (
                v["harness"].as_str().unwrap_or_default().to_owned(),
                v["event"].as_str().unwrap_or_default().to_owned(),
                field_names(&v["payload"]),
            )
        })
        .collect()
}

/// The shape of each line of `session.jsonl`: the type, the subtype and the names of the fields.
/// The values hold ids, times and costs, and they differ from run to run. The lines are grouped
/// by peer and direction, each group in its order. Two peers get the same broadcast in an order
/// that changes from run to run, and a line that comes can arrive before or after a line that
/// goes. The order in one group of one peer is the order of the protocol.
fn session_shapes(text: &str, masks: &[(Regex, String)]) -> BTreeMap<String, Vec<String>> {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for record in text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
    {
        let group = format!(
            "{}{}",
            record["peer"]
                .as_str()
                .map_or(String::new(), |p| format!("{p}:")),
            record["dir"].as_str().unwrap_or_default()
        );
        let shape = match record.get("json") {
            Some(value) => {
                let names: Vec<String> = field_names(value).into_iter().collect();
                let kind = |v: &Value, key: &str| v[key].as_str().unwrap_or_default().to_owned();
                format!(
                    "{} {} {} {} [{}]",
                    kind(value, "type"),
                    kind(value, "subtype"),
                    kind(value, "method"),
                    kind(&value["request"], "subtype"),
                    names.join(",")
                )
            }
            None => format!(
                "text {}",
                mask(record["text"].as_str().unwrap_or_default(), masks)
            ),
        };
        groups.entry(group).or_default().push(shape);
    }
    groups
}

/// The interactions of a recording with a core, without the ids and the times.
fn interaction_shapes(text: &str) -> Vec<Value> {
    let Ok(Value::Array(all)) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    all.into_iter()
        .map(|mut interaction| {
            if let Some(map) = interaction.as_object_mut() {
                for key in [
                    "id",
                    "token",
                    "native_ref",
                    "shim_pid",
                    "deadline_ms",
                    "idempotency_key",
                    // The deadline, the key of the delivery and the number of the connection.
                    "deadline",
                    "key",
                    "conn",
                ] {
                    map.remove(key);
                }
                // The winner of a race is not known before the race.
                if matches!(
                    map.get("decided_by").and_then(Value::as_str),
                    Some("first" | "second")
                ) {
                    map.insert(
                        "decided_by".to_owned(),
                        Value::String("the winner of the race".to_owned()),
                    );
                }
            }
            interaction
        })
        .collect()
}

/// The steps of the core in a recording: the operation and what it found or did.
fn core_event_shapes(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|event| {
            let op = event["op"].as_str().unwrap_or_default();
            let detail = &event["detail"];
            if op == "race" {
                // Which of the two answers wins is not known before the race: the two results
                // are compared as a set.
                let mut results: Vec<String> = ["first", "second"]
                    .iter()
                    .map(|which| detail[which]["result"].to_string())
                    .collect();
                results.sort();
                return format!("race {}", results.join(" "));
            }
            let text = |v: &Value| v.as_str().unwrap_or_default().to_owned();
            format!(
                "{op} state={} mode={} rev={} error={}",
                text(&detail["result"]["state"]),
                text(&detail["mode"]),
                detail["result"]["rev"],
                text(&detail["error"]),
            )
        })
        .collect()
}

/// How much of the last user text goes into the description of a request. The log keeps 200
/// characters, and a text with a random file name in it ends at another place in each run.
const LAST_USER_CHARS: usize = 100;

/// The description of each request. The requests that offer tools come first, in their order, and
/// then the others in theirs. Codex asks for the title of a task on the side, and that request can
/// come before or after the request for the next turn.
fn request_shapes(text: &str, masks: &[(Regex, String)]) -> Vec<String> {
    let mut main = Vec::new();
    let mut auxiliary = Vec::new();
    for v in text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
    {
        let field = |k: &str| v[k].to_string();
        let last_user: String = mask(v["last_user"].as_str().unwrap_or_default(), masks)
            .chars()
            .take(LAST_USER_CHARS)
            .collect();
        let shape = format!(
            "{} {} {} model={} main={} stream={} turn={} last_user={last_user:?}",
            field("api"),
            v["method"].as_str().unwrap_or_default(),
            v["path"].as_str().unwrap_or_default(),
            field("model"),
            field("main"),
            field("stream"),
            field("turn"),
        );
        if v["main"] == true {
            main.push(shape);
        } else {
            auxiliary.push(shape);
        }
    }
    main.extend(auxiliary);
    main
}

/// Lists what differs between the recordings in `a` and `b`. An empty list means that they are
/// the same.
pub fn compare(a: &Path, b: &Path) -> Result<Vec<Difference>, GoldenError> {
    let mut found = Vec::new();
    let mut add = |what: &str, detail: String| {
        found.push(Difference {
            what: what.to_owned(),
            detail,
        });
    };

    let (ma, mb) = (Meta::read(a)?, Meta::read(b)?);
    macro_rules! same {
        ($($field:ident),+) => {$(
            if ma.$field != mb.$field {
                add(
                    "meta",
                    format!("{}: {:?} and {:?}", stringify!($field), ma.$field, mb.$field),
                );
            }
        )+};
    }
    same!(
        mode,
        harness,
        harness_version,
        scenario,
        scenario_blake3,
        cols,
        rows,
        exit,
        llm_requests,
        llm_main_requests,
        turns_used,
        turns_total,
        unmatched_main,
        hook_calls,
        hook_calls_api,
        screens,
        processes
    );

    let scenario = Scenario::parse(&read(a, "scenario.toml")?)?;
    let masks: Vec<(Regex, String)> = scenario
        .masks
        .iter()
        .map(|m| {
            Regex::new(&m.pattern)
                .map(|r| (r, m.with.clone()))
                .map_err(|e| GoldenError::Setup(format!("a mask: {e}")))
        })
        .collect::<Result<_, _>>()?;
    for name in &ma.screens {
        let (Ok(sa), Ok(sb)) = (
            read(a, &format!("screens/{name}.txt")),
            read(b, &format!("screens/{name}.txt")),
        ) else {
            add(&format!("screen {name}"), "a file is missing".to_owned());
            continue;
        };
        if let Some(difference) = first_row_difference(&mask(&sa, &masks), &mask(&sb, &masks)) {
            add(&format!("screen {name}"), difference);
        }
    }

    for name in &ma.processes {
        let (Ok(pa), Ok(pb)) = (
            read(a, &format!("processes/{name}.txt")),
            read(b, &format!("processes/{name}.txt")),
        ) else {
            add(&format!("processes {name}"), "a file is missing".to_owned());
            continue;
        };
        if let Some(difference) = first_row_difference(&mask(&pa, &masks), &mask(&pb, &masks)) {
            add(&format!("processes {name}"), difference);
        }
    }

    if ma.mode == "pipe" {
        let (ha, hb) = (
            session_shapes(&read(a, "session.jsonl")?, &masks),
            session_shapes(&read(b, "session.jsonl")?, &masks),
        );
        if ha != hb {
            let keys: BTreeSet<&String> = ha.keys().chain(hb.keys()).collect();
            let mut found = false;
            for key in keys {
                let (la, lb) = (ha.get(key), hb.get(key));
                if la != lb {
                    let none = Vec::new();
                    let (la, lb) = (la.unwrap_or(&none), lb.unwrap_or(&none));
                    let at = (0..la.len().max(lb.len()))
                        .find(|i| la.get(*i) != lb.get(*i))
                        .unwrap_or_default();
                    add(
                        "session",
                        format!(
                            "the lines of {key} differ at line {}: {:?} and {:?}",
                            at + 1,
                            la.get(at),
                            lb.get(at)
                        ),
                    );
                    found = true;
                }
            }
            debug_assert!(found);
        }
    }

    let (ia, ib) = (
        read(a, "core/interactions.json").ok(),
        read(b, "core/interactions.json").ok(),
    );
    match (ia, ib) {
        (Some(ta), Some(tb)) => {
            let (sa, sb) = (interaction_shapes(&ta), interaction_shapes(&tb));
            if sa != sb {
                add("core interactions", format!("{sa:?} and {sb:?}"));
            }
            let (ea, eb) = (
                core_event_shapes(&read(a, "core/events.jsonl").unwrap_or_default()),
                core_event_shapes(&read(b, "core/events.jsonl").unwrap_or_default()),
            );
            if ea != eb {
                let at = (0..ea.len().max(eb.len()))
                    .find(|i| ea.get(*i) != eb.get(*i))
                    .unwrap_or_default();
                add(
                    "core events",
                    format!(
                        "the steps differ at event {}: {:?} and {:?}",
                        at + 1,
                        ea.get(at),
                        eb.get(at)
                    ),
                );
            }
        }
        (None, None) => {}
        _ => add("core", "only one recording has a core".to_owned()),
    }

    let (sa, sb) = (
        signal_shapes(&read(a, "signals.jsonl")?),
        signal_shapes(&read(b, "signals.jsonl")?),
    );
    if sa != sb {
        let at = (0..sa.len().max(sb.len()))
            .find(|i| sa.get(*i) != sb.get(*i))
            .unwrap_or_default();
        add(
            "signals",
            format!(
                "the sequences differ at hook {}: {:?} and {:?}",
                at + 1,
                sa.get(at),
                sb.get(at)
            ),
        );
    }

    let (ra, rb) = (
        request_shapes(&read(a, "llm-requests.jsonl")?, &masks),
        request_shapes(&read(b, "llm-requests.jsonl")?, &masks),
    );
    if ra != rb {
        let at = (0..ra.len().max(rb.len()))
            .find(|i| ra.get(*i) != rb.get(*i))
            .unwrap_or_default();
        add(
            "llm-requests",
            format!(
                "the requests differ at request {}: {:?} and {:?}",
                at + 1,
                ra.get(at),
                rb.get(at)
            ),
        );
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn masks() -> Vec<(Regex, String)> {
        vec![
            (
                Regex::new(r"\d\d:\d\d").expect("regex"),
                "<time>".to_owned(),
            ),
            (
                Regex::new(r"[0-9a-f]{8}-[0-9a-f-]{27}").expect("regex"),
                "<uuid>".to_owned(),
            ),
        ]
    }

    #[test]
    fn masks_hide_the_text_that_changes() {
        assert_eq!(
            mask(
                "done 23:50 id 01a11d81-ac94-7360-a00f-a498ad4029e7",
                &masks()
            ),
            "done <time> id <uuid>"
        );
        assert_eq!(mask("nothing", &masks()), "nothing");
    }

    #[test]
    fn the_first_differing_row_is_named_with_both_texts() {
        assert_eq!(first_row_difference("a\nb\nc", "a\nb\nc"), None);
        let text = first_row_difference("a\nb\nc", "a\nX\nc").expect("a difference");
        assert!(text.starts_with("row 2:"), "{text}");
        assert!(
            text.contains("first:  b") && text.contains("second: X"),
            "{text}"
        );
        let longer = first_row_difference("a", "a\nb").expect("a difference");
        assert!(longer.contains("(no row)"), "{longer}");
    }

    #[test]
    fn session_lines_are_compared_by_type_and_field_names() {
        let line = |cost: f64, id: &str| {
            format!(
                "{{\"t\":1.0,\"dir\":\"out\",\"json\":{{\"type\":\"result\",\"session_id\":\"{id}\",\"total_cost_usd\":{cost}}}}}\n"
            )
        };
        assert_eq!(
            session_shapes(&line(0.5, "a"), &[]),
            session_shapes(&line(0.9, "b"), &[])
        );
        // Two peers: the order between them does not count, the order inside one does.
        let peer = |name: &str, method: &str| {
            format!(
                "{{\"t\":1.0,\"dir\":\"out\",\"peer\":\"{name}\",\"json\":{{\"method\":\"{method}\"}}}}\n"
            )
        };
        let first = peer("a", "x") + &peer("b", "x") + &peer("a", "y");
        let second = peer("b", "x") + &peer("a", "x") + &peer("a", "y");
        let swapped = peer("a", "y") + &peer("a", "x") + &peer("b", "x");
        assert_eq!(session_shapes(&first, &[]), session_shapes(&second, &[]));
        assert_ne!(session_shapes(&first, &[]), session_shapes(&swapped, &[]));
        let other = "{\"t\":1.0,\"dir\":\"out\",\"json\":{\"type\":\"result\"}}\n";
        assert_ne!(
            session_shapes(&line(0.5, "a"), &[]),
            session_shapes(other, &[])
        );
        let text = "{\"t\":1.0,\"dir\":\"err\",\"text\":\"warning 12:30\"}\n";
        let masks = vec![(
            Regex::new(r"\d\d:\d\d").expect("regex"),
            "<time>".to_owned(),
        )];
        assert_eq!(session_shapes(text, &masks)["err"], ["text warning <time>"]);
    }

    #[test]
    fn signals_are_compared_by_event_and_field_names() {
        let one = "{\"at\":1,\"harness\":\"claude\",\"event\":\"Stop\",\"payload\":{\"session_id\":\"a\",\"cwd\":\"/x\"}}\n";
        let two = "{\"at\":9,\"harness\":\"claude\",\"event\":\"Stop\",\"payload\":{\"cwd\":\"/y\",\"session_id\":\"b\"}}\n";
        let three =
            "{\"at\":9,\"harness\":\"claude\",\"event\":\"Stop\",\"payload\":{\"cwd\":\"/y\"}}\n";
        assert_eq!(signal_shapes(one), signal_shapes(two));
        assert_ne!(signal_shapes(one), signal_shapes(three));
    }

    #[test]
    fn a_random_name_in_the_last_user_text_is_masked_before_the_text_is_cut() {
        let line = |name: &str, tail: &str| {
            let text = format!("saved to /tmp/plans/plan-{name}.md and then {tail}");
            format!(
                "{{\"api\":\"a\",\"method\":\"POST\",\"path\":\"/p\",\"main\":true,\"last_user\":\"{text}\"}}\n"
            )
        };
        let masks = vec![(
            Regex::new("plan-[a-z-]+\\.md").expect("regex"),
            "plan-<words>.md".to_owned(),
        )];
        let tail = "x".repeat(150);
        assert_eq!(
            request_shapes(&line("short", &tail), &masks),
            request_shapes(&line("a-much-longer-name", &tail), &masks)
        );
    }

    #[test]
    fn a_request_on_the_side_can_come_before_or_after_a_main_request() {
        let line = |main: bool, turn: &str, text: &str| {
            format!(
                "{{\"api\":\"openai\",\"method\":\"POST\",\"path\":\"/v1/responses\",\"main\":{main},\"turn\":{turn},\"last_user\":\"{text}\"}}\n"
            )
        };
        let (m0, m1, side) = (
            line(true, "0", "a"),
            line(true, "1", "b"),
            line(false, "null", "title"),
        );
        let one = format!("{m0}{side}{m1}");
        let two = format!("{m0}{m1}{side}");
        let other = format!("{m1}{m0}{side}");
        assert_eq!(request_shapes(&one, &[]), request_shapes(&two, &[]));
        assert_ne!(request_shapes(&one, &[]), request_shapes(&other, &[]));
    }

    #[test]
    fn requests_are_compared_without_their_time_and_body() {
        let one = "{\"api\":\"openai\",\"at_ms\":5,\"method\":\"POST\",\"path\":\"/v1/responses\",\"model\":\"m\",\"main\":true,\"stream\":true,\"turn\":0,\"last_user\":\"hi\",\"body\":{\"a\":1}}\n";
        let two = "{\"api\":\"openai\",\"at_ms\":99,\"method\":\"POST\",\"path\":\"/v1/responses\",\"model\":\"m\",\"main\":true,\"stream\":true,\"turn\":0,\"last_user\":\"hi\",\"body\":{\"a\":2}}\n";
        assert_eq!(request_shapes(one, &[]), request_shapes(two, &[]));
        assert_ne!(
            request_shapes(one, &[]),
            request_shapes(&two.replace("\"turn\":0", "\"turn\":null"), &[])
        );
    }
}
