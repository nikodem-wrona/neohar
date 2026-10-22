//! What the host asks of the core for a click, a key or a link.
//!
//! A notification, the tray, the global hotkey and a link of the form `neohar://…` all end in one
//! of two calls: answer an interaction, or show it. The host decides nothing here. The core
//! checks that the interaction is open and can be answered, and it answers a late request with an
//! error.

use nh_proto::api::ids::PaneId;
use serde_json::{Value, json};
use tauri::AppHandle;

use nh_core::machines::MachineLinks as _;

use crate::{
    machines::{self, LOCAL},
    windows::{focus_any_window, focused_window, has_window_of, open_first_window, window_of},
};

/// The decisions that a button of the host can make.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Allow,
    AllowAlways,
    Deny,
}

impl Choice {
    fn decision(self) -> Value {
        match self {
            Self::Allow => json!({ "kind": "approval", "behavior": "allow" }),
            Self::AllowAlways => {
                json!({ "kind": "approval", "behavior": "allow", "always": true })
            }
            Self::Deny => json!({ "kind": "approval", "behavior": "deny" }),
        }
    }
}

/// Answers an approval on the core of `machine`. `via` names the surface: `notification` or `tray`.
pub fn answer(
    app: &AppHandle,
    machine: &str,
    interaction: String,
    choice: Choice,
    via: &'static str,
) {
    let Some(machines) = machines::of(app) else {
        return;
    };
    let machine = machine.to_owned();
    tauri::async_runtime::spawn(async move {
        let core = match machines.backend(&machine) {
            Ok(core) => core,
            Err(error) => {
                tracing::info!(%error, machine, "the machine of the answer is not reachable");
                return;
            }
        };
        let params = json!({
            "interaction": interaction,
            "decision": choice.decision(),
            "via": via,
        });
        if let Err(error) = core.rpc("interaction.answer", params).await {
            // The core says why: the interaction is answered, closed or not answerable. For a
            // click on a notification it has shown who answered.
            tracing::info!(%error, interaction, via, machine, "the answer did not go through");
        }
    });
}

/// Shows an interaction of `machine`: the app comes to the front, and a window of that machine
/// shows the card. Without a name it is the interaction that `interaction.next` of that machine
/// gives.
pub fn reveal(app: &AppHandle, machine: &str, interaction: Option<String>) {
    let Some(machines) = machines::of(app) else {
        return;
    };
    let (app, machine) = (app.clone(), machine.to_owned());
    tauri::async_runtime::spawn(async move {
        let core = match machines.backend(&machine) {
            Ok(core) => core,
            Err(error) => {
                tracing::info!(%error, machine, "the machine of the interaction is not reachable");
                focus_any_window(&app);
                return;
            }
        };
        if !has_window_of(&app, &machine) {
            open_first_window(&core).await;
        }
        let mut params = json!({});
        if let Some(interaction) = interaction {
            params["interaction"] = json!(interaction);
        }
        if let Err(error) = core.rpc("interaction.reveal", params).await {
            tracing::info!(%error, machine, "the core did not show the interaction");
        }
        // The core asks the host to focus the window it picked. This covers a window that is not
        // built yet.
        focus_any_window(&app);
    });
}

/// Shows a pane of `machine`: the app comes to the front, and a window of that machine shows the
/// tab of the pane, in any workspace, with the focus on the pane (`go.pane`). A click on the
/// notification of a run whose turn ended does it.
pub fn show_pane(app: &AppHandle, machine: &str, pane: PaneId) {
    let Some(machines) = machines::of(app) else {
        return;
    };
    let (app, machine) = (app.clone(), machine.to_owned());
    tauri::async_runtime::spawn(async move {
        let core = match machines.backend(&machine) {
            Ok(core) => core,
            Err(error) => {
                tracing::info!(%error, machine, "the machine of the pane is not reachable");
                focus_any_window(&app);
                return;
            }
        };
        if !has_window_of(&app, &machine) {
            open_first_window(&core).await;
        }
        focus_any_window(&app);
        let Some(window) = window_of(&app, &machine) else {
            tracing::info!(machine, "no window of the machine shows the pane");
            return;
        };
        let params = json!({ "command": format!("go.pane:{pane}"), "window": window.to_string() });
        if let Err(error) = core.call(Some(window), "palette.execute", params).await {
            tracing::info!(%error, machine, "the window did not show the pane");
        }
    });
}

/// The machines to ask for the next interaction, in order: the machine of the window
/// in focus, then `local`, then the other machines in the order of `config.toml`. Each machine
/// comes once.
pub fn next_order(focused: Option<&str>, configured: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for machine in focused
        .into_iter()
        .chain(std::iter::once(LOCAL))
        .chain(configured.iter().map(String::as_str))
    {
        if !out.iter().any(|known| known == machine) {
            out.push(machine.to_owned());
        }
    }
    out
}

/// Shows the card that waits longest on any machine: the global hotkey, and a click
/// that names no interaction. The first machine with an interaction wins. When nothing waits, the
/// app only comes to the front.
pub fn reveal_next(app: &AppHandle) {
    let Some(machines) = machines::of(app) else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let focused = focused_window(&app).map(|(machine, _)| machine);
        let configured: Vec<String> = machines
            .manager()
            .map(|manager| manager.rows().into_iter().map(|row| row.label).collect())
            .unwrap_or_default();
        for machine in next_order(focused.as_deref(), &configured) {
            // A machine whose link is down has nothing to show.
            let Ok(core) = machines.backend(&machine) else {
                continue;
            };
            let Ok(next) = core.rpc("interaction.next", json!({})).await else {
                continue;
            };
            if let Some(interaction) = next["interaction"].as_str() {
                reveal(&app, &machine, Some(interaction.to_owned()));
                return;
            }
        }
        reveal(&app, LOCAL, None);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_hotkey_asks_the_focused_machine_then_local_then_the_others_in_file_order() {
        let configured = labels(&["studio", "build", "lab"]);
        assert_eq!(
            next_order(Some("build"), &configured),
            ["build", "local", "studio", "lab"]
        );
        assert_eq!(
            next_order(Some("local"), &configured),
            ["local", "studio", "build", "lab"]
        );
        assert_eq!(
            next_order(None, &configured),
            ["local", "studio", "build", "lab"]
        );
        assert_eq!(next_order(None, &[]), ["local"]);
        // A machine that is not in the file (a window that outlives its machine) comes first once.
        assert_eq!(
            next_order(Some("gone"), &configured),
            ["gone", "local", "studio", "build", "lab"]
        );
    }
}
