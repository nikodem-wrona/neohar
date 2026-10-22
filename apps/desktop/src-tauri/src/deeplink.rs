//! Links of the form `neohar://…`: `neohar://pane/w3:p5`, `neohar://interaction/i42`,
//! `neohar://task/k7`. A notification on a platform without actions uses them, and so does the
//! phone app.
//!
//! A link names something. The core knows what that is, and the host only finds the name and
//! passes it on. The host does not search, sort or decide here (the thin-presentation rule).

use serde_json::json;
use tauri::{AppHandle, Url};

use crate::{
    machines::{self, LOCAL},
    windows::{focus_any_window, window_of},
};

/// What a link names.
#[derive(Debug, PartialEq, Eq)]
pub enum Kind {
    /// A pane, by handle (`w3:p5`) or by id.
    Pane,
    /// An interaction, by handle (`i42`) or by id.
    Interaction,
    /// A task, by handle (`k7`) or by id.
    Task,
}

/// What a link names, and the machine it is for. A link with no machine is for
/// `local`: `neohar://pane/w3:p5`. A link with a machine has it before the name:
/// `neohar://pane/studio/w3:p5`.
#[derive(Debug, PartialEq, Eq)]
pub struct Target {
    pub kind: Kind,
    pub machine: String,
    pub name: String,
}

/// The target of a link, or `None` when the text is no link of the app.
pub fn parse(text: &str) -> Option<Target> {
    let url = Url::parse(text).ok()?;
    if url.scheme() != "neohar" {
        return None;
    }
    let path = url.path().strip_prefix('/')?;
    let (machine, name) = match path.split_once('/') {
        None => (LOCAL, path),
        Some((machine, name)) => {
            // The machine is a label of `config.toml`; the name has no `/`.
            if !nh_core::config::valid_machine_label(machine) || name.contains('/') {
                return None;
            }
            (machine, name)
        }
    };
    if name.is_empty() {
        return None;
    }
    let kind = match url.host_str()? {
        "pane" => Kind::Pane,
        "interaction" => Kind::Interaction,
        "task" => Kind::Task,
        _ => return None,
    };
    Some(Target {
        kind,
        machine: machine.to_owned(),
        name: name.to_owned(),
    })
}

/// Opens what a link names: the app comes to the front, and a pane gets focus in the window that
/// draws it. The link goes to the core of its machine; a machine whose link is down answers
/// nothing.
pub fn open(app: &AppHandle, target: Target) {
    focus_any_window(app);
    let Target {
        kind,
        machine,
        name,
    } = target;
    let Some(backend) = machines::of(app).and_then(|m| m.backend(&machine).ok()) else {
        tracing::info!(
            machine,
            name,
            "a link names a machine that is not connected"
        );
        return;
    };
    match kind {
        Kind::Pane => {
            let Some(window) = window_of(app, &machine) else {
                tracing::info!(name, machine, "a link names a pane, and no window shows it");
                return;
            };
            tauri::async_runtime::spawn(async move {
                let params = json!({ "window": window.to_string(), "pane": name });
                if let Err(error) = backend.call(Some(window), "pane.focus", params).await {
                    tracing::info!(%error, "a link names a pane that no window draws");
                }
            });
        }
        // A link to an interaction shows its card and never answers it.
        Kind::Interaction => crate::reach::reveal(app, &machine, Some(name)),
        // A link to a task shows its tab in the window that has the focus.
        Kind::Task => {
            let Some(window) = window_of(app, &machine) else {
                tracing::info!(name, machine, "a link names a task, and no window shows it");
                return;
            };
            tauri::async_runtime::spawn(async move {
                let shown = match backend.rpc("task.show", json!({ "task": name })).await {
                    Ok(shown) => shown,
                    Err(error) => {
                        tracing::info!(%error, "a link names a task that does not exist");
                        return;
                    }
                };
                let Some(tab) = shown["task"]["tab"].as_str() else {
                    tracing::info!(name, "the task has no tab");
                    return;
                };
                let params = json!({ "tab": tab, "window": window.to_string() });
                if let Err(error) = backend.call(Some(window), "tab.select", params).await {
                    tracing::info!(%error, "the tab of the task did not open");
                }
            });
        }
    }
}

/// Opens the links in a list of texts, for example the arguments of a second launch.
pub fn open_all<'a>(app: &AppHandle, texts: impl IntoIterator<Item = &'a str>) {
    for text in texts {
        if let Some(target) = parse(text) {
            open(app, target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(kind: Kind, machine: &str, name: &str) -> Option<Target> {
        Some(Target {
            kind,
            machine: machine.to_owned(),
            name: name.to_owned(),
        })
    }

    #[test]
    fn a_link_names_a_pane_an_interaction_or_a_task_of_local() {
        assert_eq!(
            parse("neohar://pane/w3:p5"),
            target(Kind::Pane, "local", "w3:p5")
        );
        assert_eq!(
            parse("neohar://interaction/i42"),
            target(Kind::Interaction, "local", "i42")
        );
        assert_eq!(parse("neohar://task/k7"), target(Kind::Task, "local", "k7"));
        assert_eq!(
            parse("neohar://pane/01ARZ3NDEKTSV4RRFFQ69G5FAV"),
            target(Kind::Pane, "local", "01ARZ3NDEKTSV4RRFFQ69G5FAV")
        );
    }

    #[test]
    fn a_link_with_a_machine_has_it_before_the_name() {
        assert_eq!(
            parse("neohar://pane/studio/w3:p5"),
            target(Kind::Pane, "studio", "w3:p5")
        );
        assert_eq!(
            parse("neohar://interaction/studio/i42"),
            target(Kind::Interaction, "studio", "i42")
        );
        assert_eq!(
            parse("neohar://task/build-1/k7"),
            target(Kind::Task, "build-1", "k7")
        );
    }

    #[test]
    fn anything_else_is_no_link_of_the_app() {
        for text in [
            "neohar://pane",
            "neohar://pane/",
            "neohar://pane/a/b/c",
            "neohar://pane/Studio/w3:p5",
            "neohar://pane/studio/",
            "neohar://pane//w3:p5",
            "neohar://other/x",
            "https://pane/w3:p5",
            "--flag",
            "",
        ] {
            assert_eq!(parse(text), None, "{text:?}");
        }
    }
}
