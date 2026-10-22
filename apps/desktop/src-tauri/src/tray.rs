//! The icon in the menu bar and its menu.
//!
//! The core sends the whole model each time: the open interactions in the order of
//! `interaction.next`, and the number for the badge. The host builds the menu again. An approval
//! has a submenu with Allow, Allow always and Deny. The other interactions have one line that
//! opens the card. The last items quit the app: the one that keeps the agents, and the one that
//! stops everything.
//!
//! The tray has one section for each machine that has a model. With one machine it
//! looks as it did. With several, a line with the label of the machine opens each section, and
//! the ids of the items name the machine, so a button answers on the machine that asked.

use std::sync::Mutex;

use nh_core::host::TrayModel;
use tauri::{
    AppHandle, Manager, Wry,
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{TrayIcon, TrayIconBuilder},
};

use crate::{
    menu,
    reach::{self, Choice},
    windows::focus_any_window,
};

const ID: &str = "neohar";
const ALLOW: &str = "tray:allow:";
const ALWAYS: &str = "tray:always:";
const DENY: &str = "tray:deny:";
const OPEN: &str = "tray:open:";
const SHOW: &str = "tray:show";

/// The icon, once it exists. The app builds it on the first model.
#[derive(Default)]
pub struct TrayState(Mutex<Option<TrayIcon>>);

fn item(
    app: &AppHandle,
    id: impl Into<String>,
    text: &str,
    enabled: bool,
) -> tauri::Result<MenuItem<Wry>> {
    MenuItem::with_id(app, id.into(), text, enabled, None::<&str>)
}

/// The id of an item that names an interaction of a machine: `<prefix><machine>:<interaction>`.
/// A label has no `:`, so the machine ends at the first one.
fn entry_id(prefix: &str, machine: &str, interaction: &str) -> String {
    format!("{prefix}{machine}:{interaction}")
}

/// The machine and the interaction of an item id, after its prefix.
fn parse_entry<'a>(id: &'a str, prefix: &str) -> Option<(&'a str, &'a str)> {
    id.strip_prefix(prefix)?.split_once(':')
}

fn build_menu(app: &AppHandle, sections: &[(String, TrayModel)]) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    let any = sections.iter().any(|(_, model)| !model.entries.is_empty());
    if !any {
        menu.append(&item(app, "tray:none", "Nothing needs you", false)?)?;
    }
    let several = sections.len() > 1;
    for (machine, model) in sections {
        if several && !model.entries.is_empty() {
            menu.append(&item(app, format!("tray:head:{machine}"), machine, false)?)?;
        }
        for entry in &model.entries {
            let id = entry.interaction.map(|i| i.to_string()).unwrap_or_default();
            if entry.approval {
                let sub =
                    Submenu::with_id(app, format!("tray:sub:{machine}:{id}"), &entry.label, true)?;
                sub.append(&item(app, entry_id(ALLOW, machine, &id), "Allow", true)?)?;
                sub.append(&item(
                    app,
                    entry_id(ALWAYS, machine, &id),
                    "Allow always",
                    true,
                )?)?;
                sub.append(&item(app, entry_id(DENY, machine, &id), "Deny", true)?)?;
                sub.append(&PredefinedMenuItem::separator(app)?)?;
                sub.append(&item(app, entry_id(OPEN, machine, &id), "Open", true)?)?;
                menu.append(&sub)?;
            } else {
                menu.append(&item(
                    app,
                    entry_id(OPEN, machine, &id),
                    &entry.label,
                    true,
                )?)?;
            }
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&item(app, SHOW, "Show neohar", true)?)?;
    menu.append(&item(
        app,
        menu::QUIT,
        "Quit neohar (agents keep running)",
        true,
    )?)?;
    menu.append(&item(
        app,
        menu::QUIT_AND_STOP,
        "Quit and stop everything…",
        true,
    )?)?;
    Ok(menu)
}

/// Shows the sections, one for each machine. Call it on the main thread. The badge in the title
/// is the sum of the badges of the sections.
pub fn show(app: &AppHandle, sections: &[(String, TrayModel)]) {
    let menu = match build_menu(app, sections) {
        Ok(menu) => menu,
        Err(error) => {
            tracing::warn!(%error, "the menu of the tray did not build");
            return;
        }
    };
    let badge = sections
        .iter()
        .fold(0u32, |sum, (_, model)| sum.saturating_add(model.badge));
    let title = (badge > 0).then(|| badge.to_string());
    let state = app.state::<TrayState>();
    let mut icon = state
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(icon) = icon.as_ref() {
        let _ = icon.set_menu(Some(menu));
        let _ = icon.set_title(title);
        return;
    }
    // The glyph is a template image, black and alpha only: macOS tints it for the menu bar.
    let builder = TrayIconBuilder::with_id(ID)
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .tooltip("neohar")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()));
    match builder.build(app) {
        Ok(built) => {
            let _ = built.set_title(title);
            *icon = Some(built);
        }
        Err(error) => tracing::warn!(%error, "the tray icon did not build"),
    }
}

/// Handles an item of the menu of the tray.
fn on_menu(app: &AppHandle, id: &str) {
    let answer = |prefix: &str, choice: Choice| {
        parse_entry(id, prefix)
            .map(|(machine, interaction)| {
                reach::answer(app, machine, interaction.to_owned(), choice, "tray");
            })
            .is_some()
    };
    if answer(ALLOW, Choice::Allow)
        || answer(ALWAYS, Choice::AllowAlways)
        || answer(DENY, Choice::Deny)
    {
        return;
    }
    if let Some((machine, interaction)) = parse_entry(id, OPEN) {
        reach::reveal(app, machine, Some(interaction.to_owned()));
    } else if id == SHOW {
        focus_any_window(app);
    } else {
        crate::on_menu(app, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_of_the_tray_names_the_machine_and_the_interaction() {
        let id = entry_id(ALLOW, "studio", "01ARZ3NDEKTSV4RRFFQ69G5FAV");
        assert_eq!(id, "tray:allow:studio:01ARZ3NDEKTSV4RRFFQ69G5FAV");
        assert_eq!(
            parse_entry(&id, ALLOW),
            Some(("studio", "01ARZ3NDEKTSV4RRFFQ69G5FAV"))
        );
        // Another prefix, and an id with no machine, are no entries.
        assert_eq!(parse_entry(&id, DENY), None);
        assert_eq!(parse_entry("tray:allow:no-colon", ALLOW), None);
        assert_eq!(
            parse_entry(&entry_id(OPEN, "local", "x"), OPEN),
            Some(("local", "x"))
        );
    }
}
