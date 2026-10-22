//! The methods of the API: one declaration for each, and everything else comes
//! from the table.
//!
//! The table makes [`Method`] (the names), [`Call`] (a call with typed params), and the registry
//! that `nh api schema` prints: name, scopes, and the schemas of the params and of the result.
//! A method that the core does not implement yet answers `unsupported`.

use schemars::{Schema, SchemaGenerator};
use serde_json::Value;

use super::{
    calls::{agent::*, internal::*, pane::*, phone::*, system::*, workspace::*},
    common::{Empty, JobStarted},
    envelope::{ErrorCode, RpcError},
    model::PhoneStatusValue,
};

/// Who may call a method.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Scopes {
    /// The UI, over `rpc` of the Tauri host or the WebSocket.
    pub ui: bool,
    /// The CLI and any other client on the API socket.
    pub cli: bool,
    /// A hook shim, with the token of a pane.
    pub pane: bool,
    /// Development and test builds only.
    pub dev: bool,
    /// A paired phone calls the methods of the UI, less the refusals of
    /// [`Method::phone_refusal`]. This flag gives a phone a method that the UI does not have.
    pub phone: bool,
}

impl Scopes {
    pub const UI: Self = Self {
        ui: true,
        cli: false,
        pane: false,
        dev: false,
        phone: false,
    };
    pub const CLI: Self = Self {
        ui: false,
        cli: true,
        pane: false,
        dev: false,
        phone: false,
    };
    pub const BOTH: Self = Self {
        ui: true,
        cli: true,
        pane: false,
        dev: false,
        phone: false,
    };
    pub const PANE: Self = Self {
        ui: false,
        cli: false,
        pane: true,
        dev: false,
        phone: false,
    };
    pub const DEV: Self = Self {
        ui: false,
        cli: false,
        pane: false,
        dev: true,
        phone: false,
    };

    pub const CLI_PHONE: Self = Self {
        phone: true,
        ..Self::CLI
    };
    /// A phone only: the methods that a phone calls for itself.
    pub const PHONE: Self = Self {
        ui: false,
        cli: false,
        pane: false,
        dev: false,
        phone: true,
    };

    /// The names of the flags, for the registry. [`Method::callers`] also names a phone for a
    /// method of the UI.
    pub fn names(self) -> Vec<&'static str> {
        [
            (self.ui, "ui"),
            (self.cli, "cli"),
            (self.pane, "pane"),
            (self.dev, "dev"),
            (self.phone, "phone"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect()
    }
}

macro_rules! methods {
    ($($variant:ident $name:literal $scope:ident ($params:ty) -> $result:ty;)*) => {
        /// The name of a method.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Method {
            $($variant),*
        }

        impl Method {
            pub const ALL: &'static [Method] = &[$(Method::$variant),*];

            /// The name on the wire, for example `pane.split`.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Method::$variant => $name),*
                }
            }

            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(Method::$variant),)*
                    _ => None,
                }
            }

            pub const fn scopes(self) -> Scopes {
                match self {
                    $(Method::$variant => Scopes::$scope),*
                }
            }

            /// The schema of the params.
            pub fn params_schema(self, generator: &mut SchemaGenerator) -> Schema {
                match self {
                    $(Method::$variant => generator.subschema_for::<$params>()),*
                }
            }

            /// The schema of the result.
            pub fn result_schema(self, generator: &mut SchemaGenerator) -> Schema {
                match self {
                    $(Method::$variant => generator.subschema_for::<$result>()),*
                }
            }
        }

        /// A call with its params decoded.
        #[derive(Clone, Debug, PartialEq)]
        // The variants differ in size, and a call lives for one dispatch.
        #[allow(clippy::large_enum_variant)]
        pub enum Call {
            $($variant($params)),*
        }

        impl Call {
            pub const fn method(&self) -> Method {
                match self {
                    $(Call::$variant(_) => Method::$variant),*
                }
            }

            /// Decodes the params of a method. `null` counts as `{}`.
            pub fn decode(method: Method, params: Value) -> Result<Self, RpcError> {
                let params = if params.is_null() {
                    Value::Object(serde_json::Map::new())
                } else {
                    params
                };
                match method {
                    $(Method::$variant => serde_json::from_value::<$params>(params)
                        .map(Call::$variant)
                        .map_err(|e| invalid_params(method, &e))),*
                }
            }
        }
    };
}

impl Method {
    /// Whether the method changes state that the store keeps. While the store is degraded,
    /// the core refuses these with `degraded_storage`. The others go on: reads, input to
    /// panes, rendering, the state of a window, the configuration (a file, not a table), and the
    /// calls of a hook shim that only observe.
    pub const fn needs_persistence(self) -> bool {
        !matches!(
            self,
            Self::SessionInfo
                | Self::SessionStopEverything
                | Self::SessionShutdown
                | Self::DebugDropLinks
                | Self::MachineInfo
                | Self::MachineList
                | Self::MachineConnect
                | Self::MachineDisconnect
                | Self::MachineOpenWindow
                | Self::WorkspaceList
                | Self::WorkspaceShow
                | Self::GroupList
                | Self::GroupShow
                | Self::PaneFocus
                | Self::PaneClaim
                | Self::WindowSetSingle
                | Self::PaneResize
                | Self::PaneInput
                | Self::PanePaste
                | Self::PaneSendBytes
                | Self::PaneDropFiles
                | Self::PaneDropBlobs
                | Self::PaneInsertPaths
                | Self::PaneScroll
                | Self::PaneSelect
                | Self::PaneSelectClear
                | Self::PaneCopy
                | Self::PaneRead
                | Self::PanePeekOpen
                | Self::PanePeekClose
                | Self::PaneClipboardAllow
                | Self::PaneList
                | Self::PaneShow
                | Self::WindowFocusReport
                | Self::WindowReportMetrics
                | Self::WindowPickFolder
                | Self::FsDirs
                | Self::FsList
                | Self::FsRead
                | Self::FsOpen
                | Self::FsCopyPath
                | Self::AskpassAsk
                | Self::RenderAck
                | Self::AgentList
                | Self::AgentShow
                | Self::AgentChoices
                | Self::AgentInterrupt
                | Self::AgentPrompt
                | Self::AgentReply
                | Self::AgentWait
                | Self::InteractionList
                | Self::InteractionShow
                | Self::InteractionNext
                | Self::InteractionReveal
                | Self::TaskList
                | Self::TaskShow
                | Self::TaskChanges
                | Self::TaskDiff
                | Self::ChangesFollow
                | Self::ChangesDiff
                | Self::WorktreeList
                | Self::PhonePairBegin
                | Self::PhonePairCancel
                | Self::PhonePublish
                | Self::PhoneUnpublish
                | Self::PhonePushTest
                | Self::PhoneVisible
                | Self::EventsSubscribe
                | Self::EventsUnsubscribe
                | Self::ProjectionSubscribe
                | Self::ProjectionUnsubscribe
                | Self::ProjectionVersions
                | Self::ConfigGet
                | Self::ConfigSet
                | Self::ConfigReload
                | Self::PolicyList
                | Self::PolicySuggest
                | Self::IntegrationList
                | Self::IntegrationStatus
                | Self::IntegrationCapabilities
                | Self::DoctorRun
                | Self::DebugBundle
                | Self::SearchQuery
                | Self::PaletteQuery
                | Self::RichDiff
                | Self::RichMarkdown
                | Self::RichHighlight
                | Self::TimelinePage
                | Self::TimelineItem
                | Self::JobsCancel
                | Self::AdapterSignal
                | Self::DebugScreen
                | Self::DebugSpawnPanes
        )
    }
}

/// Why a phone may not call a method of the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhoneRefusal {
    /// A phone has no such thing: the method makes or ends a native window, or it acts on the
    /// screen of the Mac. The answer is `unsupported`.
    Unsupported(&'static str),
    /// The pairing and the network stay on the Mac. The answer is `permission_denied`.
    Denied(&'static str),
}

impl Method {
    /// Why a phone may not call this method of the UI, or `None`.
    pub const fn phone_refusal(self) -> Option<PhoneRefusal> {
        match self {
            Self::WindowOpen | Self::WindowClose | Self::PanePopout | Self::MachineOpenWindow => {
                Some(PhoneRefusal::Unsupported(
                    "A phone has one window, and this method makes or ends a native window.",
                ))
            }
            Self::WindowPickFolder => Some(PhoneRefusal::Unsupported(
                "A phone has no folder panel. The page shows its folder browser.",
            )),
            Self::FsOpen => Some(PhoneRefusal::Unsupported(
                "A path opens on the screen of the Mac, not on a phone.",
            )),
            Self::PhonePairBegin | Self::PhonePairCancel => Some(PhoneRefusal::Denied(
                "Pair a phone on the Mac. A phone cannot pair another phone.",
            )),
            Self::PhonePublish | Self::PhoneUnpublish => Some(PhoneRefusal::Denied(
                "Change the mapping of tailscale serve on the Mac.",
            )),
            _ => None,
        }
    }

    /// Whether a paired phone may call the method: a method of the UI that has no
    /// refusal, or a method with the flag `phone`.
    pub const fn phone_may_call(self) -> bool {
        let scopes = self.scopes();
        scopes.phone || (scopes.ui && self.phone_refusal().is_none())
    }

    /// The callers of the method, for the registry: the flags, and `phone` for a method of the UI
    /// that a phone may call.
    pub fn callers(self) -> Vec<&'static str> {
        let mut names = self.scopes().names();
        if self.phone_may_call() && !names.contains(&"phone") {
            names.push("phone");
        }
        names
    }
}

fn invalid_params(method: Method, error: &serde_json::Error) -> RpcError {
    RpcError::plain(
        ErrorCode::InvalidParams,
        format!("{}: {error}", method.name()),
    )
}

impl Call {
    /// Decodes a request by the name of the method. An unknown name is `method_not_found`.
    pub fn parse(name: &str, params: Value) -> Result<Self, RpcError> {
        let method = Method::from_name(name).ok_or_else(|| {
            RpcError::plain(ErrorCode::MethodNotFound, format!("no such method: {name}"))
        })?;
        Self::decode(method, params)
    }
}

methods! {
    // session and machine
    SessionInfo "session.info" BOTH (Empty) -> SessionInfo;
    MachineInfo "machine.info" BOTH (Empty) -> MachineInfo;
    MachineList "machine.list" BOTH (Empty) -> MachineList;
    MachineConnect "machine.connect" BOTH (MachineTarget) -> Empty;
    MachineDisconnect "machine.disconnect" BOTH (MachineTarget) -> Empty;
    MachineOpenWindow "machine.open_window" BOTH (MachineTarget) -> Empty;
    SessionStopEverything "session.stop_everything" BOTH (SessionStopEverything) -> SessionStopped;
    SessionShutdown "session.shutdown" CLI (Empty) -> Empty;

    // workspaces and groups
    WorkspaceCreate "workspace.create" BOTH (WorkspaceCreate) -> WorkspaceCreated;
    WorkspaceRename "workspace.rename" BOTH (WorkspaceRename) -> Empty;
    WorkspaceClose "workspace.close" BOTH (WorkspaceClose) -> Empty;
    WorkspaceMove "workspace.move" BOTH (WorkspaceMove) -> Empty;
    WorkspaceList "workspace.list" BOTH (WorkspaceList) -> WorkspaceListResult;
    WorkspaceShow "workspace.show" BOTH (WorkspaceShow) -> WorkspaceShown;
    GroupCreate "group.create" BOTH (GroupCreate) -> GroupCreated;
    GroupRename "group.rename" BOTH (GroupRename) -> Empty;
    GroupClose "group.close" BOTH (GroupClose) -> Empty;
    GroupMove "group.move" BOTH (GroupMove) -> Empty;
    GroupList "group.list" BOTH (GroupList) -> GroupListResult;
    GroupShow "group.show" BOTH (GroupShow) -> GroupSummary;

    // tabs and layout
    TabCreate "tab.create" BOTH (TabCreate) -> TabCreated;
    TabRename "tab.rename" BOTH (TabRename) -> Empty;
    TabClose "tab.close" BOTH (TabClose) -> Empty;
    TabMove "tab.move" BOTH (TabMove) -> Empty;
    TabSelect "tab.select" BOTH (TabSelect) -> Empty;
    LayoutSetRatios "layout.set_ratios" BOTH (LayoutSetRatios) -> Empty;
    LayoutEqualize "layout.equalize" BOTH (LayoutEqualize) -> Empty;
    LayoutZoom "layout.zoom" BOTH (LayoutZoom) -> Empty;

    // panes
    PaneSplit "pane.split" BOTH (PaneSplit) -> PaneCreated;
    PaneClose "pane.close" BOTH (PaneClose) -> Empty;
    PaneFocus "pane.focus" UI (PaneFocus) -> Empty;
    PaneClaim "pane.claim" UI (PaneClaim) -> Empty;
    PaneMove "pane.move" BOTH (PaneMove) -> Empty;
    PaneFloat "pane.float" BOTH (PaneFloat) -> Empty;
    PaneUnfloat "pane.unfloat" BOTH (PaneUnfloat) -> Empty;
    PaneSetFloatRect "pane.set_float_rect" UI (PaneSetFloatRect) -> Empty;
    PanePopout "pane.popout" BOTH (PanePopout) -> WindowOpened;
    PaneRestart "pane.restart" BOTH (PaneRestart) -> Empty;
    PaneResize "pane.resize" UI (PaneResize) -> Empty;
    PaneInput "pane.input" UI (PaneInput) -> Empty;
    PanePaste "pane.paste" BOTH (PanePaste) -> Empty;
    PaneSendBytes "pane.send_bytes" BOTH (PaneSendBytes) -> Empty;
    PaneDropFiles "pane.drop_files" UI (PaneDropFiles) -> Empty;
    PaneDropBlobs "pane.drop_blobs" UI (PaneDropBlobs) -> Empty;
    PaneInsertPaths "pane.insert_paths" UI (PaneInsertPaths) -> Empty;
    PaneScroll "pane.scroll" BOTH (PaneScroll) -> Empty;
    PaneSelect "pane.select" UI (PaneSelect) -> Empty;
    PaneSelectClear "pane.select_clear" UI (PaneSelectClear) -> Empty;
    PaneCopy "pane.copy" BOTH (PaneCopy) -> CopyResult;
    PaneRead "pane.read" CLI_PHONE (PaneRead) -> PaneText;
    PanePeekOpen "pane.peek_open" UI (PanePeekOpen) -> Empty;
    PanePeekClose "pane.peek_close" UI (PanePeekClose) -> Empty;
    PaneClipboardAllow "pane.clipboard_allow" UI (PaneClipboardAllow) -> CopyResult;
    PaneList "pane.list" BOTH (PaneList) -> PaneListResult;
    PaneShow "pane.show" BOTH (PaneShow) -> PaneShown;

    // windows and rendering
    WindowOpen "window.open" UI (WindowOpen) -> WindowOpened;
    WindowClose "window.close" UI (WindowClose) -> Empty;
    WindowSetVisiblePanes "window.set_visible_panes" UI (WindowSetVisiblePanes) -> Empty;
    WindowFocusReport "window.focus_report" UI (WindowFocusReport) -> Empty;
    WindowReportMetrics "window.report_metrics" UI (WindowReportMetrics) -> Empty;
    WindowSetBounds "window.set_bounds" UI (WindowSetBounds) -> Empty;
    WindowSetSingle "window.set_single" UI (WindowSetSingle) -> Empty;
    WindowPickFolder "window.pick_folder" UI (WindowPickFolder) -> FolderPicked;
    FsDirs "fs.dirs" BOTH (FsDirs) -> FsDirsResult;
    FsList "fs.list" BOTH (FsList) -> FsListResult;
    FsRead "fs.read" UI (FsRead) -> FsReadResult;
    FsOpen "fs.open" UI (FsOpen) -> Empty;
    FsCopyPath "fs.copy_path" UI (FsCopyPath) -> CopyResult;
    AskpassAsk "askpass.ask" CLI (AskpassAsk) -> AskpassAnswered;
    RenderAck "render.ack" UI (RenderAck) -> Empty;

    // agents
    AgentList "agent.list" BOTH (AgentList) -> AgentListResult;
    AgentShow "agent.show" BOTH (AgentShow) -> AgentShown;
    AgentStart "agent.start" BOTH (AgentStart) -> JobStarted;
    AgentChoices "agent.choices" BOTH (AgentChoices) -> AgentChoicesResult;
    AgentStop "agent.stop" BOTH (AgentStop) -> Empty;
    AgentInterrupt "agent.interrupt" BOTH (AgentInterrupt) -> Empty;
    AgentPrompt "agent.prompt" BOTH (AgentPrompt) -> Empty;
    AgentReply "agent.reply" BOTH (AgentReply) -> Empty;
    AgentResume "agent.resume" BOTH (AgentResume) -> JobStarted;
    AgentWait "agent.wait" CLI (AgentWait) -> AgentWaitResult;
    AgentLink "agent.link" BOTH (AgentLink) -> AgentLinked;
    AgentUnlink "agent.unlink" BOTH (AgentUnlink) -> Empty;
    AgentMove "agent.move" BOTH (AgentMove) -> Empty;
    HarnessRefresh "harness.refresh" BOTH (HarnessRefresh) -> Empty;

    // interactions
    InteractionList "interaction.list" BOTH (InteractionList) -> InteractionListResult;
    InteractionShow "interaction.show" BOTH (InteractionShow) -> InteractionShown;
    InteractionAnswer "interaction.answer" BOTH (InteractionAnswer) -> InteractionAnswered;
    InteractionDismiss "interaction.dismiss" BOTH (InteractionDismiss) -> Empty;
    InteractionNext "interaction.next" BOTH (InteractionNext) -> InteractionNextResult;
    InteractionReveal "interaction.reveal" BOTH (InteractionReveal) -> InteractionRevealed;

    // tasks, favorite folders and worktrees
    TaskCreate "task.create" BOTH (TaskCreate) -> TaskCreated;
    TaskUpdate "task.update" BOTH (TaskUpdate) -> Empty;
    TaskStart "task.start" BOTH (TaskStart) -> JobStarted;
    TaskDelete "task.delete" BOTH (TaskDelete) -> Empty;
    TaskList "task.list" BOTH (TaskList) -> TaskListResult;
    TaskShow "task.show" BOTH (TaskShow) -> TaskShown;
    TaskChanges "task.changes" BOTH (TaskChanges) -> TaskChangesResult;
    TaskDiff "task.diff" BOTH (TaskDiff) -> crate::api::rich::FileDiff;
    ChangesFollow "changes.follow" UI (ChangesFollow) -> Empty;
    ChangesDiff "changes.diff" UI (ChangesDiff) -> crate::api::rich::FileDiff;
    TaskClose "task.close" BOTH (TaskClose) -> Empty;
    FavoriteList "favorite.list" BOTH (FavoriteList) -> FavoriteListResult;
    FavoriteAdd "favorite.add" BOTH (FavoriteAdd) -> FavoriteAdded;
    FavoriteRemove "favorite.remove" BOTH (FavoriteRemove) -> Empty;
    WorktreeList "worktree.list" BOTH (WorktreeList) -> WorktreeListResult;

    // phones
    PhonePairBegin "phone.pair_begin" BOTH (PhonePairBegin) -> PhonePairStarted;
    PhonePairCancel "phone.pair_cancel" BOTH (Empty) -> Empty;
    PhoneRemove "phone.remove" BOTH (PhoneRemove) -> Empty;
    PhoneRename "phone.rename" BOTH (PhoneRename) -> Empty;
    PhonePublish "phone.publish" BOTH (Empty) -> PhoneStatusValue;
    PhoneUnpublish "phone.unpublish" BOTH (Empty) -> PhoneStatusValue;
    PhonePushSubscribe "phone.push_subscribe" PHONE (PhonePushSubscribe) -> Empty;
    PhonePushUnsubscribe "phone.push_unsubscribe" PHONE (Empty) -> Empty;
    PhonePushTest "phone.push_test" BOTH (PhonePushTest) -> Empty;
    PhoneVisible "phone.visible" PHONE (PhoneVisible) -> Empty;
    PhoneWindow "phone.window" PHONE (Empty) -> PhoneWindowResult;

    // events and projections
    EventsSubscribe "events.subscribe" CLI (EventsSubscribe) -> EventsSubscribed;
    EventsUnsubscribe "events.unsubscribe" CLI (EventsUnsubscribe) -> Empty;
    ProjectionSubscribe "projection.subscribe" BOTH (ProjectionSubscribe) -> ProjectionSubscribed;
    ProjectionUnsubscribe "projection.unsubscribe" BOTH (ProjectionUnsubscribe) -> Empty;
    ProjectionVersions "projection.versions" BOTH (ProjectionVersions) -> ProjectionVersionsResult;

    // config, policy and integrations
    ConfigGet "config.get" BOTH (ConfigGet) -> ConfigValues;
    ConfigSet "config.set" BOTH (ConfigSet) -> Empty;
    ConfigReload "config.reload" BOTH (ConfigReload) -> ConfigReloaded;
    PolicyList "policy.list" BOTH (PolicyList) -> PolicyListResult;
    PolicyAdd "policy.add" BOTH (PolicyAdd) -> PolicyAdded;
    PolicyRemove "policy.remove" BOTH (PolicyRemove) -> Empty;
    PolicySuggest "policy.suggest" BOTH (PolicySuggest) -> PolicySuggestResult;
    IntegrationList "integration.list" BOTH (IntegrationList) -> IntegrationListResult;
    IntegrationInstall "integration.install" BOTH (IntegrationInstall) -> IntegrationChanges;
    IntegrationStatus "integration.status" BOTH (IntegrationStatusParams) -> IntegrationStatusResult;
    IntegrationUninstall "integration.uninstall" BOTH (IntegrationUninstall) -> IntegrationChanges;
    IntegrationDoctor "integration.doctor" BOTH (IntegrationDoctor) -> JobStarted;
    IntegrationCapabilities "integration.capabilities" BOTH (IntegrationCapabilitiesParams) -> IntegrationCapabilitiesResult;
    DoctorRun "doctor.run" BOTH (DoctorRun) -> DoctorReport;
    DebugBundle "debug.bundle" CLI (DebugBundle) -> DebugBundled;

    // search, palette, rich content, timeline and jobs
    SearchQuery "search.query" BOTH (SearchQuery) -> SearchResults;
    PaletteQuery "palette.query" UI (PaletteQuery) -> PaletteResults;
    PaletteExecute "palette.execute" UI (PaletteExecute) -> PaletteExecuted;
    RichDiff "rich.diff" UI (RichDiff) -> crate::api::rich::Diff;
    RichMarkdown "rich.markdown" UI (RichMarkdown) -> crate::api::rich::Markdown;
    RichHighlight "rich.highlight" UI (RichHighlight) -> crate::api::rich::Spans;
    TimelinePage "timeline.page" UI (TimelinePageParams) -> TimelinePageResult;
    TimelineItem "timeline.item" UI (TimelineItemGet) -> TimelineItemText;
    JobsCancel "jobs.cancel" BOTH (JobsCancel) -> Empty;

    // hook shims
    AdapterSignal "adapter.signal" PANE (AdapterSignal) -> AdapterReceipt;
    AdapterGate "adapter.gate" PANE (AdapterGate) -> GateReply;
    AdapterGateResume "adapter.gate_resume" PANE (AdapterGateResume) -> GateReply;
    AdapterDeliveryAck "adapter.delivery_ack" PANE (AdapterDeliveryAck) -> Empty;

    // development and tests
    DebugScreen "debug.screen" DEV (DebugScreen) -> DebugScreenResult;
    DebugAnswer "debug.answer" DEV (DebugAnswer) -> DebugAnswered;
    DebugSpawnPanes "debug.spawn_panes" DEV (DebugSpawnPanes) -> DebugSpawned;
    DebugDropLinks "debug.drop_links" DEV (Empty) -> Empty;
}

/// What the server sends without a request: the name and the schema of the params.
pub struct NotificationSpec {
    pub name: &'static str,
    pub params: fn(&mut SchemaGenerator) -> Schema,
}

/// The notifications of the server.
pub fn notifications() -> Vec<NotificationSpec> {
    vec![
        NotificationSpec {
            name: "events.event",
            params: |generator| generator.subschema_for::<EventNotification>(),
        },
        NotificationSpec {
            name: "events.truncated",
            params: |generator| generator.subschema_for::<EventsTruncated>(),
        },
        NotificationSpec {
            name: "projection.message",
            params: |generator| generator.subschema_for::<ProjectionNotification>(),
        },
        NotificationSpec {
            name: "adapter.gate_result",
            params: |generator| generator.subschema_for::<GateResult>(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn every_name_is_unique_and_namespaced() {
        let mut seen = std::collections::BTreeSet::new();
        for method in Method::ALL {
            let name = method.name();
            assert!(seen.insert(name), "{name} twice");
            assert!(name.contains('.'), "{name}");
            assert_eq!(Method::from_name(name), Some(*method));
        }
        assert!(Method::ALL.len() > 100, "{} methods", Method::ALL.len());
    }

    #[test]
    fn the_method_set_is_complete() {
        // A namespace and the names it must have.
        let expected: &[(&str, &[&str])] = &[
            ("session", &["info", "stop_everything", "shutdown"]),
            (
                "machine",
                &["info", "list", "connect", "disconnect", "open_window"],
            ),
            (
                "workspace",
                &["create", "rename", "close", "move", "list", "show"],
            ),
            (
                "group",
                &["create", "rename", "close", "move", "list", "show"],
            ),
            ("tab", &["create", "rename", "close", "move", "select"]),
            (
                "pane",
                &[
                    "split",
                    "close",
                    "focus",
                    "claim",
                    "move",
                    "float",
                    "unfloat",
                    "set_float_rect",
                    "popout",
                    "restart",
                    "resize",
                    "input",
                    "paste",
                    "send_bytes",
                    "drop_files",
                    "drop_blobs",
                    "insert_paths",
                    "scroll",
                    "select",
                    "select_clear",
                    "copy",
                    "read",
                    "peek_open",
                    "peek_close",
                    "clipboard_allow",
                    "list",
                    "show",
                ],
            ),
            ("layout", &["set_ratios", "equalize", "zoom"]),
            (
                "window",
                &[
                    "open",
                    "close",
                    "set_visible_panes",
                    "focus_report",
                    "report_metrics",
                    "set_bounds",
                    "set_single",
                    "pick_folder",
                ],
            ),
            ("fs", &["dirs", "list", "read", "open", "copy_path"]),
            ("askpass", &["ask"]),
            ("render", &["ack"]),
            (
                "agent",
                &[
                    "list",
                    "show",
                    "start",
                    "choices",
                    "stop",
                    "interrupt",
                    "prompt",
                    "reply",
                    "resume",
                    "wait",
                    "link",
                    "unlink",
                    "move",
                ],
            ),
            ("harness", &["refresh"]),
            (
                "interaction",
                &["list", "show", "answer", "dismiss", "next", "reveal"],
            ),
            (
                "task",
                &[
                    "create", "update", "start", "delete", "list", "show", "changes", "diff",
                    "close",
                ],
            ),
            ("changes", &["follow", "diff"]),
            ("favorite", &["list", "add", "remove"]),
            ("worktree", &["list"]),
            (
                "phone",
                &[
                    "pair_begin",
                    "pair_cancel",
                    "remove",
                    "rename",
                    "publish",
                    "unpublish",
                    "push_subscribe",
                    "push_unsubscribe",
                    "push_test",
                    "visible",
                    "window",
                ],
            ),
            ("events", &["subscribe", "unsubscribe"]),
            ("projection", &["subscribe", "unsubscribe", "versions"]),
            ("config", &["get", "set", "reload"]),
            ("policy", &["list", "add", "remove", "suggest"]),
            (
                "integration",
                &[
                    "list",
                    "install",
                    "status",
                    "uninstall",
                    "doctor",
                    "capabilities",
                ],
            ),
            ("search", &["query"]),
            ("palette", &["query", "execute"]),
            ("rich", &["diff", "markdown", "highlight"]),
            ("timeline", &["page", "item"]),
            ("jobs", &["cancel"]),
            (
                "adapter",
                &["signal", "gate", "gate_resume", "delivery_ack"],
            ),
            (
                "debug",
                &["screen", "answer", "spawn_panes", "bundle", "drop_links"],
            ),
            ("doctor", &["run"]),
        ];
        for (namespace, names) in expected {
            for name in *names {
                let full = format!("{namespace}.{name}");
                assert!(Method::from_name(&full).is_some(), "{full} is missing");
            }
        }
        let listed: usize = expected.iter().map(|(_, names)| names.len()).sum();
        assert_eq!(
            Method::ALL.len(),
            listed,
            "a method that this list does not have"
        );
    }

    #[test]
    fn the_methods_that_need_the_store_are_the_ones_that_change_state() {
        for name in [
            "workspace.create",
            "tab.close",
            "pane.split",
            "layout.zoom",
            "interaction.answer",
            "task.create",
            "policy.add",
            "adapter.gate",
        ] {
            let method = Method::from_name(name).expect("a method");
            assert!(method.needs_persistence(), "{name}");
        }
        for name in [
            "session.info",
            "pane.input",
            "render.ack",
            "pane.list",
            "agent.show",
            "projection.subscribe",
            "search.query",
            "config.set",
            "window.focus_report",
            "adapter.signal",
        ] {
            let method = Method::from_name(name).expect("a method");
            assert!(!method.needs_persistence(), "{name}");
        }
    }

    #[test]
    fn the_scopes_follow_the_spec() {
        assert_eq!(Method::AdapterSignal.scopes(), Scopes::PANE);
        assert_eq!(Method::DebugScreen.scopes(), Scopes::DEV);
        assert!(Method::PaneInput.scopes().ui && !Method::PaneInput.scopes().cli);
        for method in Method::ALL {
            let scopes = method.scopes();
            if method.name().starts_with("adapter.") {
                assert!(
                    scopes.pane && !scopes.ui && !scopes.cli,
                    "{}",
                    method.name()
                );
            } else {
                assert!(!scopes.pane, "only the adapter methods are for panes");
            }
            // `debug.bundle` is for people (`nh debug bundle`). The other debug methods are
            // for development builds.
            if method.name().starts_with("debug.") && *method != Method::DebugBundle {
                assert!(scopes.dev, "{}", method.name());
            }
        }
        // A phone has the methods of the UI, less the refusals, and the methods
        // that only a phone calls.
        let only_phone: Vec<&str> = Method::ALL
            .iter()
            .filter(|method| method.scopes().phone && !method.scopes().ui)
            .map(|method| method.name())
            .collect();
        assert_eq!(
            only_phone,
            [
                "pane.read",
                "phone.push_subscribe",
                "phone.push_unsubscribe",
                "phone.visible",
                "phone.window",
            ]
        );
        let refused: Vec<&str> = Method::ALL
            .iter()
            .filter(|method| method.scopes().ui && !method.phone_may_call())
            .map(|method| method.name())
            .collect();
        assert_eq!(
            refused,
            [
                "machine.open_window",
                "pane.popout",
                "window.open",
                "window.close",
                "window.pick_folder",
                "fs.open",
                "phone.pair_begin",
                "phone.pair_cancel",
                "phone.publish",
                "phone.unpublish",
            ]
        );
        for method in [
            "pane.input",
            "config.set",
            "task.create",
            "agent.reply",
            "pane.claim",
        ] {
            let method = Method::from_name(method).expect("a method");
            assert!(method.phone_may_call(), "{}", method.name());
        }
        // A refusal is only for a method of the UI.
        for method in Method::ALL {
            if method.phone_refusal().is_some() {
                assert!(method.scopes().ui, "{}", method.name());
            }
        }
    }

    #[test]
    fn a_call_decodes_its_params() {
        let call = Call::parse(
            "pane.split",
            json!({"pane": "w3:p5", "direction": "right", "ratio": 0.4}),
        )
        .expect("decodes");
        assert_eq!(call.method(), Method::PaneSplit);
        let Call::PaneSplit(split) = call else {
            panic!("a split")
        };
        assert_eq!(split.ratio, Some(0.4));
    }

    #[test]
    fn null_params_count_as_an_empty_object() {
        assert!(Call::parse("session.info", Value::Null).is_ok());
        assert!(Call::parse("group.list", Value::Null).is_ok());
        let err = Call::parse("workspace.rename", Value::Null).expect_err("fields are missing");
        assert_eq!(err.call_error().code, ErrorCode::InvalidParams);
    }

    #[test]
    fn an_unknown_method_and_a_bad_param_are_distinct_errors() {
        let unknown = Call::parse("pane.dance", json!({})).expect_err("unknown");
        assert_eq!(unknown.call_error().code, ErrorCode::MethodNotFound);
        let bad = Call::parse("pane.close", json!({"pane": "w3"})).expect_err("a workspace handle");
        assert_eq!(bad.call_error().code, ErrorCode::InvalidParams);
        assert!(bad.message.contains("pane.close"));
    }

    #[test]
    fn pane_input_takes_the_three_logical_events() {
        for event in [
            json!({"type": "key", "code": "KeyC", "key": "c", "mods": ["ctrl"]}),
            json!({"type": "mouse", "kind": "down", "button": "left", "col": 3, "row": 4}),
            json!({"type": "text", "text": "é"}),
        ] {
            let params = json!({"pane": "w3:p5", "input_id": 1, "event": event});
            assert!(Call::parse("pane.input", params).is_ok());
        }
    }

    #[test]
    fn pane_scroll_takes_a_line_or_a_delta() {
        assert!(Call::parse("pane.scroll", json!({"pane": "w3:p5", "top_line": 10})).is_ok());
        assert!(Call::parse("pane.scroll", json!({"pane": "w3:p5", "delta": -3})).is_ok());
        assert!(Call::parse("pane.scroll", json!({"pane": "w3:p5"})).is_err());
    }

    #[test]
    fn an_answer_names_its_kind() {
        let params = json!({
            "interaction": "i42",
            "decision": {"kind": "approval", "behavior": "allow", "always": true},
            "via": "card",
        });
        let Call::InteractionAnswer(answer) =
            Call::parse("interaction.answer", params).expect("decodes")
        else {
            panic!("an answer")
        };
        assert!(matches!(answer.decision, Decision::Approval(ref a) if a.always));
    }
}
