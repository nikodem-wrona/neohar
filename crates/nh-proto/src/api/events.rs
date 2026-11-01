//! Events.
//!
//! Every state change appends events to the log in the same transaction (the transactional
//! outbox). A client reads them with `events.subscribe {cursor, filter}`. An event has a
//! kind (`<noun>.<verb-past>` for layout changes), a version `v` of its payload, and a subject.

use schemars::{Schema, SchemaGenerator};
use serde_json::Value;

use super::{
    calls::system::PolicyScope,
    common::{Harness, Risk, Source, Via, wire},
    ids::{
        ChatId, DeviceId, ExecutionId, GroupId, InteractionId, PaneId, RunId, TabId, WindowId,
        WorkspaceId,
    },
    model::{AttentionReason, ExecutionState, InteractionKind, NodeState},
    time::Timestamp,
};

wire! {
    /// A place in the event log. `log_epoch` changes when the log starts again, and a cursor of
    /// another epoch is stale.
    pub struct Cursor {
        pub machine_uuid: String,
        pub session_uuid: String,
        pub log_epoch: u64,
        pub seq: u64,
    }

    /// An entry of the log.
    pub struct Event {
        /// The place in the log. It grows by one for each event.
        pub seq: u64,
        pub at: Timestamp,
        /// The kind, for example `agent.state_changed`.
        pub kind: String,
        /// The version of the payload. A payload only gets new fields within a version.
        pub v: u16,
        /// The id of the main object, as text.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub subject: Option<String>,
        pub payload: Value,
    }

    /// Which events a subscription wants.
    #[derive(Default)]
    pub struct EventFilter {
        /// Kinds. A trailing `*` is a prefix: `interaction.*`. Without it all kinds match.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub kinds: Vec<String>,
        /// Only events about this object.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub subject: Option<String>,
    }

    // --- payloads: agents ---

    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum Facet {
        Liveness,
        Execution,
        Pending,
        AdapterHealth,
    }

    /// A run was seen. It is not identified yet.
    pub struct AgentDetected {
        pub run: RunId,
        pub harness: Harness,
        /// How the core found it: `launch`, `hook`, `process`, `screen`.
        pub via: String,
    }

    /// A run got its session id and its pane.
    pub struct AgentIdentified {
        pub run: RunId,
        pub harness: Harness,
        pub session_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub pane: Option<PaneId>,
    }

    /// Every change of a facet.
    pub struct AgentStateChanged {
        pub run: RunId,
        pub facet: Facet,
        pub from: Value,
        pub to: Value,
        pub source: Source,
        pub confidence: f32,
    }

    pub struct AgentHarnessVersionUnvalidated {
        pub harness: Harness,
        pub version: String,
    }

    pub struct AgentDriftDetected {
        pub harness: Harness,
        pub version: String,
        pub observations: u32,
        pub bad_signal_rate: f32,
    }

    /// A lower source disagreed with a structured one.
    pub struct AdapterDisagreement {
        pub run: RunId,
        pub facet: Facet,
        pub structured: Value,
        pub other: Value,
    }

    pub struct HarnessManifestLoaded {
        pub id: String,
        pub version: String,
        pub source: String,
        pub serial: u64,
    }

    // --- payloads: interactions ---

    pub struct InteractionOpened {
        pub interaction: InteractionId,
        pub kind: InteractionKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub run: Option<RunId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub pane: Option<PaneId>,
        pub risk: Risk,
        /// Policy decided it at once.
        #[serde(default)]
        pub by_policy: bool,
    }

    pub struct InteractionDecided {
        pub interaction: InteractionId,
        pub rev: u32,
        pub by: String,
        pub via: Via,
    }

    /// The payload of `interaction.delivery_started`, `interaction.delivered` and
    /// `interaction.delivery_unknown`.
    pub struct InteractionDelivery {
        pub interaction: InteractionId,
        pub rev: u32,
    }

    pub struct InteractionDeliveryFailed {
        pub interaction: InteractionId,
        pub rev: u32,
        pub reason: String,
    }

    /// The payload of `interaction.resolved_elsewhere` and `interaction.dismissed`.
    pub struct InteractionEnded {
        pub interaction: InteractionId,
    }

    pub struct InteractionCancelled {
        pub interaction: InteractionId,
        /// For example `process_exited`.
        pub reason: String,
    }

    // --- payloads: panes ---

    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum UnconfirmedReason {
        HolderLost,
        ServerRestarted,
        NotWritten,
    }

    /// An input was in flight when the holder or the core went away.
    pub struct PaneInputUnconfirmed {
        pub pane: PaneId,
        pub input_id: u64,
        pub reason: UnconfirmedReason,
    }

    pub struct PaneExited {
        pub pane: PaneId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub code: Option<i32>,
    }

    /// The program in the pane rang the bell. The core sends at most one event in 2 seconds for
    /// a pane.
    pub struct PaneBell {
        pub pane: PaneId,
    }

    // --- payloads: layout ---

    /// The payload of the `workspace.*` events.
    pub struct WorkspaceEvent {
        pub workspace: WorkspaceId,
        pub handle: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub name: Option<String>,
    }

    /// The payload of the `group.*` events.
    pub struct GroupEvent {
        pub group: GroupId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub name: Option<String>,
    }

    /// The payload of the `tab.*` events.
    pub struct TabEvent {
        pub tab: TabId,
        pub handle: String,
        pub workspace: WorkspaceId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub name: Option<String>,
    }

    /// The payload of the `pane.*` events that name a pane and its place.
    pub struct PaneEvent {
        pub pane: PaneId,
        pub handle: String,
        pub tab: TabId,
        pub workspace: WorkspaceId,
    }

    /// The layout of a tab changed (a split, a ratio, a float or a zoom).
    pub struct LayoutChanged {
        pub tab: TabId,
    }

    /// The payload of the `window.*` events.
    pub struct WindowEvent {
        pub window: WindowId,
    }

    // --- payloads: phones ---

    /// The payload of `phone.paired`, `phone.renamed` and `phone.removed`. The
    /// event never holds a code or a token.
    pub struct PhoneEvent {
        pub device: DeviceId,
        pub name: String,
    }

    /// The payload of `phone.pushed`: the push service took a push of a run
    /// for a phone. The event holds the reason, never the text of the push.
    pub struct PhonePushed {
        pub device: DeviceId,
        pub run: RunId,
        pub reason: AttentionReason,
    }

    /// The payload of `phone.push_failed`: a push of a run did not reach the push
    /// service of a phone.
    pub struct PhonePushFailed {
        pub device: DeviceId,
        /// The last status of the push service. `None` when no answer came.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub status: Option<u16>,
    }

    /// The payload of `phone.replied`: a phone sent `agent.reply` to a run. The event
    /// does not hold the text.
    pub struct PhoneReplied {
        pub device: DeviceId,
        pub run: RunId,
    }

    /// The payload of `git.repo_added`: `git_repos` keeps a repository that the person added.
    pub struct GitRepoAdded {
        /// The path that the person gave.
        pub path: String,
        /// The common git directory of the repository.
        pub common_dir: String,
    }

    /// The payload of `git.trusted` and `git.untrusted`: the person trusts a repository, or takes
    /// the trust back.
    pub struct GitTrustEvent {
        /// The path that the person gave.
        pub path: String,
        /// The common git directory of the repository: the key of the trust.
        pub common_dir: String,
    }

    /// The payload of `git.switched`, `git.unstashed`, `git.fetched` and `git.pulled`: a write of
    /// the Git view that worked. A write that failed appends no event.
    pub struct GitWrite {
        /// The worktree, or the common git directory for a fetch.
        pub path: String,
        /// The git command, for example `git switch --end-of-options main`.
        pub command: String,
        /// The branch before a switch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub from: Option<String>,
        /// The branch after a switch or a pull.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub to: Option<String>,
        /// The stash that a switch made or that `git.unstash` put back.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub stash: Option<String>,
        /// The agents in the worktree at that time.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub agents: Vec<RunId>,
        /// The fetch ran in the background, not from a click.
        #[serde(default)]
        pub background: bool,
    }

    /// The payload of `policy.added` and `policy.removed`.
    pub struct PolicyEvent {
        pub id: String,
        pub scope: PolicyScope,
    }

    /// The payload of `workflow.execution_started` and `workflow.execution_ended`.
    pub struct WorkflowExecutionEvent {
        pub execution: ExecutionId,
        /// The name of the workflow file without `.toml`.
        pub workflow: String,
        pub state: ExecutionState,
    }

    /// The payload of `hermes.chat_started` and `hermes.chat_closed`.
    pub struct HermesChatEvent {
        pub chat: ChatId,
        pub cwd: String,
    }

    /// The payload of `workflow.node_changed`.
    pub struct WorkflowNodeEvent {
        pub execution: ExecutionId,
        pub node: String,
        pub state: NodeState,
        /// The agent run of the node, while it has one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub run: Option<RunId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub reason: Option<String>,
    }
}

/// What a kind of event is, for the registry.
pub struct EventSpec {
    pub kind: &'static str,
    /// The version of the payload.
    pub v: u16,
    pub payload: fn(&mut SchemaGenerator) -> Schema,
}

/// Declares the kinds, the typed payload enum and the registry from one table.
macro_rules! events {
    ($($variant:ident = $kind:literal ($payload:ty);)*) => {
        /// The kinds of event. A payload type can serve several kinds.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum EventKind {
            $($variant),*
        }

        impl EventKind {
            pub const ALL: &'static [EventKind] = &[$(EventKind::$variant),*];

            pub const fn name(self) -> &'static str {
                match self {
                    $(EventKind::$variant => $kind),*
                }
            }

            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($kind => Some(EventKind::$variant),)*
                    _ => None,
                }
            }
        }

        /// A payload with its kind.
        #[derive(Clone, Debug, PartialEq)]
        pub enum EventPayload {
            $($variant($payload)),*
        }

        impl EventPayload {
            pub const fn kind(&self) -> EventKind {
                match self {
                    $(EventPayload::$variant(_) => EventKind::$variant),*
                }
            }

            /// The payload as JSON.
            pub fn to_value(&self) -> Value {
                match self {
                    $(EventPayload::$variant(payload) => {
                        serde_json::to_value(payload).unwrap_or(Value::Null)
                    }),*
                }
            }

            /// Reads a payload of a kind.
            pub fn parse(kind: EventKind, payload: Value) -> Result<Self, serde_json::Error> {
                Ok(match kind {
                    $(EventKind::$variant => {
                        EventPayload::$variant(serde_json::from_value(payload)?)
                    }),*
                })
            }
        }

        /// Every kind with the schema of its payload.
        pub fn specs() -> Vec<EventSpec> {
            vec![$(EventSpec {
                kind: $kind,
                v: 1,
                payload: |generator| generator.subschema_for::<$payload>(),
            }),*]
        }
    };
}

events! {
    AgentDetected = "agent.detected" (AgentDetected);
    AgentIdentified = "agent.identified" (AgentIdentified);
    AgentStateChanged = "agent.state_changed" (AgentStateChanged);
    AgentHarnessVersionUnvalidated = "agent.harness_version_unvalidated" (AgentHarnessVersionUnvalidated);
    AgentDriftDetected = "agent.drift_detected" (AgentDriftDetected);
    AdapterDisagreement = "adapter.disagreement" (AdapterDisagreement);
    HarnessManifestLoaded = "harness.manifest_loaded" (HarnessManifestLoaded);
    InteractionOpened = "interaction.opened" (InteractionOpened);
    InteractionDecided = "interaction.decided" (InteractionDecided);
    InteractionDeliveryStarted = "interaction.delivery_started" (InteractionDelivery);
    InteractionDelivered = "interaction.delivered" (InteractionDelivery);
    InteractionDeliveryUnknown = "interaction.delivery_unknown" (InteractionDelivery);
    InteractionDeliveryFailed = "interaction.delivery_failed" (InteractionDeliveryFailed);
    InteractionResolvedElsewhere = "interaction.resolved_elsewhere" (InteractionEnded);
    InteractionDismissed = "interaction.dismissed" (InteractionEnded);
    InteractionCancelled = "interaction.cancelled" (InteractionCancelled);
    PaneInputUnconfirmed = "pane.input_unconfirmed" (PaneInputUnconfirmed);
    PaneCreated = "pane.created" (PaneEvent);
    PaneClosed = "pane.closed" (PaneEvent);
    PaneMoved = "pane.moved" (PaneEvent);
    PaneFloated = "pane.floated" (PaneEvent);
    PaneUnfloated = "pane.unfloated" (PaneEvent);
    PaneRestarted = "pane.restarted" (PaneEvent);
    PaneExited = "pane.exited" (PaneExited);
    PaneBell = "pane.bell" (PaneBell);
    WorkspaceCreated = "workspace.created" (WorkspaceEvent);
    WorkspaceRenamed = "workspace.renamed" (WorkspaceEvent);
    WorkspaceMoved = "workspace.moved" (WorkspaceEvent);
    WorkspaceClosed = "workspace.closed" (WorkspaceEvent);
    GroupCreated = "group.created" (GroupEvent);
    GroupRenamed = "group.renamed" (GroupEvent);
    GroupMoved = "group.moved" (GroupEvent);
    GroupClosed = "group.closed" (GroupEvent);
    TabCreated = "tab.created" (TabEvent);
    TabRenamed = "tab.renamed" (TabEvent);
    TabMoved = "tab.moved" (TabEvent);
    TabClosed = "tab.closed" (TabEvent);
    TabSelected = "tab.selected" (TabEvent);
    LayoutChanged = "layout.changed" (LayoutChanged);
    WindowOpened = "window.opened" (WindowEvent);
    WindowClosed = "window.closed" (WindowEvent);
    PhonePaired = "phone.paired" (PhoneEvent);
    PhoneRenamed = "phone.renamed" (PhoneEvent);
    PhoneRemoved = "phone.removed" (PhoneEvent);
    PhonePushed = "phone.pushed" (PhonePushed);
    PhonePushFailed = "phone.push_failed" (PhonePushFailed);
    PhoneReplied = "phone.replied" (PhoneReplied);
    PolicyAdded = "policy.added" (PolicyEvent);
    PolicyRemoved = "policy.removed" (PolicyEvent);
    GitRepoAdded = "git.repo_added" (GitRepoAdded);
    GitTrusted = "git.trusted" (GitTrustEvent);
    GitUntrusted = "git.untrusted" (GitTrustEvent);
    GitSwitched = "git.switched" (GitWrite);
    GitUnstashed = "git.unstashed" (GitWrite);
    GitFetched = "git.fetched" (GitWrite);
    GitPulled = "git.pulled" (GitWrite);
    WorkflowExecutionStarted = "workflow.execution_started" (WorkflowExecutionEvent);
    WorkflowNodeChanged = "workflow.node_changed" (WorkflowNodeEvent);
    WorkflowExecutionEnded = "workflow.execution_ended" (WorkflowExecutionEvent);
    HermesChatStarted = "hermes.chat_started" (HermesChatEvent);
    HermesChatClosed = "hermes.chat_closed" (HermesChatEvent);
}

impl Event {
    /// Makes an event of a typed payload. `seq` and `at` come from the log.
    pub fn new(seq: u64, at: Timestamp, subject: Option<String>, payload: &EventPayload) -> Self {
        Self {
            seq,
            at,
            kind: payload.kind().name().to_owned(),
            v: 1,
            subject,
            payload: payload.to_value(),
        }
    }

    /// The typed payload. `None` when the kind is not one of this version of the contract.
    pub fn typed(&self) -> Option<Result<EventPayload, serde_json::Error>> {
        EventKind::from_name(&self.kind).map(|kind| EventPayload::parse(kind, self.payload.clone()))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn every_kind_has_a_unique_name_that_round_trips() {
        let mut seen = std::collections::BTreeSet::new();
        for kind in EventKind::ALL {
            assert!(seen.insert(kind.name()), "{} twice", kind.name());
            assert_eq!(EventKind::from_name(kind.name()), Some(*kind));
            assert!(kind.name().contains('.'), "{}", kind.name());
        }
        assert_eq!(specs().len(), EventKind::ALL.len());
    }

    #[test]
    fn a_typed_event_round_trips_through_the_log_form() {
        let run: RunId = "01M4F1W7EM2GN3EV6NHXZJEGGA".parse().expect("an id");
        let payload = EventPayload::AgentStateChanged(AgentStateChanged {
            run,
            facet: Facet::Execution,
            from: json!({"state": "working"}),
            to: json!({"state": "idle"}),
            source: Source::Structured,
            confidence: 1.0,
        });
        let event = Event::new(
            9,
            Timestamp(1_791_549_296_789),
            Some(run.to_string()),
            &payload,
        );
        assert_eq!(event.kind, "agent.state_changed");
        let wire = serde_json::to_string(&event).expect("serializes");
        let back: Event = serde_json::from_str(&wire).expect("decodes");
        assert_eq!(back.typed().expect("known kind").expect("decodes"), payload);
    }

    #[test]
    fn an_unknown_kind_has_no_typed_payload() {
        let event = Event {
            seq: 1,
            at: Timestamp(0),
            kind: "future.thing".to_owned(),
            v: 3,
            subject: None,
            payload: Value::Null,
        };
        assert!(event.typed().is_none());
    }

    #[test]
    fn a_payload_of_the_wrong_shape_is_an_error() {
        let bad = EventPayload::parse(EventKind::PaneExited, json!({"code": 1}));
        assert!(bad.is_err(), "a pane id is missing");
    }
}
