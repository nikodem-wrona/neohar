//! The part of the tailer that runs on the actor: it writes the counts, changes the runs and
//! publishes.

use nh_agents::{
    adapter::AdapterSignal,
    harness::name as harness_name,
    transcript::{LimitWindow, UsageDelta},
};
use nh_proto::api::{
    common::Harness,
    ids::RunId,
    model::{Execution, LimitValue, RateLimitWindow, Usage},
    projections::Projection,
    time::Timestamp,
};
use nh_store::UsageTotals;

use super::{
    Limits, day,
    tail::{Batch, TimelineUpdate},
};
use crate::{actor::Parts, agents::runs, persist::now_ms};

const HARNESSES: [Harness; 2] = [Harness::Claude, Harness::Codex];

fn usage_of(totals: &UsageTotals) -> Usage {
    Usage {
        input_tokens: totals.input,
        output_tokens: totals.output,
        cache_read_tokens: totals.cache_read,
        cache_write_tokens: totals.cache_write,
        cost_usd: totals.cost_usd,
    }
}

fn add(usage: &mut Usage, delta: &UsageDelta) {
    usage.input_tokens += delta.tokens.input;
    usage.output_tokens += delta.tokens.output;
    usage.cache_read_tokens += delta.tokens.cache_read;
    usage.cache_write_tokens += delta.tokens.cache_write;
    if let Some(cost) = delta.cost_usd {
        usage.cost_usd = Some(usage.cost_usd.unwrap_or(0.0) + cost);
    }
}

/// After a start of the core: the runs show the usage that the table has, and `limits` shows the
/// usage of today.
pub(crate) fn started(parts: &mut Parts<'_>) {
    let totals = parts.commit(|tx| tx.usage_by_run()).unwrap_or_default();
    for (id, totals) in totals {
        if let Some(run) = parts.state.runs.by_id.get_mut(&RunId::new(id)) {
            run.usage = usage_of(&totals);
        }
    }
    let today = day(Timestamp(now_ms()));
    let rows = parts
        .commit(|tx| tx.usage_on_day(&today))
        .unwrap_or_default();
    set_today(&mut parts.state.limits, &today, rows);
    publish_limits(parts);
}

fn set_today(limits: &mut Limits, today: &str, rows: Vec<(String, UsageTotals)>) {
    limits.today.clear();
    for (name, totals) in rows {
        let harness = match name.as_str() {
            "claude" => Harness::Claude,
            "codex" => Harness::Codex,
            _ => continue,
        };
        limits
            .today
            .insert(harness, (today.to_owned(), usage_of(&totals)));
    }
}

fn window(found: &LimitWindow) -> RateLimitWindow {
    RateLimitWindow {
        name: found.name.clone(),
        used_percent: found.used_percent,
        resets_at: found.resets_at,
    }
}

/// The windows of a harness: those that its transcript reported, and a full window for each run
/// that is rate limited now.
fn windows_of(parts: &Parts<'_>, harness: Harness) -> Vec<RateLimitWindow> {
    let mut windows = parts
        .state
        .limits
        .windows
        .get(&harness)
        .cloned()
        .unwrap_or_default();
    for run in parts.state.runs.by_id.values() {
        if run.harness != harness || !run.live() {
            continue;
        }
        if let Execution::RateLimited { resets_at } = run.arbiter.execution().value {
            windows.push(RateLimitWindow {
                name: "limit".to_owned(),
                used_percent: 100.0,
                resets_at,
            });
        }
    }
    windows
}

/// Publishes the row of each harness in `limits`. A harness with no window and no use today has no
/// row.
pub(crate) fn publish_limits(parts: &mut Parts<'_>) {
    let today = day(Timestamp(now_ms()));
    for harness in HARNESSES {
        let windows = windows_of(parts, harness);
        let usage_today = parts
            .state
            .limits
            .today
            .get(&harness)
            .filter(|(day, _)| *day == today)
            .map(|(_, usage)| usage.clone())
            .unwrap_or_default();
        let key = harness_name(harness);
        let empty = windows.is_empty() && usage_today == Usage::default();
        if empty {
            parts.projections.remove(Projection::Limits, key);
            continue;
        }
        let value = LimitValue {
            account: "default".to_owned(),
            harness,
            windows,
            usage_today,
        };
        if let Ok(value) = serde_json::to_value(value) {
            parts.projections.set(Projection::Limits, key, value);
        }
    }
}

/// Applies what a poll of the transcript of a run found.
pub(super) fn apply(parts: &mut Parts<'_>, run: RunId, batch: Batch) {
    let Batch {
        offset,
        ledger_json,
        deltas,
        tool_ends,
        limits,
        timeline,
    } = batch;
    let Some(harness) = parts.state.runs.by_id.get(&run).map(|run| run.harness) else {
        return;
    };
    let mut changed = false;
    if ledger_json.is_some() || !deltas.is_empty() {
        let today = day(Timestamp(now_ms()));
        let ulid = run.ulid();
        let stored = parts.commit(|tx| {
            for delta in &deltas {
                tx.add_usage(
                    ulid,
                    &day(delta.at),
                    &delta.model,
                    delta.tokens.input,
                    delta.tokens.output,
                    delta.tokens.cache_read,
                    delta.tokens.cache_write,
                    delta.cost_usd,
                )?;
            }
            if let Some(json) = &ledger_json {
                tx.set_transcript_state(ulid, offset, json)?;
            }
            tx.usage_on_day(&today)
        });
        match stored {
            Ok(rows) => {
                set_today(&mut parts.state.limits, &today, rows);
                changed = !deltas.is_empty();
            }
            Err(error) => tracing::warn!(%error, "the usage of a transcript is not stored"),
        }
    }
    let windows = limits.map(|found| found.iter().map(window).collect::<Vec<_>>());
    if let Some(windows) = &windows {
        parts.state.limits.windows.insert(harness, windows.clone());
    }
    if let Some(run) = parts.state.runs.by_id.get_mut(&run) {
        for delta in &deltas {
            add(&mut run.usage, delta);
        }
        if let Some(windows) = &windows {
            run.rate_limit = windows
                .iter()
                .max_by(|a, b| a.used_percent.total_cmp(&b.used_percent))
                .cloned();
        }
    }
    // A tool result ends the request that the call made, whoever answered it.
    if !tool_ends.is_empty() {
        for end in &tool_ends {
            let signal = [AdapterSignal::ToolEnded {
                call_id: end.call.clone(),
                tool: String::new(),
                ok: end.ok,
                summary: None,
            }];
            crate::interaction::apply::signals(parts, run, &signal, Vec::new(), end.at, None);
        }
    }
    if changed || windows.is_some() {
        runs::publish(parts, run);
        publish_limits(parts);
    }
    if let Some(update) = timeline {
        publish_timeline(parts, run, update);
    }
}

fn publish_timeline(parts: &Parts<'_>, run: RunId, update: TimelineUpdate) {
    let projection = Projection::Timeline(run);
    let hub = parts.projections;
    // The projection is never dropped: a window may have subscribed since the poll. Its rows go.
    let clear = |keep: &[String]| {
        let stale: Vec<String> = hub
            .snapshot(projection)
            .items
            .keys()
            .filter(|key| !keep.contains(key))
            .cloned()
            .collect();
        for key in stale {
            hub.remove(projection, &key);
        }
    };
    match update {
        TimelineUpdate::Drop => clear(&[]),
        TimelineUpdate::Set {
            reset,
            upserts,
            evicted,
            order,
        } => {
            if reset {
                clear(&order);
            }
            for item in upserts {
                if let Ok(value) = serde_json::to_value(&item) {
                    hub.set(projection, &item.id, value);
                }
            }
            for id in evicted {
                hub.remove(projection, &id);
            }
            hub.set_order(projection, &order);
        }
    }
}
