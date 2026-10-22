//! Version drift telemetry.
//!
//! The core counts, for each version of each harness, what it saw and how much of it was wrong:
//! a lower source that disagreed with a structured one, an answer that failed, and an interaction
//! that the harness resolved by a path the core did not take. A version with at least
//! [`MIN_OBSERVATIONS`] observations and a bad share of [`BAD_RATE`] raises `agent.drift_detected`
//! once, with a line in the doctor and a notification. The numbers stay on this machine.

use nh_agents::harness::name as harness_name;
use nh_proto::api::{
    common::{Harness, Urgency},
    events::{AgentDriftDetected, EventPayload},
    model::InteractionState,
};

use crate::{
    actor::Parts,
    host::{Host as _, HostNotification},
    interaction::model::Interaction,
};

/// How many observations a version needs before the alarm can go off.
pub(crate) const MIN_OBSERVATIONS: u32 = 20;
/// The share of bad signals, from 0 to 1, at which the alarm goes off.
pub(crate) const BAD_RATE: f32 = 0.25;

/// Whether the counters of a version raise the alarm now.
pub(crate) fn alarm(observations: u32, bad_signals: u32, raised: bool) -> bool {
    !raised
        && observations >= MIN_OBSERVATIONS
        && f64::from(bad_signals) >= f64::from(observations) * f64::from(BAD_RATE)
}

fn display_name(harness: Harness) -> &'static str {
    match harness {
        Harness::Claude => "Claude Code",
        Harness::Codex => "Codex",
    }
}

fn warning_id(harness: Harness, version: &str) -> String {
    format!("drift:{}:{version}", harness_name(harness))
}

fn warn(parts: &Parts<'_>, harness: Harness, version: &str) {
    parts.state.health.warn(
        &warning_id(harness, version),
        format!(
            "{} {version} behaves differently than neohar expects.",
            display_name(harness)
        ),
        Some(format!(
            "Run `nh integration doctor {}`.",
            harness_name(harness)
        )),
    );
}

/// Adds to the counters of the version that a run has, and raises the alarm when they say so.
pub(crate) fn record(
    parts: &mut Parts<'_>,
    harness: Harness,
    version: Option<&str>,
    observations: u32,
    bad_signals: u32,
) {
    let Some(version) = version else {
        return;
    };
    let name = harness_name(harness);
    let stored = parts.commit(|tx| {
        let row = tx.add_drift(name, version, observations, bad_signals)?;
        if !alarm(row.observations, row.bad_signals, row.raised) {
            return Ok(None);
        }
        tx.mark_drift_raised(name, version)?;
        tx.append(
            Some(name),
            &EventPayload::AgentDriftDetected(AgentDriftDetected {
                harness,
                version: version.to_owned(),
                observations: row.observations,
                bad_signal_rate: row.bad_signals as f32 / row.observations as f32,
            }),
        )?;
        Ok(Some(row))
    });
    match stored {
        Ok(Some(_)) => {
            warn(parts, harness, version);
            if let Some(host) = parts.state.host.clone() {
                host.notify(HostNotification {
                    title: format!(
                        "{} {version} behaves differently than neohar expects",
                        display_name(harness)
                    ),
                    body: format!("Run `nh integration doctor {name}`."),
                    urgency: Urgency::Normal,
                    interaction: None,
                    pane: None,
                    actions: Vec::new(),
                });
            }
        }
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, "the drift counters are not stored"),
    }
}

/// An interaction changed state. The ends that tell something about the harness count.
pub(crate) fn interaction_changed(
    parts: &mut Parts<'_>,
    before: &Interaction,
    after: &Interaction,
) {
    if before.state == after.state {
        return;
    }
    // (an observation, and whether it was a bad signal)
    let seen = match after.state {
        InteractionState::Delivered => Some(false),
        InteractionState::Failed | InteractionState::DeliveryUnknown => Some(true),
        // The harness resolved it. It is a bad signal when the core had a decision to deliver:
        // the harness took a path that the core did not take.
        InteractionState::ResolvedElsewhere => Some(after.delivery.decision.is_some()),
        _ => None,
    };
    let Some(bad) = seen else {
        return;
    };
    let Some(run) = after.run.and_then(|run| parts.state.runs.by_id.get(&run)) else {
        return;
    };
    let (harness, version) = (run.harness, run.version.as_ref().map(ToString::to_string));
    record(parts, harness, version.as_deref(), 1, u32::from(bad));
}

/// The lines of the doctor for the versions that raised the alarm before the core started.
pub(crate) fn started(parts: &mut Parts<'_>) {
    let rows = parts.commit(|tx| tx.drift_rows()).unwrap_or_default();
    for row in rows.into_iter().filter(|row| row.raised) {
        let harness = match row.harness.as_str() {
            "claude" => Harness::Claude,
            "codex" => Harness::Codex,
            _ => continue,
        };
        warn(parts, harness, &row.version);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use nh_proto::api::projections::Projection;

    use crate::{agents::tests::Fixture, host::HostCall};

    #[test]
    fn the_alarm_needs_enough_observations_and_a_bad_share() {
        assert!(!alarm(19, 19, false), "too few observations");
        assert!(!alarm(20, 4, false), "a share of 20%");
        assert!(alarm(20, 5, false), "a share of 25%");
        assert!(alarm(100, 90, false));
        assert!(!alarm(100, 90, true), "it goes once");
    }

    #[tokio::test]
    async fn a_version_that_goes_wrong_raises_the_alarm_once() {
        let mut f = Fixture::new();
        f.update(|parts| record(parts, Harness::Claude, Some("2.2.0"), 15, 2))
            .await;
        assert!(!f.kinds().contains(&"agent.drift_detected".to_owned()));
        // 20 observations with 5 bad ones is a share of 25%.
        f.update(|parts| record(parts, Harness::Claude, Some("2.2.0"), 5, 3))
            .await;
        assert!(f.kinds().contains(&"agent.drift_detected".to_owned()));
        let notified = f.host.calls().into_iter().any(|call| {
            matches!(&call, HostCall::Notify(_, n)
                if n.title == "Claude Code 2.2.0 behaves differently than neohar expects")
        });
        assert!(notified, "{:?}", f.host.calls());
        f.hub.flush();
        let health = f.hub.snapshot(Projection::Health);
        let text = serde_json::to_string(&health.items).expect("json");
        assert!(text.contains("drift:claude:2.2.0"), "{text}");
        // More bad signals do not raise it again, and another version has its own counters.
        f.update(|parts| record(parts, Harness::Claude, Some("2.2.0"), 10, 10))
            .await;
        assert!(!f.kinds().contains(&"agent.drift_detected".to_owned()));
        f.update(|parts| record(parts, Harness::Claude, Some("2.1.292"), 30, 0))
            .await;
        assert!(!f.kinds().contains(&"agent.drift_detected".to_owned()));
    }

    #[tokio::test]
    async fn a_run_with_no_version_counts_for_nothing() {
        let mut f = Fixture::new();
        f.update(|parts| record(parts, Harness::Codex, None, 40, 40))
            .await;
        assert!(!f.kinds().contains(&"agent.drift_detected".to_owned()));
    }
}
