//! Projection names and the messages of a subscription.
//!
//! A projection is a keyed map plus an order. A client gets a snapshot, then patches. A singleton
//! projection has one key, `_`.

use std::{borrow::Cow, collections::BTreeMap, fmt, str::FromStr};

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_json::Value;

use super::{
    common::wire,
    ids::{IdError, PaneId, RunId, WindowId},
    model::{
        AgentValue, AttentionValue, ChangeRow, ChangesHead, HarnessValue, HealthValue,
        InteractionValue, JobValue, LimitValue, LinkValue, MachineValue, NotificationValue,
        PaletteValue, PaneValue, PhonePairingValue, PhoneStatusValue, PhoneValue, SettingsSection,
        TimelineItem, WindowValue, WorkspaceValue,
    },
};

/// The key of a singleton projection.
pub const SINGLETON_KEY: &str = "_";

/// A projection that a client can subscribe to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Projection {
    Windows,
    Tree,
    Panes,
    Agents,
    Attention,
    Interactions,
    /// The items of one run (`timeline:<run>`).
    Timeline(RunId),
    Notifications,
    Machines,
    /// The harnesses of this machine: installed, version, login state.
    Harnesses,
    Limits,
    Jobs,
    Palette,
    Settings,
    Health,
    /// The link of this window to its machine. The host of the client serves it, and no
    /// core does.
    Link,
    /// The text of the visible rows of one pane, for accessibility (`pane_text:<pane>`).
    PaneText(PaneId),
    /// The rows of `agents` with the read state of one window (`agents_for:<window>`).
    AgentsFor(WindowId),
    /// The changed files of the folder that the Changes tab of one window follows
    /// (`changes_for:<window>`).
    ChangesFor(WindowId),
    /// The head of the Changes tab of one window: the folder, its worktree and the totals
    /// (`changes_head:<window>`).
    ChangesHead(WindowId),
    /// The paired phones.
    Phones,
    /// The offer of a pairing, with its codes. A phone cannot follow it.
    PhonePairing,
    /// The listener of the phone.
    PhoneStatus,
}

impl Projection {
    /// The projections without a parameter, in a fixed order.
    pub const STATIC: [Self; 16] = [
        Self::Windows,
        Self::Tree,
        Self::Panes,
        Self::Agents,
        Self::Attention,
        Self::Interactions,
        Self::Notifications,
        Self::Machines,
        Self::Harnesses,
        Self::Limits,
        Self::Jobs,
        Self::Palette,
        Self::Settings,
        Self::Phones,
        Self::PhonePairing,
        Self::PhoneStatus,
    ];

    /// Whether the projection has one key, `_`.
    pub const fn is_singleton(self) -> bool {
        matches!(
            self,
            Self::Palette
                | Self::Health
                | Self::Link
                | Self::ChangesHead(_)
                | Self::PhonePairing
                | Self::PhoneStatus
        )
    }

    /// The name without the parameter.
    pub const fn base_name(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Tree => "tree",
            Self::Panes => "panes",
            Self::Agents => "agents",
            Self::Attention => "attention",
            Self::Interactions => "interactions",
            Self::Timeline(_) => "timeline",
            Self::Notifications => "notifications",
            Self::Machines => "machines",
            Self::Harnesses => "harnesses",
            Self::Limits => "limits",
            Self::Jobs => "jobs",
            Self::Palette => "palette",
            Self::Settings => "settings",
            Self::Health => "health",
            Self::Link => "link",
            Self::PaneText(_) => "pane_text",
            Self::AgentsFor(_) => "agents_for",
            Self::ChangesFor(_) => "changes_for",
            Self::ChangesHead(_) => "changes_head",
            Self::Phones => "phones",
            Self::PhonePairing => "phone_pairing",
            Self::PhoneStatus => "phone_status",
        }
    }
}

impl fmt::Display for Projection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeline(run) => write!(f, "timeline:{run}"),
            Self::PaneText(pane) => write!(f, "pane_text:{pane}"),
            Self::AgentsFor(window) => write!(f, "agents_for:{window}"),
            Self::ChangesFor(window) => write!(f, "changes_for:{window}"),
            Self::ChangesHead(window) => write!(f, "changes_head:{window}"),
            other => f.write_str(other.base_name()),
        }
    }
}

impl FromStr for Projection {
    type Err = IdError;

    fn from_str(text: &str) -> Result<Self, IdError> {
        if let Some(run) = text.strip_prefix("timeline:") {
            return run.parse().map(Self::Timeline);
        }
        if let Some(pane) = text.strip_prefix("pane_text:") {
            return pane.parse().map(Self::PaneText);
        }
        if let Some(window) = text.strip_prefix("agents_for:") {
            return window.parse().map(Self::AgentsFor);
        }
        if let Some(window) = text.strip_prefix("changes_for:") {
            return window.parse().map(Self::ChangesFor);
        }
        if let Some(window) = text.strip_prefix("changes_head:") {
            return window.parse().map(Self::ChangesHead);
        }
        Self::STATIC
            .iter()
            .chain(&[Self::Health, Self::Link])
            .find(|projection| projection.base_name() == text)
            .copied()
            .ok_or_else(|| IdError::NotRef(text.to_owned(), "a projection"))
    }
}

impl Serialize for Projection {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Projection {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(de::Error::custom)
    }
}

impl JsonSchema for Projection {
    fn schema_name() -> Cow<'static, str> {
        "Projection".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^(windows|tree|panes|agents|attention|interactions|notifications|machines|harnesses|limits|jobs|palette|settings|phones|phone_pairing|phone_status|health|link|timeline:[0-9A-HJKMNP-TV-Z]{26}|pane_text:[0-9A-HJKMNP-TV-Z]{26}|agents_for:[0-9A-HJKMNP-TV-Z]{26}|changes_for:[0-9A-HJKMNP-TV-Z]{26}|changes_head:[0-9A-HJKMNP-TV-Z]{26})$",
            "description": "The name of a projection. timeline:<run>, pane_text:<pane>, agents_for:<window>, changes_for:<window> and changes_head:<window> take an id."
        })
    }
}

wire! {
    /// The state of a projection: the values by key, and the order of the keys.
    #[derive(Default)]
    pub struct ProjectionData {
        pub items: BTreeMap<String, Value>,
        pub order: Vec<String>,
    }

    /// One change of a patch.
    #[serde(tag = "op", rename_all = "snake_case")]
    pub enum PatchOp {
        /// Sets the value of a key. The key is new or it exists.
        Set { key: String, value: Value },
        Remove { key: String },
        /// Sets the order of all keys.
        Order { keys: Vec<String> },
    }

    /// What a subscription delivers: a snapshot once, then patches. `resync` means that
    /// the client is older than the core retains, and it subscribes again.
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum ProjectionMessage {
        Snapshot { version: u64, data: ProjectionData },
        Patch { version: u64, ops: Vec<PatchOp> },
        Resync,
    }
}

/// What a client sees of one projection: the name, the type of the key and of the value. The
/// registry that `nh api schema` prints is made from this.
pub struct ProjectionSpec {
    pub name: &'static str,
    /// What the key is.
    pub key: &'static str,
    pub singleton: bool,
    /// Whether the name has a parameter, as in `timeline:<run>`.
    pub parameter: Option<&'static str>,
    pub value: fn(&mut SchemaGenerator) -> Schema,
}

macro_rules! spec {
    ($name:literal, $key:literal, $value:ty) => {
        ProjectionSpec {
            name: $name,
            key: $key,
            singleton: false,
            parameter: None,
            value: |generator| generator.subschema_for::<$value>(),
        }
    };
    ($name:literal, $key:literal, $value:ty, param $param:literal) => {
        ProjectionSpec {
            name: $name,
            key: $key,
            singleton: false,
            parameter: Some($param),
            value: |generator| generator.subschema_for::<$value>(),
        }
    };
    ($name:literal, $value:ty, singleton, param $param:literal) => {
        ProjectionSpec {
            name: $name,
            key: "_",
            singleton: true,
            parameter: Some($param),
            value: |generator| generator.subschema_for::<$value>(),
        }
    };
    ($name:literal, $value:ty, singleton) => {
        ProjectionSpec {
            name: $name,
            key: "_",
            singleton: true,
            parameter: None,
            value: |generator| generator.subschema_for::<$value>(),
        }
    };
}

/// Every projection with its value type.
pub fn specs() -> Vec<ProjectionSpec> {
    vec![
        spec!("windows", "window id", WindowValue),
        spec!("tree", "workspace id", WorkspaceValue),
        spec!("panes", "pane id", PaneValue),
        spec!("agents", "run id", AgentValue),
        spec!("attention", "run id", AttentionValue),
        spec!("interactions", "interaction id", InteractionValue),
        spec!("timeline", "item id", TimelineItem, param "run id"),
        spec!("notifications", "notification id", NotificationValue),
        spec!("machines", "machine label", MachineValue),
        spec!("harnesses", "harness id", HarnessValue),
        spec!("limits", "account and harness", LimitValue),
        spec!("jobs", "job id", JobValue),
        spec!("palette", PaletteValue, singleton),
        spec!("settings", "section name", SettingsSection),
        spec!("health", HealthValue, singleton),
        spec!("link", LinkValue, singleton),
        spec!("pane_text", "row number", String, param "pane id"),
        spec!("agents_for", "run id", AgentValue, param "window id"),
        spec!("changes_for", "path", ChangeRow, param "window id"),
        spec!("changes_head", ChangesHead, singleton, param "window id"),
        spec!("phones", "device id", PhoneValue),
        spec!("phone_pairing", PhonePairingValue, singleton),
        spec!("phone_status", PhoneStatusValue, singleton),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_round_trips() {
        let run: RunId = "01M4F1W7EM2GN3EV6NHXZJEGGA".parse().expect("an id");
        let pane: PaneId = "01M4F1W7EM2GN3EV6NHXZJEGGB".parse().expect("an id");
        let mut all: Vec<Projection> = Projection::STATIC.to_vec();
        all.push(Projection::Health);
        all.push(Projection::Link);
        all.push(Projection::Timeline(run));
        all.push(Projection::PaneText(pane));
        all.push(Projection::AgentsFor(
            "01M4F1W7EM2GN3EV6NHXZJEGGC".parse().expect("an id"),
        ));
        all.push(Projection::ChangesFor(
            "01M4F1W7EM2GN3EV6NHXZJEGGE".parse().expect("an id"),
        ));
        all.push(Projection::ChangesHead(
            "01M4F1W7EM2GN3EV6NHXZJEGGF".parse().expect("an id"),
        ));
        for projection in all {
            let text = projection.to_string();
            assert_eq!(
                text.parse::<Projection>().expect("parses"),
                projection,
                "{text}"
            );
            let json = serde_json::to_string(&projection).expect("serializes");
            assert_eq!(
                serde_json::from_str::<Projection>(&json).expect("decodes"),
                projection
            );
        }
    }

    #[test]
    fn a_wrong_name_is_refused() {
        for bad in [
            "",
            "window",
            "timeline",
            "timeline:w3",
            "pane_text:",
            "agents_for:",
            "agents_for:w3",
            "Tree",
            "timeline:x",
        ] {
            assert!(bad.parse::<Projection>().is_err(), "{bad}");
        }
    }

    #[test]
    fn the_messages_have_the_shape_of_the_spec() {
        let snapshot = ProjectionMessage::Snapshot {
            version: 4,
            data: ProjectionData::default(),
        };
        assert_eq!(
            serde_json::to_value(&snapshot).expect("serializes"),
            serde_json::json!({"type": "snapshot", "version": 4, "data": {"items": {}, "order": []}})
        );
        let patch = ProjectionMessage::Patch {
            version: 5,
            ops: vec![
                PatchOp::Set {
                    key: "a".into(),
                    value: Value::Bool(true),
                },
                PatchOp::Remove { key: "b".into() },
                PatchOp::Order {
                    keys: vec!["a".into()],
                },
            ],
        };
        assert_eq!(
            serde_json::to_value(&patch).expect("serializes"),
            serde_json::json!({"type": "patch", "version": 5, "ops": [
                {"op": "set", "key": "a", "value": true},
                {"op": "remove", "key": "b"},
                {"op": "order", "keys": ["a"]}]})
        );
        assert_eq!(
            serde_json::to_value(ProjectionMessage::Resync).expect("serializes"),
            serde_json::json!({"type": "resync"})
        );
    }

    #[test]
    fn six_projections_are_singletons() {
        let singles: Vec<_> = specs()
            .into_iter()
            .filter(|s| s.singleton)
            .map(|s| s.name)
            .collect();
        assert_eq!(
            singles,
            [
                "palette",
                "health",
                "link",
                "changes_head",
                "phone_pairing",
                "phone_status"
            ]
        );
        let window: WindowId = "01M4F1W7EM2GN3EV6NHXZJEGGC".parse().expect("an id");
        assert!(
            Projection::Palette.is_singleton()
                && Projection::Health.is_singleton()
                && Projection::Link.is_singleton()
                && Projection::ChangesHead(window).is_singleton()
                && Projection::PhonePairing.is_singleton()
                && Projection::PhoneStatus.is_singleton()
        );
        assert!(!Projection::Phones.is_singleton());
        assert!(!Projection::Tree.is_singleton());
        assert!(!Projection::ChangesFor(window).is_singleton());
    }

    #[test]
    fn the_registry_covers_every_name() {
        let names: Vec<_> = specs().iter().map(|s| s.name).collect();
        for projection in Projection::STATIC {
            assert!(names.contains(&projection.base_name()), "{projection}");
        }
        assert!(
            names.contains(&"health")
                && names.contains(&"link")
                && names.contains(&"timeline")
                && names.contains(&"pane_text")
        );
    }
}
