//! Params and results of `session.*`, `machine.*`, `workspace.*`, `group.*`, `tab.*`, `layout.*`,
//! `window.*` and `render.*`.
//!
//! Most commands return only ids or nothing: the UI sees the effect in the projection
//! patch. The `list` and `show` methods return data for the CLI.

use crate::api::{
    common::{Bounds, ProcessSpec, RendererKind, wire},
    ids::{GroupId, PaneId, Ref, TabId, WindowId, WorkspaceId},
    model::{MachineValue, WorkspaceValue},
    time::Timestamp,
};

wire! {
    // --- session and machine ---

    /// The versions of the wire formats of this core.
    #[derive(Copy)]
    pub struct Protocols {
        /// `nh_proto::version::FRAME_PROTO`.
        pub frame: u16,
        /// `nh_proto::version::HOLDER_PROTO`.
        pub holder: u16,
    }

    /// `session.info`: the core that answers.
    pub struct SessionInfo {
        /// The name of the session (`default`).
        pub name: String,
        /// `neohar/1`.
        pub api_version: String,
        /// The version of the neohar build.
        pub version: String,
        pub pid: u32,
        pub started_at: Timestamp,
        pub session_uuid: String,
        pub machine_uuid: String,
        /// Changes when the event log starts again. A cursor from another epoch is stale.
        pub log_epoch: u64,
        pub protocols: Protocols,
    }

    /// `session.stop_everything`: stops every program of every pane, as "Quit and stop
    /// everything" does. The core keeps running.
    pub struct SessionStopEverything {
        /// Skip the grace period: the agents get no time to end by themselves, and a program that
        /// runs after `Term` and `Hup` gets `Kill` at once.
        #[serde(default)]
        pub force: bool,
    }

    /// What `session.stop_everything` did, as numbers of panes.
    #[derive(Copy, Eq)]
    pub struct SessionStopped {
        /// The panes whose program ended after the polite signals.
        pub ended: u32,
        /// The panes whose program had to be killed.
        pub killed: u32,
        /// The panes whose program still ran at the end. Their holders keep running.
        pub left: u32,
    }

    /// `machine.info`: the machine of the core.
    pub struct MachineInfo {
        /// The label of the machine. The local machine is `local`.
        pub label: String,
        pub uuid: String,
        pub os: String,
        pub arch: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub hostname: Option<String>,
    }

    // --- workspaces and groups ---

    pub struct WorkspaceCreate {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub name: Option<String>,
        /// The root path. It is the cwd of the first pane.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub root: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub group: Option<Ref<GroupId>>,
        /// The label of the machine. Without it the workspace is local.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub machine: Option<String>,
        /// What the first pane runs.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub command: Option<ProcessSpec>,
    }

    /// A workspace starts with one tab that has one pane.
    pub struct WorkspaceCreated {
        pub workspace: WorkspaceId,
        pub handle: String,
        pub tab: TabId,
        pub tab_handle: String,
        pub pane: PaneId,
        pub pane_handle: String,
    }

    pub struct WorkspaceRename {
        pub workspace: Ref<WorkspaceId>,
        pub name: String,
    }

    /// `confirm_required` when a pane has a live process and `force` is false.
    pub struct WorkspaceClose {
        pub workspace: Ref<WorkspaceId>,
        #[serde(default)]
        pub force: bool,
    }

    /// Moves a workspace in the sidebar.
    pub struct WorkspaceMove {
        pub workspace: Ref<WorkspaceId>,
        /// The group to put it in. Without it the workspace has no group.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub group: Option<Ref<GroupId>>,
        /// Put it before this workspace. Without it the workspace goes to the end.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub before: Option<Ref<WorkspaceId>>,
    }

    pub struct WorkspaceList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub group: Option<Ref<GroupId>>,
    }

    pub struct WorkspaceSummary {
        pub id: WorkspaceId,
        pub handle: String,
        pub name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub root: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub group: Option<GroupId>,
        pub tabs: u32,
        pub panes: u32,
        /// The panes of the workspace that have a live agent run.
        pub agents: u32,
    }

    pub struct WorkspaceListResult {
        pub workspaces: Vec<WorkspaceSummary>,
    }

    pub struct WorkspaceShow {
        pub workspace: Ref<WorkspaceId>,
    }

    pub struct WorkspaceShown {
        pub workspace: WorkspaceValue,
    }

    pub struct GroupCreate {
        pub name: String,
    }

    pub struct GroupCreated {
        pub group: GroupId,
    }

    pub struct GroupRename {
        pub group: Ref<GroupId>,
        pub name: String,
    }

    /// Closing a group keeps its workspaces. They have no group afterwards.
    pub struct GroupClose {
        pub group: Ref<GroupId>,
    }

    pub struct GroupMove {
        pub group: Ref<GroupId>,
        /// Put it before this group. Without it the group goes to the end.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub before: Option<Ref<GroupId>>,
    }

    pub struct GroupList {}

    pub struct GroupSummary {
        pub id: GroupId,
        pub name: String,
        pub workspaces: Vec<WorkspaceId>,
    }

    pub struct GroupListResult {
        pub groups: Vec<GroupSummary>,
    }

    pub struct GroupShow {
        pub group: Ref<GroupId>,
    }

    // --- tabs ---

    pub struct TabCreate {
        pub workspace: Ref<WorkspaceId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub command: Option<ProcessSpec>,
    }

    pub struct TabCreated {
        pub tab: TabId,
        pub handle: String,
        pub pane: PaneId,
        pub pane_handle: String,
    }

    pub struct TabRename {
        pub tab: Ref<TabId>,
        pub name: String,
    }

    /// `confirm_required` when a pane has a live process and `force` is false.
    pub struct TabClose {
        pub tab: Ref<TabId>,
        #[serde(default)]
        pub force: bool,
    }

    pub struct TabMove {
        pub tab: Ref<TabId>,
        /// The workspace to move it to. Without it the tab stays in its workspace.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub workspace: Option<Ref<WorkspaceId>>,
        /// The place among the tabs, counted from 0. Without it the tab goes to the end.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub index: Option<u32>,
    }

    /// Shows a tab in a window.
    pub struct TabSelect {
        pub tab: Ref<TabId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub window: Option<WindowId>,
    }

    // --- layout ---

    pub struct LayoutSetRatios {
        pub tab: Ref<TabId>,
        /// The split node, as the indices of the children from the root. `[]` is the root.
        pub path: Vec<u32>,
        pub ratios: Vec<f32>,
    }

    pub struct LayoutEqualize {
        pub tab: Ref<TabId>,
        #[serde(default)]
        pub path: Vec<u32>,
    }

    pub struct LayoutZoom {
        pub tab: Ref<TabId>,
        /// The pane to zoom. Without it the zoom ends.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub pane: Option<Ref<PaneId>>,
    }

    // --- windows ---

    pub struct WindowOpen {
        /// The panes to show. Without them the window shows `tab`.
        #[serde(default)]
        pub panes: Vec<Ref<PaneId>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub tab: Option<Ref<TabId>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub bounds: Option<Bounds>,
    }

    pub struct WindowOpened {
        pub window: WindowId,
    }

    pub struct WindowClose {
        pub window: WindowId,
    }

    pub struct WindowSetVisiblePanes {
        pub window: WindowId,
        pub panes: Vec<Ref<PaneId>>,
    }

    /// Focus and visibility from the OS and the page, for gate mode and pacing.
    pub struct WindowFocusReport {
        pub window: WindowId,
        pub focused: bool,
        pub visible: bool,
        #[serde(default)]
        pub minimized: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub refresh_hz: Option<f32>,
        /// The pane whose surface is the active element of the page. `null` when focus is in
        /// chrome such as the palette.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub active_pane: Option<Ref<PaneId>>,
    }

    /// What the page measured.
    pub struct WindowReportMetrics {
        pub window: WindowId,
        pub cell_width_px: f32,
        pub cell_height_px: f32,
        pub device_pixel_ratio: f32,
        pub renderer: RendererKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub gpu: Option<String>,
    }

    pub struct WindowSetBounds {
        pub window: WindowId,
        pub bounds: Bounds,
    }

    /// The single view of a window. With `single`, the window draws only its
    /// focused pane, or the first pane of its tab when no pane of the tab has the focus.
    pub struct WindowSetSingle {
        pub window: WindowId,
        pub single: bool,
    }

    /// The folder panel of the system, attached to the window.
    pub struct WindowPickFolder {
        pub window: WindowId,
        /// The folder that the panel shows first. The core ignores a path that is not absolute.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub start: Option<String>,
        /// The message of the panel.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub title: Option<String>,
    }

    /// The folder that the person chose, or `null` when the person cancelled the panel.
    pub struct FolderPicked {
        pub path: Option<String>,
    }

    /// `fs.dirs`: the folders in a folder of the machine, for the folder browser of the task
    /// dialog.
    pub struct FsDirs {
        /// An absolute path or a path under `~`. Without it, the home folder of the machine.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub path: Option<String>,
    }

    /// The folders of a folder, in the order in which the browser shows them. The favorites
    /// come first, then the other folders by name, without case. Hidden folders are not there.
    pub struct FsDirsResult {
        /// The folder that the core listed, as an absolute path.
        pub path: String,
        /// The folder above it. The root folder has none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub parent: Option<String>,
        pub entries: Vec<FsDirEntry>,
        /// True when the folder has more folders than the core sends (2,000).
        pub truncated: bool,
    }

    pub struct FsDirEntry {
        /// The name that the browser shows.
        pub name: String,
        /// The absolute path that the browser sends back to open the folder or to choose it.
        pub path: String,
        /// True for the root of a git repository.
        pub repo: bool,
    }

    /// `fs.list`: the entries of a folder of the machine, for the Files tab.
    pub struct FsList {
        /// An absolute path.
        pub path: String,
    }

    /// The entries of a folder in the order of the tree: the folders first, then the other
    /// entries, each group by name without case. `.git` and `.DS_Store` are not there.
    pub struct FsListResult {
        /// The folder that the core listed, as an absolute path.
        pub path: String,
        pub entries: Vec<FsEntry>,
        /// True when the folder has more entries than the core sends (2,000).
        pub truncated: bool,
    }

    pub struct FsEntry {
        pub name: String,
        /// The absolute path, for `fs.list`, `fs.read` and the actions of a row.
        pub path: String,
        pub kind: FsEntryKind,
        /// The name starts with a dot. The tree shows the entry dim.
        pub hidden: bool,
        /// The entry is a symlink. `kind` is the kind of its target.
        pub link: bool,
        /// The name of the icon of the entry in the Material Icon Theme, by its name:
        /// `folder` and `file` when no other icon fits. A folder has the name of its closed icon.
        pub icon: String,
    }

    #[serde(rename_all = "snake_case")]
    #[derive(Copy, Eq, Hash)]
    pub enum FsEntryKind {
        /// A folder, or a symlink to a folder. It opens in the tree.
        Dir,
        /// A regular file, or a symlink to one. It opens in the file view.
        File,
        /// A socket, a pipe, a device or a broken link. The tree shows it and opens nothing.
        Other,
    }

    /// `fs.read`: the preview of a regular file of the machine.
    pub struct FsRead {
        /// An absolute path.
        pub path: String,
    }

    #[serde(rename_all = "snake_case")]
    #[derive(Copy, Eq, Hash)]
    pub enum FsPreviewKind {
        /// Lines of text with their highlight.
        Text,
        /// An image that the page shows from `image`.
        Image,
        /// Bytes that are not text. The view shows only `info`.
        Binary,
    }

    pub struct FsReadResult {
        pub path: String,
        /// The last part of the path.
        pub name: String,
        /// The icon of the file, as in `FsEntry`.
        pub icon: String,
        pub kind: FsPreviewKind,
        /// The text of the head of the file view, for example "120 lines · 4.2 KB".
        pub info: String,
        /// One line of text for each line of the file, with its highlight. Empty for an image
        /// and for a binary file.
        pub lines: Vec<crate::api::rich::Spans>,
        /// The HTML of a Markdown file of at most 256 KiB.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub markdown: Option<crate::api::rich::Markdown>,
        /// A `data:` URL of an image of at most 8 MiB.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub image: Option<String>,
        /// The file has more than the core sends: 1 MiB or 10,000 lines.
        pub truncated: bool,
    }

    /// `fs.open`: the host of the window opens a path, or shows it in the Finder.
    pub struct FsOpen {
        pub window: WindowId,
        /// An absolute path.
        pub path: String,
        /// Show the path in the Finder, and do not open it.
        #[serde(default)]
        pub reveal: bool,
    }

    /// `fs.copy_path`: the core writes a path to the clipboard of the device that called.
    pub struct FsCopyPath {
        /// An absolute path.
        pub path: String,
        /// A folder that holds `path`. The clipboard gets the path from this folder.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub relative_to: Option<String>,
        /// Return the text, and write no clipboard of a host: a phone writes it in the tap.
        #[serde(default)]
        pub return_text: bool,
    }

    /// `machine.connect`, `machine.disconnect` and `machine.open_window`: the label of a machine of
    /// `config.toml`.
    pub struct MachineTarget {
        pub machine: String,
    }

    /// `machine.list`: the rows of the projection `machines`, in its order.
    pub struct MachineList {
        pub machines: Vec<MachineValue>,
    }

    /// `askpass.ask`: a question of `ssh` that the app asks the person. `nh askpass`
    /// calls it with the text that ssh gave. Only the host in the process of the core answers.
    pub struct AskpassAsk {
        /// The question as ssh wrote it: a host key, a passphrase or a password.
        pub prompt: String,
        /// The label of the machine that the link is for, when the core knows it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub machine: Option<String>,
    }

    /// The answer of the person, or `null` when the person cancelled. The core keeps it nowhere:
    /// not in an event, not in a log.
    pub struct AskpassAnswered {
        pub answer: Option<String>,
    }

    // --- render ---

    /// One painted frame. `pane` is the render id that the frames carry.
    pub struct PaneAck {
        pub pane: u32,
        pub rev: u64,
    }

    /// `render.ack`. The UI sends it at most once for each painted animation frame, and only
    /// when something changed.
    pub struct RenderAck {
        pub window: WindowId,
        #[serde(default)]
        pub acks: Vec<PaneAck>,
        /// The panes whose frames the UI discarded. The core sends a keyframe for each.
        #[serde(default)]
        pub need_keyframe: Vec<u32>,
    }
}
