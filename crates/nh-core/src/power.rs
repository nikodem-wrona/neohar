//! The power assertion of the core.
//!
//! The core keeps the Mac out of idle sleep while agents work. Every [`POLL`] it reads the
//! settings and the state, and [`wants`] decides. While the assertion is wanted, a child
//! `caffeinate -i -w <pid>` holds it. When it is no longer wanted, the core kills the child. `-w`
//! ends the child also when the process dies without a word. Two holders run the task:
//!
//! - `nh daemon` follows `daemon.prevent_sleep`: by default while an agent run is `working` or a
//!   gate holds a hook.
//! - The app holds it only for a paired phone: while `phone.keep_awake` is on, a phone has a push
//!   subscription, and a run works, a gate holds a hook, or a push went less than [`AFTER_PUSH`]
//!   ago. Without such a phone, a person at the screen decides when the Mac sleeps. The daemon
//!   follows this rule too.
//!
//! On other systems this is a no-op.

use std::{
    sync::{Arc, Weak},
    time::Duration,
};

use nh_proto::api::model::Execution;

use crate::{config::schema::PreventSleep, runtime::Inner};

/// How often the task looks at the state. Idle sleep counts minutes, so a few seconds are enough.
pub const POLL: Duration = Duration::from_secs(5);

/// How long the app keeps the Mac awake after a push, for the reply of the person.
pub const AFTER_PUSH: Duration = Duration::from_secs(2 * 60 * 60);

/// The program that holds the assertion.
const CAFFEINATE: &str = "/usr/bin/caffeinate";

/// Who holds the assertion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holder {
    /// `nh daemon`.
    Daemon,
    /// The app that hosts the core, for a paired phone.
    App,
}

/// What [`wants`] reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AwakeFacts {
    /// `daemon.prevent_sleep`.
    pub prevent_sleep: PreventSleep,
    /// `phone.keep_awake`.
    pub keep_awake: bool,
    /// A paired phone has a push subscription.
    pub subscribed: bool,
    /// A live agent run is `working`.
    pub working: bool,
    /// A gate holds a hook.
    pub holding: bool,
    /// How long ago the push service took the last push. `None` before the first one.
    pub since_push: Option<Duration>,
}

/// Whether the holder wants the machine awake.
pub fn wants(holder: Holder, facts: &AwakeFacts) -> bool {
    let busy = facts.working || facts.holding;
    let phone = facts.keep_awake
        && facts.subscribed
        && (busy || facts.since_push.is_some_and(|age| age < AFTER_PUSH));
    match holder {
        Holder::App => phone,
        Holder::Daemon => {
            phone
                || match facts.prevent_sleep {
                    PreventSleep::Never => false,
                    PreventSleep::Always => true,
                    PreventSleep::WhileAgentsWork => busy,
                }
        }
    }
}

/// The facts of the core now.
async fn facts(inner: &Arc<Inner>) -> AwakeFacts {
    let config = inner.config.config();
    let holding = inner
        .api
        .gates
        .as_ref()
        .is_some_and(|gates| gates.holding());
    let (working, subscribed, since_push) = inner
        .actor
        .update(|parts| {
            let working = parts.state.runs.by_id.values().any(|run| {
                run.live() && matches!(run.arbiter.execution().value, Execution::Working)
            });
            (
                working,
                parts.state.phones.subscribed(),
                parts.state.native.push.since_delivered(),
            )
        })
        .await
        .unwrap_or((false, false, None));
    AwakeFacts {
        prevent_sleep: config.daemon.prevent_sleep,
        keep_awake: config.phone.keep_awake,
        subscribed,
        working,
        holding,
        since_push,
    }
}

/// Whether the core wants the machine awake now.
pub(crate) async fn wanted(inner: &Arc<Inner>, holder: Holder) -> bool {
    wants(holder, &facts(inner).await)
}

/// Holds the assertion while [`wanted`] says so, until the core ends. `pid` is the process that
/// `caffeinate -w` follows: the daemon or the app.
pub(crate) async fn run(inner: Weak<Inner>, pid: u32, holder: Holder) {
    let mut child: Option<tokio::process::Child> = None;
    loop {
        let Some(core) = inner.upgrade() else {
            return;
        };
        let want = cfg!(target_os = "macos") && wanted(&core, holder).await;
        drop(core);
        // A child that ended by itself (someone killed it) starts again on the next look.
        if let Some(running) = child.as_mut()
            && !matches!(running.try_wait(), Ok(None))
        {
            child = None;
        }
        match (want, child.is_some()) {
            (true, false) => {
                match tokio::process::Command::new(CAFFEINATE)
                    .args(["-i", "-w", &pid.to_string()])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .kill_on_drop(true)
                    .spawn()
                {
                    Ok(started) => {
                        tracing::info!(?holder, "the core keeps the machine awake");
                        child = Some(started);
                    }
                    Err(error) => tracing::warn!(%error, "caffeinate does not start"),
                }
            }
            (false, true) => {
                if let Some(mut running) = child.take() {
                    let _ = running.kill().await;
                    tracing::info!(?holder, "the core lets the machine sleep");
                }
            }
            _ => {}
        }
        tokio::time::sleep(POLL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle() -> AwakeFacts {
        AwakeFacts {
            prevent_sleep: PreventSleep::WhileAgentsWork,
            keep_awake: true,
            subscribed: true,
            working: false,
            holding: false,
            since_push: None,
        }
    }

    #[test]
    fn the_app_keeps_the_mac_awake_only_for_a_phone_with_a_subscription() {
        let working = AwakeFacts {
            working: true,
            ..idle()
        };
        assert!(wants(Holder::App, &working), "a run works");
        assert!(
            wants(
                Holder::App,
                &AwakeFacts {
                    holding: true,
                    ..idle()
                }
            ),
            "a gate holds a hook"
        );
        assert!(!wants(Holder::App, &idle()), "nothing happens");
        assert!(
            !wants(
                Holder::App,
                &AwakeFacts {
                    subscribed: false,
                    ..working
                }
            ),
            "no phone with a subscription: the person at the screen decides"
        );
        assert!(
            !wants(
                Holder::App,
                &AwakeFacts {
                    keep_awake: false,
                    ..working
                }
            ),
            "phone.keep_awake is off"
        );
        // `daemon.prevent_sleep` is for the daemon only.
        assert!(!wants(
            Holder::App,
            &AwakeFacts {
                prevent_sleep: PreventSleep::Always,
                subscribed: false,
                ..idle()
            }
        ));
    }

    #[test]
    fn a_push_keeps_the_mac_awake_for_two_hours() {
        let after = |minutes: u64| AwakeFacts {
            since_push: Some(Duration::from_secs(minutes * 60)),
            ..idle()
        };
        assert!(wants(Holder::App, &after(1)));
        assert!(wants(Holder::App, &after(119)));
        assert!(!wants(Holder::App, &after(121)));
    }

    #[test]
    fn the_daemon_follows_its_setting_and_the_rule_of_the_phone() {
        let no_phone = AwakeFacts {
            subscribed: false,
            ..idle()
        };
        let working = AwakeFacts {
            working: true,
            ..no_phone
        };
        assert!(wants(Holder::Daemon, &working));
        assert!(!wants(Holder::Daemon, &no_phone));
        assert!(!wants(
            Holder::Daemon,
            &AwakeFacts {
                prevent_sleep: PreventSleep::Never,
                ..working
            }
        ));
        assert!(wants(
            Holder::Daemon,
            &AwakeFacts {
                prevent_sleep: PreventSleep::Always,
                ..no_phone
            }
        ));
        // A push a moment ago keeps the daemon awake too.
        assert!(wants(
            Holder::Daemon,
            &AwakeFacts {
                prevent_sleep: PreventSleep::Never,
                since_push: Some(Duration::from_secs(60)),
                ..idle()
            }
        ));
    }
}
