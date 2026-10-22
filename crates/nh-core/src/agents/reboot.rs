//! Agents that ended with the machine ("Reboot").
//!
//! After a reboot the holders are gone, and a pane starts again with a new shell. The agent that
//! ran in it ended, but the harness kept the session, so the manifest can start it again with the
//! resume command line. `agents.resume_on_launch` says what the core does: `never` forgets the
//! runs, `always` resumes each one, and `ask` opens a notice and offers a command of the palette.

use std::sync::Arc;

use nh_proto::api::{
    calls::agent::AgentResume,
    ids::{Ref, RunId},
};

use super::{Control, runs};
use crate::{actor::Parts, config::ResumeOnLaunch, interaction::apply};

/// Whether the manifest of the run has a resume command line.
fn resumable(parts: &Parts<'_>, id: RunId) -> bool {
    let manifests = runs::loaded_manifests(parts);
    parts
        .state
        .runs
        .by_id
        .get(&id)
        .and_then(|run| runs::manifest_of(&manifests, run))
        .and_then(|manifest| manifest.resume.as_ref())
        .is_some_and(|resume| !resume.argv.is_empty())
}

fn resume_in_background(control: &Arc<Control>, run: RunId) {
    let this = Arc::clone(control);
    control.background("Resume an agent", async move {
        this.resume(AgentResume {
            agent: Ref::<RunId>::id(run),
            placement: None,
        })
        .await
        .map(|_| ())
    });
}

/// The restart of the panes is done. Applies the setting to the runs that ended with the machine.
pub(crate) fn offer(parts: &mut Parts<'_>, control: &Arc<Control>) {
    let mut candidates = std::mem::take(&mut parts.state.runs.rebooted);
    candidates.retain(|id| resumable(parts, *id));
    if candidates.is_empty() {
        return;
    }
    match parts.state.config.config().agents.resume_on_launch {
        ResumeOnLaunch::Never => {}
        ResumeOnLaunch::Always => {
            for run in candidates {
                resume_in_background(control, run);
            }
        }
        ResumeOnLaunch::Ask => {
            let count = candidates.len();
            let title = if count == 1 {
                "Resume 1 agent".to_owned()
            } else {
                format!("Resume {count} agents")
            };
            let text = format!(
                "{count} of your agents ended when this computer restarted. Their sessions are \
                 saved. Choose \"Resume the agents that ended with the restart\" in the palette \
                 to start them again."
            );
            parts.state.runs.offered = candidates;
            parts.state.runs.offer_notice = apply::open_notice(parts, &title, &text);
        }
    }
}

/// The runs that the notice offered. The notice closes.
pub(crate) fn take_offered(parts: &mut Parts<'_>) -> Vec<RunId> {
    let offered = std::mem::take(&mut parts.state.runs.offered);
    if let Some(notice) = parts.state.runs.offer_notice.take() {
        apply::dismiss_by_core(parts, notice);
    }
    offered
}
