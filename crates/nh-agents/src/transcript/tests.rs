use std::{fs, path::PathBuf};

use nh_proto::api::{
    model::{TimelineBody, TimelineItem, ToolStatus},
    time::Timestamp,
};
use serde_json::{Value, json};

use super::{
    Block, Entry, Format, Kind, Ledger, LineBuffer, Prices, TokenUsage, UsageDelta,
    ledger::Billing, parse_line,
};

const NOW: Timestamp = Timestamp(1_000_000);

fn golden() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/harness-golden")
}

/// The transcripts of the recordings of one harness, with the name of the scenario.
fn recordings(harness: &str) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    let versions = fs::read_dir(golden().join(harness)).expect("recordings");
    for version in versions.flatten() {
        for scenario in fs::read_dir(version.path()).expect("scenarios").flatten() {
            let file = scenario.path().join("transcripts/transcript-1.jsonl");
            if file.exists() {
                found.push((scenario.file_name().to_string_lossy().into_owned(), file));
            }
        }
    }
    found.sort();
    assert!(!found.is_empty(), "no recordings of {harness}");
    found
}

fn entries(format: Format, file: &PathBuf) -> Vec<Entry> {
    let text = fs::read_to_string(file).expect("a transcript");
    let mut offset = 0;
    let mut all = Vec::new();
    for line in text.lines() {
        all.extend(parse_line(format, line, offset));
        offset += line.len() as u64 + 1;
    }
    all
}

fn one(format: Format, line: Value) -> Vec<Entry> {
    parse_line(format, &line.to_string(), 7)
}

fn deltas(ledger: &mut Ledger, prices: &Prices, entries: &[Entry]) -> Vec<UsageDelta> {
    entries
        .iter()
        .flat_map(|entry| ledger.apply(entry, prices, Billing::Api, NOW))
        .collect()
}

// --- the buffer ------------------------------------------------------------------------------

#[test]
fn the_buffer_keeps_an_unfinished_line_for_the_next_read() {
    let mut buffer = LineBuffer::default();
    assert!(buffer.push(b"{\"a\":1}\n{\"b\"").len() == 1);
    assert_eq!(buffer.offset(), 8);
    let lines = buffer.push(b":2}\r\n\n{\"c\":3}\n");
    let texts: Vec<_> = lines.iter().map(|l| (l.offset, l.text.as_str())).collect();
    assert_eq!(texts, vec![(8, "{\"b\":2}"), (18, "{\"c\":3}")]);
    assert_eq!(buffer.offset(), 26);
}

#[test]
fn the_buffer_goes_on_at_a_saved_offset_and_starts_again_when_the_file_is_replaced() {
    let mut buffer = LineBuffer::at(100);
    let lines = buffer.push(b"x\n");
    assert_eq!((lines[0].offset, buffer.offset()), (100, 102));
    buffer.push(b"half");
    buffer.reset();
    assert_eq!(buffer.offset(), 0);
    assert_eq!(buffer.push(b"y\n")[0].offset, 0);
}

#[test]
fn a_line_with_bytes_that_are_not_utf8_is_read_with_replacements() {
    let mut buffer = LineBuffer::default();
    let lines = buffer.push(b"a\xffb\n");
    assert_eq!(lines[0].text, "a\u{fffd}b");
}

// --- Claude ----------------------------------------------------------------------------------

#[test]
fn lines_that_are_not_json_or_have_an_unknown_type_give_nothing() {
    assert!(parse_line(Format::Claude, "not json", 0).is_empty());
    assert!(parse_line(Format::Codex, "", 0).is_empty());
    for kind in [
        "mode",
        "atis-latch",
        "file-history-snapshot",
        "queue-operation",
        "a-new-type",
    ] {
        assert!(one(Format::Claude, json!({"type": kind, "sessionId": "s"})).is_empty());
    }
    assert!(one(Format::Codex, json!({"type": "world_state", "payload": {}})).is_empty());
    assert!(
        one(
            Format::Codex,
            json!({"type": "event_msg", "payload": {"type": "new"}})
        )
        .is_empty()
    );
}

#[test]
fn a_claude_prompt_is_a_string_or_a_text_block_of_the_person() {
    let line = json!({"type": "user", "uuid": "u1", "timestamp": "2026-10-08T23:05:05.581Z",
        "message": {"role": "user", "content": "make a note"}});
    let found = one(Format::Claude, line);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, "u1");
    assert_eq!(found[0].at, Some(Timestamp(1_791_500_705_581)));
    assert_eq!(
        found[0].kind,
        Kind::Prompt {
            text: "make a note".into()
        }
    );

    let blocks = json!({"type": "user", "uuid": "u2",
        "message": {"content": [{"type": "text", "text": "and then"}]}});
    assert_eq!(one(Format::Claude, blocks)[0].id, "u2.0");
    // The text that the harness adds is not a prompt.
    let meta = json!({"type": "user", "uuid": "u3", "isMeta": true,
        "message": {"content": "caveat"}});
    assert!(one(Format::Claude, meta).is_empty());
    let summary = json!({"type": "user", "uuid": "u4", "isCompactSummary": true,
        "message": {"content": "This session is being continued"}});
    assert!(one(Format::Claude, summary).is_empty());
    let other = json!({"type": "user", "uuid": "u5", "origin": {"kind": "task-notification"},
        "message": {"content": "a task ended"}});
    assert!(one(Format::Claude, other).is_empty());
}

#[test]
fn a_claude_tool_result_says_if_the_call_failed_or_was_refused() {
    let result = |content: Value, error: bool| {
        one(
            Format::Claude,
            json!({"type": "user", "uuid": "r", "message": {"content": [
                {"type": "tool_result", "tool_use_id": "toolu_1", "content": content, "is_error": error}
            ]}}),
        )
    };
    let ok = result(
        json!([{"type": "text", "text": "line 1"}, {"type": "text", "text": "line 2"}]),
        false,
    );
    assert_eq!(
        ok[0].kind,
        Kind::ToolResult {
            tool_use: "toolu_1".into(),
            failed: false,
            denied: false,
            text: "line 1\nline 2".into()
        }
    );
    let denied = result(
        json!("The user doesn't want to proceed with this tool use. The tool use was rejected"),
        true,
    );
    assert!(matches!(
        denied[0].kind,
        Kind::ToolResult {
            failed: true,
            denied: true,
            ..
        }
    ));
    let broken = result(json!("exit status 1"), true);
    assert!(matches!(
        broken[0].kind,
        Kind::ToolResult {
            failed: true,
            denied: false,
            ..
        }
    ));
}

#[test]
fn a_claude_message_has_its_id_model_usage_and_blocks() {
    let line = json!({"type": "assistant", "uuid": "a1", "isSidechain": false,
        "message": {"id": "msg_1", "model": "claude-opus-5-5", "role": "assistant",
            "content": [
                {"type": "text", "text": "I will run it."},
                {"type": "tool_use", "id": "toolu_1", "name": "Bash",
                    "input": {"command": "ls -la\npwd", "description": "list"}},
                {"type": "tool_use", "id": "toolu_2", "name": "Task",
                    "input": {"subagent_type": "Explore", "description": "look around"}},
                {"type": "thinking", "thinking": "hmm"}
            ],
            "usage": {"input_tokens": 100, "output_tokens": 20, "cache_read_input_tokens": 5,
                "cache_creation_input_tokens": 7,
                "cache_creation": {"ephemeral_1h_input_tokens": 3, "ephemeral_5m_input_tokens": 4}}}});
    let found = one(Format::Claude, line);
    let Kind::Assistant {
        message,
        model,
        usage,
        blocks,
    } = &found[0].kind
    else {
        panic!("{:?}", found[0]);
    };
    assert_eq!(message.as_deref(), Some("msg_1"));
    assert_eq!(model.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(
        *usage,
        Some(TokenUsage {
            input: 100,
            output: 20,
            cache_read: 5,
            cache_write: 7,
            cache_write_1h: 3
        })
    );
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0], Block::Text("I will run it.".into()));
    assert_eq!(
        blocks[1],
        Block::ToolUse {
            id: "toolu_1".into(),
            name: "Bash".into(),
            summary: "ls -la pwd".into(),
            subagent: None
        }
    );
    assert!(matches!(&blocks[2], Block::ToolUse { subagent: Some(kind), .. } if kind == "Explore"));
}

#[test]
fn a_synthetic_reply_and_a_line_of_a_subagent_are_told_apart() {
    let synthetic = json!({"type": "assistant", "uuid": "a2",
        "message": {"id": "m", "model": "<synthetic>", "content": [{"type": "text", "text": "API Error"}],
            "usage": {"input_tokens": 0, "output_tokens": 0}}});
    let found = one(Format::Claude, synthetic);
    assert!(matches!(
        &found[0].kind,
        Kind::Assistant {
            model: None,
            usage: None,
            ..
        }
    ));
    let side = json!({"type": "assistant", "uuid": "a3", "isSidechain": true,
        "message": {"id": "m2", "model": "claude-opus-5-5", "content": [],
            "usage": {"input_tokens": 3, "output_tokens": 1}}});
    assert!(one(Format::Claude, side)[0].sidechain);
}

#[test]
fn claude_system_lines_give_the_end_of_a_turn_and_a_compaction() {
    let end =
        json!({"type": "system", "subtype": "turn_duration", "uuid": "s1", "durationMs": 183});
    assert_eq!(
        one(Format::Claude, end)[0].kind,
        Kind::TurnEnd {
            duration_ms: Some(183)
        }
    );
    let boundary = json!({"type": "system", "subtype": "compact_boundary", "uuid": "s2"});
    assert_eq!(one(Format::Claude, boundary)[0].kind, Kind::Compaction);
    assert!(
        one(
            Format::Claude,
            json!({"type": "system", "subtype": "stop_hook_summary"})
        )
        .is_empty()
    );
}

// --- Codex -----------------------------------------------------------------------------------

#[test]
fn codex_counts_cached_tokens_inside_its_input_and_the_parser_takes_them_out() {
    let line = json!({"type": "token_usage_record", "ordinal": 11, "payload": {
        "response_id": "resp_1",
        "usage": {"input_tokens": 1000, "cached_input_tokens": 400, "cache_write_input_tokens": 100,
            "output_tokens": 30, "reasoning_output_tokens": 10, "total_tokens": 1030}}});
    let found = one(Format::Codex, line);
    assert_eq!(found[0].id, "o11");
    assert_eq!(
        found[0].kind,
        Kind::Usage {
            key: "resp_1".into(),
            model: None,
            usage: TokenUsage {
                input: 500,
                output: 30,
                cache_read: 400,
                cache_write: 100,
                cache_write_1h: 0
            }
        }
    );
}

#[test]
fn codex_lines_give_turns_messages_calls_and_results() {
    let start = one(
        Format::Codex,
        json!({"type": "event_msg", "ordinal": 1,
        "payload": {"type": "task_started", "turn_id": "t1"}}),
    );
    assert_eq!(start[0].kind, Kind::TurnStart);
    let done = one(
        Format::Codex,
        json!({"type": "event_msg", "ordinal": 2,
        "payload": {"type": "task_complete", "duration_ms": 1598}}),
    );
    assert_eq!(
        done[0].kind,
        Kind::TurnEnd {
            duration_ms: Some(1598)
        }
    );
    let prompt = one(
        Format::Codex,
        json!({"type": "event_msg", "ordinal": 3, "payload": {
        "type": "item_completed", "item": {"type": "UserMessage", "content": [{"type": "text", "text": "make a note"}]}}}),
    );
    assert_eq!(
        prompt[0].kind,
        Kind::Prompt {
            text: "make a note".into()
        }
    );
    let reply = one(
        Format::Codex,
        json!({"type": "event_msg", "ordinal": 4, "payload": {
        "type": "item_completed", "item": {"type": "AgentMessage", "id": "msg_1", "content": [{"type": "Text", "text": "Done."}]}}}),
    );
    assert!(
        matches!(&reply[0].kind, Kind::Assistant { blocks, .. } if blocks == &vec![Block::Text("Done.".into())])
    );
    let call = one(
        Format::Codex,
        json!({"type": "response_item", "ordinal": 5, "payload": {
        "type": "function_call", "name": "exec_command", "call_id": "call_1",
        "arguments": "{\"cmd\":\"touch note.txt\"}"}}),
    );
    assert!(matches!(&call[0].kind, Kind::Assistant { blocks, .. }
        if matches!(&blocks[0], Block::ToolUse { id, summary, .. } if id == "call_1" && summary == "touch note.txt")));
    let failed = one(
        Format::Codex,
        json!({"type": "response_item", "ordinal": 6, "payload": {
        "type": "function_call_output", "call_id": "call_1",
        "output": "Chunk ID: 1\nWall time: 0.1 seconds\nProcess exited with code 2\nOriginal token count: 3\nOutput:\nboom\n"}}),
    );
    assert_eq!(
        failed[0].kind,
        Kind::ToolResult {
            tool_use: "call_1".into(),
            failed: true,
            denied: false,
            text: "boom\n".into()
        }
    );
    let model = one(
        Format::Codex,
        json!({"type": "turn_context", "payload": {"model": "gpt-5.5"}}),
    );
    assert_eq!(
        model[0].kind,
        Kind::Model {
            model: "gpt-5.5".into()
        }
    );
}

#[test]
fn codex_rate_limits_give_windows_only_when_they_are_not_null() {
    let none = json!({"type": "event_msg", "payload": {"type": "token_count",
        "rate_limits": {"primary": null, "secondary": null}}});
    assert!(one(Format::Codex, none).is_empty());
    let some = json!({"type": "event_msg", "payload": {"type": "token_count", "rate_limits": {
        "primary": {"used_percent": 42.5, "window_minutes": 300, "resets_at": 1_791_503_047},
        "secondary": {"used_percent": 10.0, "window_minutes": 10080}}}});
    let Kind::Limits { windows } = &one(Format::Codex, some)[0].kind else {
        panic!("limits");
    };
    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0].name, "5h");
    assert_eq!(windows[0].used_percent, 42.5);
    assert_eq!(windows[0].resets_at, Some(Timestamp(1_791_503_047_000)));
    assert_eq!(windows[1].name, "weekly");
    assert_eq!(windows[1].resets_at, None);
}

// --- prices ----------------------------------------------------------------------------------

#[test]
fn a_model_is_found_by_the_longest_prefix_of_its_id() {
    let prices = Prices::bundled();
    let prefix = |model: &str| prices.find(model).map(|(prefix, ..)| prefix.to_owned());
    assert_eq!(prefix("claude-opus-5-5"), Some("claude-opus-5-5".into()));
    assert_eq!(prefix("claude-opus-5"), Some("claude-opus-5".into()));
    assert_eq!(
        prefix("claude-opus-4-5-20251101"),
        Some("claude-opus-4-5".into())
    );
    assert_eq!(
        prefix("claude-sonnet-4-20250514"),
        Some("claude-sonnet-4".into())
    );
    assert_eq!(
        prefix("anthropic.claude-opus-4-1-20250805-v1:0"),
        Some("claude-opus-4-1".into())
    );
    assert_eq!(prefix("Claude-Haiku-4-5"), Some("claude-haiku-4-5".into()));
    assert_eq!(prefix("gpt-5.4-mini"), Some("gpt-5.4-mini".into()));
    assert_eq!(prefix("gpt-5.5-codex"), Some("gpt-5.5".into()));
    assert_eq!(prefix("gpt-5.1-codex-max"), Some("gpt-5.1".into()));
    assert_eq!(prefix("o3-pro"), Some("o3-pro".into()));
    assert_eq!(prefix("mock-model"), None);
}

#[test]
fn the_cost_of_the_recorded_mock_run_is_the_figure_that_claude_wrote() {
    // The recording `headless-cost`: 36797 tokens in, 8 tokens out, 0.147348 dollars.
    let usage = TokenUsage {
        input: 36797,
        output: 8,
        ..TokenUsage::default()
    };
    let cost = Prices::bundled()
        .cost("claude-opus-5-5", &usage)
        .expect("a price");
    assert!((cost - 0.147348).abs() < 1e-9, "{cost}");
}

#[test]
fn the_cost_has_a_price_for_the_cache_the_hour_and_a_long_prompt() {
    let prices = Prices::bundled();
    // Opus 5.5: cache read 0.20, 5-minute write 5, 1-hour write 8.
    let usage = TokenUsage {
        cache_read: 1_000_000,
        cache_write: 2_000_000,
        cache_write_1h: 1_000_000,
        ..TokenUsage::default()
    };
    let cost = prices.cost("claude-opus-5-5", &usage).expect("a price");
    assert!((cost - (0.20 + 5.0 + 8.0)).abs() < 1e-9, "{cost}");
    // Haiku 5.5 costs more for a prompt of more than 100000 tokens.
    let short = TokenUsage {
        input: 100_000,
        ..TokenUsage::default()
    };
    let long = TokenUsage {
        input: 100_001,
        ..TokenUsage::default()
    };
    let short_cost = prices.cost("claude-haiku-5-5", &short).expect("a price");
    let long_cost = prices.cost("claude-haiku-5-5", &long).expect("a price");
    assert!((short_cost - 0.01).abs() < 1e-9, "{short_cost}");
    assert!((long_cost - 0.0500005).abs() < 1e-9, "{long_cost}");
    // gpt-5.5: the cached tokens cost a tenth, and a prompt of more than 272000 tokens costs double.
    let codex = TokenUsage {
        input: 500_000,
        cache_read: 100_000,
        output: 10_000,
        ..TokenUsage::default()
    };
    let cost = prices.cost("gpt-5.5", &codex).expect("a price");
    assert!(
        (cost - (500_000.0 * 10.0 + 100_000.0 * 1.0 + 10_000.0 * 45.0) / 1e6).abs() < 1e-9,
        "{cost}"
    );
    assert_eq!(prices.cost("mock-model", &codex), None);
}

#[test]
fn every_price_in_the_bundled_table_is_positive_and_the_prefixes_are_unique() {
    let text = include_str!("../../prices.toml");
    let table: toml::Table = toml::from_str(text).expect("TOML");
    let models = table["model"].as_array().expect("models");
    let mut prefixes = std::collections::HashSet::new();
    for model in models {
        let prefix = model["prefix"].as_str().expect("a prefix");
        assert!(prefixes.insert(prefix.to_owned()), "twice: {prefix}");
        assert!(prefix == prefix.to_lowercase(), "{prefix}");
        for key in ["input", "output", "cache_read"] {
            let price = model[key]
                .as_float()
                .or_else(|| model[key].as_integer().map(|i| i as f64));
            assert!(price.is_some_and(|p| p > 0.0), "{prefix} {key}");
        }
    }
    assert!(table["meta"]["date"].as_str().is_some());
}

// --- the ledger ------------------------------------------------------------------------------

fn message(id: &str, input: u64, output: u64) -> Entry {
    Entry {
        id: format!("line-{id}-{input}-{output}"),
        at: Some(Timestamp(5)),
        sidechain: false,
        kind: Kind::Assistant {
            message: Some(id.into()),
            model: Some("claude-opus-5-5".into()),
            usage: Some(TokenUsage {
                input,
                output,
                ..TokenUsage::default()
            }),
            blocks: Vec::new(),
        },
    }
}

#[test]
fn a_message_on_several_lines_is_counted_once() {
    let prices = Prices::bundled();
    let mut ledger = Ledger::default();
    let found = deltas(
        &mut ledger,
        &prices,
        &[
            message("m1", 100, 10),
            message("m1", 100, 10),
            message("m1", 100, 10),
            message("m2", 50, 5),
        ],
    );
    let total: u64 = found.iter().map(|d| d.tokens.input).sum();
    assert_eq!((found.len(), total), (2, 150));
}

#[test]
fn a_message_whose_usage_grows_adds_only_the_growth() {
    let prices = Prices::bundled();
    let mut ledger = Ledger::default();
    let found = deltas(
        &mut ledger,
        &prices,
        &[message("m1", 100, 1), message("m1", 100, 40)],
    );
    assert_eq!(found.len(), 2);
    assert_eq!(
        found[1].tokens,
        TokenUsage {
            output: 39,
            ..TokenUsage::default()
        }
    );
}

#[test]
fn a_ledger_that_is_restored_does_not_count_a_message_twice() {
    let prices = Prices::bundled();
    let mut ledger = Ledger::default();
    deltas(&mut ledger, &prices, &[message("m1", 100, 10)]);
    let saved = serde_json::to_string(&ledger.save()).expect("JSON");
    let mut restored = Ledger::restore(serde_json::from_str(&saved).expect("saved state"));
    // The harness wrote the next block of the same message after the restart.
    assert!(deltas(&mut restored, &prices, &[message("m1", 100, 10)]).is_empty());
    assert_eq!(
        deltas(&mut restored, &prices, &[message("m2", 1, 1)]).len(),
        1
    );
}

#[test]
fn a_subscription_counts_tokens_and_no_dollars() {
    let prices = Prices::bundled();
    let mut ledger = Ledger::default();
    let found = ledger.apply(&message("m1", 100, 10), &prices, Billing::Subscription, NOW);
    assert_eq!(found[0].cost_usd, None);
    let cost = Entry {
        id: "c".into(),
        at: None,
        sidechain: false,
        kind: Kind::Cost {
            models: vec![super::ModelCost {
                model: "claude-opus-5-5".into(),
                usd: 1.0,
            }],
        },
    };
    assert!(
        ledger
            .apply(&cost, &prices, Billing::Subscription, NOW)
            .is_empty()
    );
}

#[test]
fn the_cost_of_the_harness_corrects_the_estimate_and_never_goes_down() {
    let prices = Prices::bundled();
    let mut ledger = Ledger::default();
    let estimated = ledger.apply(&message("m1", 1_000_000, 0), &prices, Billing::Api, NOW);
    assert!((estimated[0].cost_usd.expect("a cost") - 4.0).abs() < 1e-9);
    let cost = |usd: f64| Entry {
        id: "c".into(),
        at: Some(Timestamp(9)),
        sidechain: false,
        kind: Kind::Cost {
            models: vec![super::ModelCost {
                model: "claude-opus-5-5".into(),
                usd,
            }],
        },
    };
    // The harness says 4.5: the 0.5 that the transcript did not show is new.
    let first = ledger.apply(&cost(4.5), &prices, Billing::Api, NOW);
    assert!((first[0].cost_usd.expect("a cost") - 0.5).abs() < 1e-9);
    assert_eq!(first[0].tokens, TokenUsage::default());
    // The same total again, and a smaller one, add nothing.
    assert!(
        ledger
            .apply(&cost(4.5), &prices, Billing::Api, NOW)
            .is_empty()
    );
    assert!(
        ledger
            .apply(&cost(2.0), &prices, Billing::Api, NOW)
            .is_empty()
    );
}

#[test]
fn codex_usage_takes_the_model_from_the_turn_context_and_counts_a_response_once() {
    let prices = Prices::bundled();
    let mut ledger = Ledger::default();
    let model = Entry {
        id: "o1".into(),
        at: None,
        sidechain: false,
        kind: Kind::Model {
            model: "gpt-5.5".into(),
        },
    };
    let usage = |ordinal: u32| Entry {
        id: format!("o{ordinal}"),
        at: None,
        sidechain: false,
        kind: Kind::Usage {
            key: "resp_1".into(),
            model: None,
            usage: TokenUsage {
                input: 1000,
                output: 100,
                ..TokenUsage::default()
            },
        },
    };
    // Usage before any model has nothing to price, and is not lost: the key stays unseen.
    assert!(deltas(&mut ledger, &prices, &[usage(2)]).is_empty());
    let found = deltas(&mut ledger, &prices, &[model, usage(3), usage(4)]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].model, "gpt-5.5");
    assert_eq!(found[0].at, Timestamp(1_000_000));
    assert!(
        found[0]
            .cost_usd
            .is_some_and(|c| (c - (1000.0 * 5.0 + 100.0 * 30.0) / 1e6).abs() < 1e-12)
    );
}

// --- the timeline ----------------------------------------------------------------------------

fn items(format: Format, lines: &[Value]) -> Vec<TimelineItem> {
    let mut builder = super::Builder::default();
    let mut all: Vec<TimelineItem> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        for entry in parse_line(format, &line.to_string(), index as u64) {
            for item in builder.feed(&entry, NOW) {
                match all.iter_mut().find(|old| old.id == item.id) {
                    Some(old) => *old = item,
                    None => all.push(item),
                }
            }
        }
    }
    all
}

#[test]
fn a_tool_call_runs_and_then_has_a_status_and_a_duration() {
    let list = items(
        Format::Claude,
        &[
            json!({"type": "user", "uuid": "u1", "timestamp": "2026-10-08T23:05:05.000Z",
                "message": {"content": "run it"}}),
            json!({"type": "assistant", "uuid": "a1", "timestamp": "2026-10-08T23:05:06.000Z",
                "message": {"id": "m1", "model": "claude-opus-5-5", "content": [
                    {"type": "text", "text": "Sure."},
                    {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "ls"}}]}}),
            json!({"type": "user", "uuid": "r1", "timestamp": "2026-10-08T23:05:08.500Z",
                "message": {"content": [{"type": "tool_result", "tool_use_id": "t1", "content": "a.txt"}]}}),
        ],
    );
    let ids: Vec<_> = list.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, vec!["u1", "a1.0", "t1", "t1.out"]);
    assert!(list.iter().all(|i| i.turn == 1));
    assert_eq!(
        list[0].body,
        TimelineBody::Prompt {
            preview: "run it".into()
        }
    );
    assert_eq!(
        list[2].body,
        TimelineBody::Tool {
            tool: "Bash".into(),
            status: ToolStatus::Ok,
            summary: "ls".into(),
            duration_ms: Some(2500)
        }
    );
    assert_eq!(
        list[2].at,
        Timestamp(1_791_500_706_000),
        "the item keeps the time of the call"
    );
    assert_eq!(
        list[3].body,
        TimelineBody::Output {
            text: "a.txt".into(),
            truncated: false
        }
    );
}

#[test]
fn a_refused_call_and_a_failed_call_have_their_own_status() {
    let call = |id: &str| {
        json!({"type": "assistant", "uuid": format!("a-{id}"),
        "message": {"id": id, "model": "x", "content": [{"type": "tool_use", "id": id, "name": "Edit", "input": {"file_path": "/a"}}]}})
    };
    let result = |id: &str, text: &str| {
        json!({"type": "user", "uuid": format!("r-{id}"),
        "message": {"content": [{"type": "tool_result", "tool_use_id": id, "is_error": true, "content": text}]}})
    };
    let list = items(
        Format::Claude,
        &[
            call("t1"),
            result("t1", "The user doesn't want to proceed with this tool use."),
            call("t2"),
            result("t2", "File not found"),
        ],
    );
    let status = |id: &str| match &list.iter().find(|i| i.id == id).expect("an item").body {
        TimelineBody::Tool { status, .. } => *status,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        (status("t1"), status("t2")),
        (ToolStatus::Denied, ToolStatus::Failed)
    );
}

#[test]
fn a_subagent_starts_and_finishes_and_its_own_lines_stay_out_of_the_timeline() {
    let list = items(
        Format::Claude,
        &[
            json!({"type": "assistant", "uuid": "a1", "message": {"id": "m1", "model": "x", "content": [
                {"type": "tool_use", "id": "t1", "name": "Task", "input": {"subagent_type": "Explore"}}]}}),
            json!({"type": "assistant", "uuid": "a2", "isSidechain": true,
                "message": {"id": "m2", "model": "x", "content": [{"type": "text", "text": "inside"}]}}),
            json!({"type": "user", "uuid": "r1", "message": {"content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "a long report"}]}}),
        ],
    );
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0].body,
        TimelineBody::Subagent {
            agent_id: "t1".into(),
            agent_type: "Explore".into(),
            finished: true
        }
    );
}

#[test]
fn the_output_of_a_tool_is_cut_and_a_compaction_is_an_item() {
    let long = "x".repeat(3000);
    let list = items(
        Format::Claude,
        &[
            json!({"type": "assistant", "uuid": "a1", "message": {"id": "m1", "model": "x", "content": [
                {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "cat big"}}]}}),
            json!({"type": "user", "uuid": "r1", "message": {"content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": long}]}}),
            json!({"type": "system", "subtype": "compact_boundary", "uuid": "c1"}),
        ],
    );
    let TimelineBody::Output { text, truncated } = &list[1].body else {
        panic!("{:?}", list[1]);
    };
    assert_eq!((text.chars().count(), *truncated), (1000, true));
    assert_eq!(list[2].body, TimelineBody::Compaction {});
}

#[test]
fn a_codex_turn_has_one_number_and_a_claude_prompt_starts_the_next() {
    let codex = items(
        Format::Codex,
        &[
            json!({"type": "event_msg", "ordinal": 1, "payload": {"type": "task_started"}}),
            json!({"type": "event_msg", "ordinal": 2, "payload": {"type": "item_completed",
                "item": {"type": "UserMessage", "content": [{"type": "text", "text": "one"}]}}}),
            json!({"type": "event_msg", "ordinal": 3, "payload": {"type": "task_started"}}),
            json!({"type": "event_msg", "ordinal": 4, "payload": {"type": "item_completed",
                "item": {"type": "UserMessage", "content": [{"type": "text", "text": "two"}]}}}),
        ],
    );
    assert_eq!(codex.iter().map(|i| i.turn).collect::<Vec<_>>(), vec![1, 2]);
    let claude = items(
        Format::Claude,
        &[
            json!({"type": "user", "uuid": "u1", "message": {"content": "one"}}),
            json!({"type": "user", "uuid": "u2", "message": {"content": "two"}}),
        ],
    );
    assert_eq!(
        claude.iter().map(|i| i.turn).collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[test]
fn a_long_prompt_is_cut_to_a_preview_on_one_line() {
    let text = format!("first line\n\n{}", "word ".repeat(200));
    let list = items(
        Format::Claude,
        &[json!({"type": "user", "uuid": "u1", "message": {"content": text}})],
    );
    let TimelineBody::Prompt { preview } = &list[0].body else {
        panic!("a prompt");
    };
    assert!(preview.starts_with("first line word word"));
    assert_eq!(preview.chars().count(), 301);
    assert!(preview.ends_with('…') && !preview.contains('\n'));
}

// --- the golden recordings -------------------------------------------------------------------

#[test]
fn every_claude_recording_parses_and_the_cost_ends_at_the_figure_of_the_harness() {
    let prices = Prices::bundled();
    let mut with_cost = 0;
    for (name, file) in recordings("claude") {
        let all = entries(Format::Claude, &file);
        assert!(
            all.iter().any(|e| matches!(e.kind, Kind::Prompt { .. })),
            "{name}: no prompt"
        );
        let mut ledger = Ledger::default();
        let found = deltas(&mut ledger, &prices, &all);
        let cost: f64 = found.iter().filter_map(|d| d.cost_usd).sum();
        // The last `cost-state` line is the total that the harness reports.
        let reported = all
            .iter()
            .filter_map(|e| match &e.kind {
                Kind::Cost { models } => Some(models.iter().map(|m| m.usd).sum::<f64>()),
                _ => None,
            })
            .next_back();
        if let Some(reported) = reported {
            with_cost += 1;
            assert!(
                (cost - reported).abs() < 1e-9,
                "{name}: {cost} against {reported}"
            );
        }
        // Several lines of one message are one count: a message has one set of tokens.
        let messages: std::collections::HashSet<_> = all
            .iter()
            .filter_map(|e| match &e.kind {
                Kind::Assistant {
                    message,
                    usage: Some(_),
                    ..
                } => message.clone(),
                _ => None,
            })
            .collect();
        let counted = found.iter().filter(|d| !d.tokens.is_zero()).count();
        assert!(
            counted >= messages.len(),
            "{name}: {counted} counts for {} messages",
            messages.len()
        );
    }
    assert!(with_cost >= 30, "only {with_cost} recordings have a cost");
}

#[test]
fn the_tokens_of_a_claude_recording_are_the_sum_of_its_messages() {
    // `permission-allow`: 2 messages with usage; the cost line counts one more call that the
    // transcript does not show, so the tokens are a little less than the total of the harness.
    let (_, file) = recordings("claude")
        .into_iter()
        .find(|(name, _)| name == "permission-allow")
        .expect("the recording");
    let all = entries(Format::Claude, &file);
    let mut ledger = Ledger::default();
    let found = deltas(&mut ledger, &Prices::bundled(), &all);
    let mut total = TokenUsage::default();
    for delta in &found {
        total.add(&delta.tokens);
    }
    assert_eq!((total.input, total.output), (37660, 25));
}

#[test]
fn every_claude_recording_makes_a_timeline_with_the_tools_finished() {
    for (name, file) in recordings("claude") {
        let mut builder = super::Builder::default();
        let mut list: Vec<TimelineItem> = Vec::new();
        for entry in entries(Format::Claude, &file) {
            for item in builder.feed(&entry, NOW) {
                match list.iter_mut().find(|old| old.id == item.id) {
                    Some(old) => *old = item,
                    None => list.push(item),
                }
            }
        }
        assert!(
            list.iter()
                .any(|i| matches!(i.body, TimelineBody::Prompt { .. })),
            "{name}"
        );
        let ids: std::collections::HashSet<_> = list.iter().map(|i| &i.id).collect();
        assert_eq!(ids.len(), list.len(), "{name}: ids are unique");
        assert!(list.iter().all(|i| i.turn >= 1), "{name}");
    }
}

#[test]
fn every_codex_recording_parses_and_the_tokens_match_the_thread_total() {
    let prices = Prices::bundled();
    for (name, file) in recordings("codex") {
        let all = entries(Format::Codex, &file);
        assert!(all.iter().any(|e| e.kind == Kind::TurnStart), "{name}");
        let mut ledger = Ledger::default();
        let found = deltas(&mut ledger, &prices, &all);
        let mut total = TokenUsage::default();
        for delta in &found {
            total.add(&delta.tokens);
        }
        // The last record of the thread has the total of the session (a resumed one counts the
        // earlier session too).
        let text = fs::read_to_string(&file).expect("a transcript");
        let thread = text
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .rfind(|v| v["type"] == "token_usage_record")
            .map(|v| v["payload"]["thread_token_usage"].clone());
        let Some(thread) = thread else { continue };
        if name == "resume" {
            continue;
        }
        let read = total.input + total.cache_read + total.cache_write;
        assert_eq!(
            read,
            thread["input_tokens"].as_u64().expect("input"),
            "{name}"
        );
        assert_eq!(
            total.output,
            thread["output_tokens"].as_u64().expect("output"),
            "{name}"
        );
    }
}
