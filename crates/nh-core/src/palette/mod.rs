//! The commands of the palette.
//!
//! The palette is a list of commands with a title, a category, a shortcut and the state of the
//! moment: enabled, or disabled with a reason. Search is a command, not a filter of the client:
//! the core ranks the titles for a query ([`fuzzy`]) and the page shows the order that it gets.
//! The commands that name an object are made from the layout: one to go to each workspace, tab
//! and pane.

pub mod fuzzy;

use nh_proto::api::{calls::system::PaletteMatch, model::PaletteCommand};

use crate::layout::model::Layout;
pub use fuzzy::match_title;

/// What a command needs before it can run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Needs {
    /// Nothing but a core.
    Nothing,
    /// A tab: the one that the window shows.
    Tab,
    /// A pane: the one in focus in the window.
    Pane,
    /// The page does it (`palette.execute` names the action and does nothing else).
    Page,
    /// The page does it on the tab that the window shows, and the page then calls a method that
    /// writes. It needs a tab and a store that works.
    PageTab,
}

/// A command that the core has always.
pub struct Spec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub shortcut: Option<&'static str>,
    pub needs: Needs,
}

const fn spec(
    id: &'static str,
    title: &'static str,
    category: &'static str,
    shortcut: Option<&'static str>,
    needs: Needs,
) -> Spec {
    Spec {
        id,
        title,
        category,
        shortcut,
        needs,
    }
}

/// The commands that exist at all times, in the order of the palette.
pub const COMMANDS: &[Spec] = &[
    spec("tab.new", "New tab", "Tab", None, Needs::Tab),
    spec("tab.close", "Close tab", "Tab", None, Needs::Tab),
    spec("tab.rename", "Rename tab", "Tab", None, Needs::PageTab),
    spec("tab.next", "Next tab", "Tab", Some("⌘⇧]"), Needs::Tab),
    spec(
        "tab.previous",
        "Previous tab",
        "Tab",
        Some("⌘⇧["),
        Needs::Tab,
    ),
    spec(
        "pane.split_right",
        "Split pane right",
        "Pane",
        Some("⌘D"),
        Needs::Pane,
    ),
    spec(
        "pane.split_down",
        "Split pane down",
        "Pane",
        Some("⌘⇧D"),
        Needs::Pane,
    ),
    spec("pane.close", "Close pane", "Pane", None, Needs::Pane),
    spec(
        "agent.resume_all",
        "Resume the agents that ended with the restart",
        "Agent",
        None,
        Needs::Nothing,
    ),
    spec(
        "agent.start_claude",
        "Start Claude Code beside this pane",
        "Agent",
        None,
        Needs::Pane,
    ),
    spec(
        "agent.start_codex",
        "Start Codex beside this pane",
        "Agent",
        None,
        Needs::Pane,
    ),
    spec(
        "agent.resume",
        "Resume the agent that ran in this pane",
        "Agent",
        None,
        Needs::Pane,
    ),
    spec(
        "agent.interrupt",
        "Interrupt the agent of this pane",
        "Agent",
        None,
        Needs::Pane,
    ),
    spec(
        "agent.stop",
        "Stop the agent of this pane",
        "Agent",
        None,
        Needs::Pane,
    ),
    spec(
        "agent.link",
        "Link this pane to its agent",
        "Agent",
        None,
        Needs::Pane,
    ),
    spec(
        "pane.zoom",
        "Zoom pane or show all panes",
        "Pane",
        None,
        Needs::Pane,
    ),
    spec(
        "pane.float",
        "Float pane or dock it",
        "Pane",
        None,
        Needs::Pane,
    ),
    spec(
        "workspace.new",
        "New workspace",
        "Workspace",
        None,
        Needs::Nothing,
    ),
    spec(
        "workspace.rename",
        "Rename workspace",
        "Workspace",
        None,
        Needs::PageTab,
    ),
    spec(
        "workspace.close",
        "Close workspace",
        "Workspace",
        None,
        Needs::Tab,
    ),
    spec(
        "window.new",
        "New window",
        "Window",
        Some("⌘⇧N"),
        Needs::Nothing,
    ),
    spec(
        "search.open",
        "Search scrollback",
        "Search",
        Some("⌘⇧F"),
        Needs::Page,
    ),
    spec(
        "inbox.open",
        "Open the inbox",
        "Go to",
        Some("⌘⇧I"),
        Needs::Page,
    ),
    spec(
        "settings.open",
        "Open settings",
        "App",
        Some("⌘,"),
        Needs::Nothing,
    ),
    spec("phone.pair", "Pair a phone", "App", None, Needs::Nothing),
];

/// The prefix of the commands that go to a workspace, a tab and a pane. The rest of the id is the
/// ULID of the object.
pub const GO_WORKSPACE: &str = "go.workspace:";
pub const GO_TAB: &str = "go.tab:";
pub const GO_PANE: &str = "go.pane:";

/// The command with this id, when it is one of the fixed commands.
pub fn find(id: &str) -> Option<&'static Spec> {
    COMMANDS.iter().find(|spec| spec.id == id)
}

/// Whether a command changes what the store keeps. While the store is degraded the core refuses
/// these (`degraded_storage`), so the palette shows them disabled with the reason.
fn writes(spec: &Spec) -> bool {
    !matches!(spec.needs, Needs::Page) && !matches!(spec.id, "settings.open" | "phone.pair")
}

/// The commands of the palette for a layout, with their state. First the fixed commands, then the
/// ones that go to a workspace, a tab or a pane, in the order of the layout. `degraded` is the
/// reason that the store is degraded (`health`), or `None` when it works.
pub fn commands(layout: &Layout, degraded: Option<&str>) -> Vec<PaletteCommand> {
    let tabs = layout.workspaces.iter().any(|w| !w.tabs.is_empty());
    let panes = !layout.panes.is_empty();
    let stuck =
        degraded.map(|reason| format!("Storage is degraded: {reason}. Changes are not saved."));
    let mut out: Vec<PaletteCommand> = COMMANDS
        .iter()
        .map(|spec| {
            let reason = match spec.needs {
                _ if writes(spec) && stuck.is_some() => stuck.clone(),
                Needs::Tab | Needs::PageTab if !tabs => Some("No tab is open.".to_owned()),
                Needs::Pane if !panes => Some("No pane is open.".to_owned()),
                _ => None,
            };
            PaletteCommand {
                id: spec.id.to_owned(),
                title: spec.title.to_owned(),
                category: spec.category.to_owned(),
                shortcut: spec.shortcut.map(str::to_owned),
                enabled: reason.is_none(),
                disabled_reason: reason,
            }
        })
        .collect();
    let go = |id: String, title: String| PaletteCommand {
        id,
        title,
        category: "Go to".to_owned(),
        shortcut: None,
        enabled: stuck.is_none(),
        disabled_reason: stuck.clone(),
    };
    for workspace in &layout.workspaces {
        out.push(go(
            format!("{GO_WORKSPACE}{}", workspace.id),
            format!("Go to workspace {}", workspace.name),
        ));
    }
    for workspace in &layout.workspaces {
        for tab in &workspace.tabs {
            out.push(go(
                format!("{GO_TAB}{}", tab.id),
                format!("Go to tab {} / {}", workspace.name, tab.name),
            ));
        }
    }
    for entry in layout.panes.values() {
        let value = layout.pane_value(entry);
        let title = if value.title.is_empty() {
            format!("Go to pane {}", value.handle)
        } else {
            format!("Go to pane {} {}", value.handle, value.title)
        };
        out.push(go(format!("{GO_PANE}{}", entry.id), title));
    }
    out
}

/// The commands that match `query`, best first. Commands with the same score keep the order of
/// the palette. An empty query gives all commands, and `limit` cuts the list.
pub fn query(
    layout: &Layout,
    degraded: Option<&str>,
    query: &str,
    limit: usize,
) -> Vec<PaletteMatch> {
    let mut found: Vec<(usize, PaletteMatch)> = commands(layout, degraded)
        .into_iter()
        .enumerate()
        .filter_map(|(index, command)| {
            let m = match_title(query, &command.title)?;
            Some((
                index,
                PaletteMatch {
                    command,
                    score: m.score,
                    ranges: m.ranges,
                },
            ))
        })
        .collect();
    // The best score first. A stable sort keeps the order of the palette among equal scores.
    found.sort_by(|a, b| b.1.score.total_cmp(&a.1.score).then_with(|| a.0.cmp(&b.0)));
    found.into_iter().take(limit).map(|(_, m)| m).collect()
}
