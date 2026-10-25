//! Links of the form `neohar://…`: `neohar://pane/w3:p5` and `neohar://interaction/i42`. A
//! script or a page can show a pane or a card with them. neohar itself makes none: its
//! notifications answer through the notification centre, and the phone app has routes of its own.
//!
//! A link names something. The core knows what that is, and the host only finds the name and
//! passes it on. The host does not search, sort or decide here (the thin-presentation rule).

use tauri::{AppHandle, Manager, Url};

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
        _ => return None,
    };
    Some(Target {
        kind,
        machine: machine.to_owned(),
        name: name.to_owned(),
    })
}

/// Opens what a link names. Any page of the web can open a link, and neohar makes none of its
/// own, so a link never moves the keyboard to a pane: a pane that gets the focus releases the
/// approval that neohar holds for it, and the keys that the person types for another app would go
/// to the dialog of the harness.
///
/// - A link to an interaction shows its card (`interaction.reveal`, which does not focus the
///   pane) and never answers it.
/// - A link to a pane brings a window of its machine to the front, and the pane does not get the
///   focus.
///
/// The link goes to the core of its machine; a machine whose link is down answers nothing.
pub fn open(app: &AppHandle, target: Target) {
    let Target {
        kind,
        machine,
        name,
    } = target;
    if machines::of(app)
        .and_then(|m| m.backend(&machine).ok())
        .is_none()
    {
        focus_any_window(app);
        tracing::info!(
            machine,
            name,
            "a link names a machine that is not connected"
        );
        return;
    }
    match action_of(&kind) {
        Action::ShowWindow => {
            show_window_of(app, &machine);
            tracing::info!(
                machine,
                name,
                "a link names a pane: a window of its machine comes to the front"
            );
        }
        Action::RevealCard => crate::reach::reveal(app, &machine, Some(name)),
    }
}

/// What the host does for a link. No action gives a pane the focus or answers a card.
#[derive(Debug, PartialEq, Eq)]
enum Action {
    /// A window of the machine comes to the front.
    ShowWindow,
    /// `interaction.reveal`: a window shows the card.
    RevealCard,
}

fn action_of(kind: &Kind) -> Action {
    match kind {
        Kind::Pane => Action::ShowWindow,
        Kind::Interaction => Action::RevealCard,
    }
}

/// Brings a window of `machine` to the front, or any window when none shows the machine.
fn show_window_of(app: &AppHandle, machine: &str) {
    let window = window_of(app, machine)
        .and_then(|id| app.get_webview_window(&machines::label_of(machine, &id.to_string())));
    match window {
        Some(window) => {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        None => focus_any_window(app),
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
    fn a_link_names_a_pane_or_an_interaction_of_local() {
        assert_eq!(
            parse("neohar://pane/w3:p5"),
            target(Kind::Pane, "local", "w3:p5")
        );
        assert_eq!(
            parse("neohar://interaction/i42"),
            target(Kind::Interaction, "local", "i42")
        );
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
    }

    #[test]
    fn a_link_never_gives_a_pane_the_focus() {
        // A pane with the focus would release its held approval.
        assert_eq!(action_of(&Kind::Pane), Action::ShowWindow);
        assert_eq!(action_of(&Kind::Interaction), Action::RevealCard);
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
            "neohar://task/k7",
            "https://pane/w3:p5",
            "--flag",
            "",
        ] {
            assert_eq!(parse(text), None, "{text:?}");
        }
    }
}
