//! The projections that the layout feeds: `tree` (one row for each workspace, with its
//! tabs and their layout), `windows` (one row for each window, with the panes it draws) and the
//! static part of `panes` (the place and the size of each pane).
//!
//! The hub drops a `set` of a value that the row has, so a command can publish every workspace
//! and the patch holds only the ones that changed.

use nh_proto::api::{
    ids::{PaneId, WindowId},
    projections::Projection,
};

use super::model::Layout;
use crate::{palette, projection::ProjectionHub};

/// Publishes the row of one pane in `panes`, after the pane runtime reported a change.
pub fn publish_pane(hub: &ProjectionHub, layout: &Layout, pane: PaneId) {
    if let Some(entry) = layout.panes.get(&pane)
        && let Ok(value) = serde_json::to_value(layout.pane_value(entry))
    {
        hub.set(Projection::Panes, &pane.to_string(), value);
    }
}

/// Publishes the commands of the palette: the fixed ones and the ones that go to an object. The
/// state actor calls it when the layout, a title or the health of the store changes.
pub fn publish_palette(hub: &ProjectionHub, layout: &Layout, degraded: Option<&str>) {
    let value = nh_proto::api::model::PaletteValue {
        commands: palette::commands(layout, degraded),
    };
    if let Ok(value) = serde_json::to_value(value) {
        hub.set_singleton(Projection::Palette, value);
    }
}

/// Publishes the row of one window in `windows`, after the host reported its state.
pub fn publish_window(hub: &ProjectionHub, layout: &Layout, window: WindowId) {
    if let Some(index) = layout.window_index(window)
        && let Ok(value) = serde_json::to_value(layout.window_value(&layout.windows[index]))
    {
        hub.set(Projection::Windows, &window.to_string(), value);
    }
}

/// Brings the projections from the layout `old` (what they show) to `new`.
pub fn publish(hub: &ProjectionHub, old: &Layout, new: &Layout) {
    for workspace in &old.workspaces {
        if new.workspace_index(workspace.id).is_none() {
            hub.remove(Projection::Tree, &workspace.id.to_string());
        }
    }
    for workspace in &new.workspaces {
        if let Ok(value) = serde_json::to_value(new.workspace_value(workspace)) {
            hub.set(Projection::Tree, &workspace.id.to_string(), value);
        }
    }
    let order: Vec<String> = new.workspaces.iter().map(|w| w.id.to_string()).collect();
    hub.set_order(Projection::Tree, &order);
    // The windows. The panes that a window draws follow the tabs, so every window is set again.
    for window in &old.windows {
        if new.window_index(window.id).is_none() {
            hub.remove(Projection::Windows, &window.id.to_string());
        }
    }
    for window in &new.windows {
        if let Ok(value) = serde_json::to_value(new.window_value(window)) {
            hub.set(Projection::Windows, &window.id.to_string(), value);
        }
    }
    let order: Vec<String> = new.windows.iter().map(|w| w.id.to_string()).collect();
    hub.set_order(Projection::Windows, &order);

    // The panes of the layout. The pane runtime adds its own rows.
    for id in old.panes.keys().filter(|id| !new.panes.contains_key(id)) {
        hub.remove(Projection::Panes, &id.to_string());
    }
    for pane in new.panes.values() {
        if let Ok(value) = serde_json::to_value(new.pane_value(pane)) {
            hub.set(Projection::Panes, &pane.id.to_string(), value);
        }
    }
}
