//! The screen watcher of a pane.
//!
//! One task for each pane. It wakes when the pane has new output and looks at the screen at most
//! ten times a second. It copies the rows that the rules of the screen manifest read, runs the
//! detector, and tells the actor only when what the rules read changed. A pane without a live run
//! of a harness that has a screen manifest costs a look at the actor once a second.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use nh_agents::screen::{self, Detector, Evaluation};
use nh_proto::api::ids::{PaneId, RunId};
use tokio::task::JoinHandle;

use super::{Agents, runs};
use crate::pane::{Liveness, Pane};

/// The shortest time between two looks (10 Hz).
const MIN_GAP: Duration = Duration::from_millis(1000 / screen::engine::MAX_RATE_HZ as u64);

/// How long the watcher keeps what it learned about the run of the pane.
const TARGET_FRESH: Duration = Duration::from_secs(1);

/// The rule of a screen manifest that says that the harness asks for a login.
const LOGIN_RULE: &str = "login_required";

/// The run that the watcher looks at, and the rules for it.
struct Active {
    run: RunId,
    manifest: String,
    /// The id of the harness manifest of the run.
    harness: String,
    detector: Detector,
    last: Option<runs::ScreenSeen>,
    /// The screen showed the rule `login_required` at the last look.
    login: bool,
}

/// What the rules read in an evaluation.
fn seen_of(evaluation: &Evaluation) -> runs::ScreenSeen {
    runs::ScreenSeen {
        state: evaluation.state(),
        dialog: evaluation.dialog().cloned(),
    }
}

pub(super) fn spawn(agents: &Arc<Agents>, pane: Arc<Pane>) -> JoinHandle<()> {
    let agents = Arc::clone(agents);
    agents.handle.clone().spawn(watch(agents, pane))
}

async fn watch(agents: Arc<Agents>, pane: Arc<Pane>) {
    let id = PaneId::new(pane.id());
    let mut damage = pane.subscribe_damage();
    let mut active: Option<Active> = None;
    let mut checked: Option<Instant> = None;
    let mut last_look = Instant::now() - MIN_GAP;
    let mut wake = Instant::now();
    loop {
        tokio::select! {
            changed = damage.changed() => {
                if changed.is_err() {
                    return;
                }
            }
            () = tokio::time::sleep_until(tokio::time::Instant::from_std(wake)) => {}
        }
        if let Liveness::Exited(_) = pane.info().liveness {
            return;
        }
        let since = last_look.elapsed();
        if since < MIN_GAP {
            tokio::time::sleep(MIN_GAP - since).await;
        }
        // The run of the pane, and its screen manifest. A look at the actor costs a message.
        if checked.is_none_or(|at| at.elapsed() >= TARGET_FRESH) {
            checked = Some(Instant::now());
            let target = agents
                .actor
                .update(move |parts| runs::screen_target(&parts, id))
                .await
                .ok()
                .flatten();
            let previous = active.take();
            active = match target {
                None => None,
                Some((run, manifest, harness)) => match previous {
                    Some(current) if current.run == run && current.manifest == manifest => {
                        Some(current)
                    }
                    _ => screen::builtin(&manifest).map(|compiled| Active {
                        run,
                        manifest,
                        harness,
                        detector: Detector::new(Arc::new(compiled)),
                        last: None,
                        login: false,
                    }),
                },
            };
        }
        let Some(current) = active.as_mut() else {
            wake = Instant::now() + TARGET_FRESH;
            continue;
        };
        let (_, rows) = pane.size();
        let screen = pane.screen_rows(current.detector.rows_needed(rows));
        last_look = Instant::now();
        let evaluation = current.detector.evaluate(&screen, last_look);
        wake = last_look
            + evaluation
                .recheck_after
                .unwrap_or(Duration::from_secs(3_600));
        // A screen that asks for a login makes the harness `logged_out`, with the source `run`.
        let login = evaluation.rule(LOGIN_RULE).is_some();
        if login
            && !current.login
            && let Some(discovery) = agents.discovery.get()
        {
            discovery.run_logged_out(&current.harness);
        }
        current.login = login;
        let seen = seen_of(&evaluation);
        if current.last.as_ref() != Some(&seen) {
            current.last = Some(seen.clone());
            let run = current.run;
            agents
                .actor
                .update_detached(move |mut parts| runs::screen(&mut parts, run, &seen));
        }
    }
}
