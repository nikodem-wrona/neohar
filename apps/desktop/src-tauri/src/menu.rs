//! The menu of the app.
//!
//! The accelerators are items of the native menu, so a focused terminal cannot swallow them: the
//! page never sees ⌘Q, ⌘W, ⌘N or ⌘,.

use nh_proto::api::model::{MachineState, MachineValue};
use tauri::{
    AppHandle, Wry,
    menu::{
        Menu, MenuBuilder, MenuItem, MenuItemBuilder, MenuItemKind, PredefinedMenuItem, Submenu,
        SubmenuBuilder,
    },
};

use crate::machines::LOCAL;

/// Ids of the items that the app handles itself.
pub const QUIT: &str = "quit";
pub const QUIT_AND_STOP: &str = "quit_and_stop";
pub const NEW_WINDOW: &str = "new_window";
pub const CLOSE_WINDOW: &str = "close_window";
pub const SETTINGS: &str = "settings";
/// The Machines menu. The host fills it from the projection `machines`.
pub const MACHINES: &str = "machines";
const MACHINE_NEW: &str = "machine:new:";
const MACHINE_CONNECT: &str = "machine:connect:";
const MACHINE_DISCONNECT: &str = "machine:disconnect:";

/// Builds the menu of the app.
pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let settings = MenuItemBuilder::with_id(SETTINGS, "Settings…")
        .accelerator("CmdOrCtrl+,")
        .build(app)?;
    let quit = MenuItemBuilder::with_id(QUIT, "Quit neohar (agents keep running)")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;
    let quit_and_stop = MenuItemBuilder::with_id(QUIT_AND_STOP, "Quit and stop everything…")
        .accelerator("CmdOrCtrl+Alt+Q")
        .build(app)?;
    let new_window = MenuItemBuilder::with_id(NEW_WINDOW, "New window")
        .accelerator("CmdOrCtrl+N")
        .build(app)?;
    let close_window = MenuItemBuilder::with_id(CLOSE_WINDOW, "Close window")
        .accelerator("CmdOrCtrl+W")
        .build(app)?;

    let app_menu = SubmenuBuilder::new(app, "neohar")
        .about(None)
        .separator()
        .item(&settings)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .item(&quit)
        .item(&quit_and_stop)
        .build()?;
    // The items come with the first model of `machines`.
    let machines = SubmenuBuilder::with_id(app, MACHINES, "Machines").build()?;
    let file = SubmenuBuilder::new(app, "File")
        .item(&new_window)
        .item(&close_window)
        .build()?;
    // The edit items are for the text fields of the page (the palette, the settings). A terminal
    // surface takes copy and paste from the page itself.
    let edit = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;
    let window = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .item(&PredefinedMenuItem::fullscreen(app, None)?)
        .build()?;
    MenuBuilder::new(app)
        .item(&app_menu)
        .item(&file)
        .item(&machines)
        .item(&edit)
        .item(&window)
        .build()
}

/// An item of the Machines menu, before it is a native item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub text: &'static str,
    pub enabled: bool,
}

fn entry(prefix: &str, label: &str, text: &'static str, enabled: bool) -> Entry {
    Entry {
        id: format!("{prefix}{label}"),
        text,
        enabled,
    }
}

/// The items of one machine: "New window", "Connect" and "Disconnect". `local` has
/// only the first. "Connect" works when the link is down, and "Disconnect" when it is up or on its
/// way.
pub fn machine_entries(row: &MachineValue) -> Vec<Entry> {
    let label = row.label.as_str();
    let mut out = vec![entry(MACHINE_NEW, label, "New window", true)];
    if label != LOCAL {
        let down = row.state == MachineState::Disconnected;
        out.push(entry(MACHINE_CONNECT, label, "Connect", down));
        out.push(entry(MACHINE_DISCONNECT, label, "Disconnect", !down));
    }
    out
}

/// The title of the submenu of a machine: its label and how the link is.
pub fn machine_title(row: &MachineValue) -> String {
    match row.state {
        MachineState::Local => format!("{} (this Mac)", row.label),
        MachineState::Connected => format!("{} · connected", row.label),
        MachineState::Connecting => format!("{} · connecting", row.label),
        MachineState::Disconnected => format!("{} · disconnected", row.label),
    }
}

/// What an item of the Machines menu asks of the local core: the method and the machine.
pub fn machine_action(id: &str) -> Option<(&'static str, &str)> {
    if let Some(label) = id.strip_prefix(MACHINE_NEW) {
        Some(("machine.open_window", label))
    } else if let Some(label) = id.strip_prefix(MACHINE_CONNECT) {
        Some(("machine.connect", label))
    } else {
        id.strip_prefix(MACHINE_DISCONNECT)
            .map(|label| ("machine.disconnect", label))
    }
}

/// Fills the Machines menu from the rows of `machines`, in their order. Call it on the main thread.
pub fn update_machines(app: &AppHandle, rows: &[MachineValue]) {
    if let Err(error) = fill_machines(app, rows) {
        tracing::warn!(%error, "the Machines menu did not build");
    }
}

fn fill_machines(app: &AppHandle, rows: &[MachineValue]) -> tauri::Result<()> {
    let Some(menu) = app.menu() else {
        return Ok(());
    };
    let Some(MenuItemKind::Submenu(machines)) = menu.get(MACHINES) else {
        return Ok(());
    };
    for old in machines.items()? {
        machines.remove(&old)?;
    }
    for row in rows {
        let sub = Submenu::with_id(
            app,
            format!("machine:sub:{}", row.label),
            machine_title(row),
            true,
        )?;
        for entry in machine_entries(row) {
            sub.append(&MenuItem::with_id(
                app,
                entry.id,
                entry.text,
                entry.enabled,
                None::<&str>,
            )?)?;
        }
        machines.append(&sub)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(label: &str, state: MachineState) -> MachineValue {
        MachineValue {
            label: label.to_owned(),
            state,
            latency_ms: None,
            version: None,
            reason: None,
            forwards: Vec::new(),
            harnesses: Vec::new(),
            update: None,
        }
    }

    fn ids(entries: &[Entry]) -> Vec<(&str, bool)> {
        entries.iter().map(|e| (e.text, e.enabled)).collect()
    }

    #[test]
    fn each_machine_has_new_window_connect_and_disconnect() {
        assert_eq!(
            ids(&machine_entries(&row("studio", MachineState::Disconnected))),
            [
                ("New window", true),
                ("Connect", true),
                ("Disconnect", false)
            ]
        );
        assert_eq!(
            ids(&machine_entries(&row("studio", MachineState::Connected))),
            [
                ("New window", true),
                ("Connect", false),
                ("Disconnect", true)
            ]
        );
        assert_eq!(
            ids(&machine_entries(&row("studio", MachineState::Connecting))),
            [
                ("New window", true),
                ("Connect", false),
                ("Disconnect", true)
            ]
        );
        // This Mac has a new window only: it has no link.
        assert_eq!(
            ids(&machine_entries(&row("local", MachineState::Local))),
            [("New window", true)]
        );
    }

    #[test]
    fn an_item_asks_the_core_for_the_method_of_its_machine() {
        let entries = machine_entries(&row("studio", MachineState::Disconnected));
        let actions: Vec<_> = entries.iter().map(|e| machine_action(&e.id)).collect();
        assert_eq!(
            actions,
            [
                Some(("machine.open_window", "studio")),
                Some(("machine.connect", "studio")),
                Some(("machine.disconnect", "studio")),
            ]
        );
        assert_eq!(machine_action("quit"), None);
        assert_eq!(machine_action("machine:sub:studio"), None);
    }

    #[test]
    fn the_title_says_how_the_link_is() {
        assert_eq!(
            machine_title(&row("local", MachineState::Local)),
            "local (this Mac)"
        );
        assert_eq!(
            machine_title(&row("studio", MachineState::Connecting)),
            "studio · connecting"
        );
    }
}
