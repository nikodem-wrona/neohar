//! Ids and handles.
//!
//! Every object has a ULID inside the core. The API shows a ULID as its 26-character text. A
//! person types a short **handle**: a workspace `w3`, a tab `w3:t2`, a pane `w3:p5`, an
//! interaction `i42`, a preview `v4`. A handle on another machine has the label of the
//! machine in front: `devbox/w3:p5`. A handle is assigned once and never changes. The API
//! accepts both forms in a [`Ref`]. An agent run can also be addressed by its unique name.

use std::{borrow::Cow, fmt, str::FromStr};

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use thiserror::Error;
use ulid::Ulid;

/// Why a text is not an id, a handle or a name.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum IdError {
    #[error("not a ULID: \"{0}\"")]
    NotUlid(String),
    #[error("not a handle: \"{0}\" (examples: w3, w3:t2, w3:p5, i42, v4, devbox/w3:p5)")]
    NotHandle(String),
    #[error("\"{given}\" is a {found} handle, and this call needs {wanted}")]
    WrongKind {
        given: String,
        found: &'static str,
        wanted: &'static str,
    },
    #[error("\"{0}\" is not an id, a handle or a name of {1}")]
    NotRef(String, &'static str),
}

/// The text of a ULID has 26 characters.
const ULID_LEN: usize = 26;
const ULID_PATTERN: &str = "^[0-9A-HJKMNP-TV-Z]{26}$";

macro_rules! ulid_id {
    ($(#[$meta:meta])* $name:ident, $what:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub Ulid);

        impl $name {
            pub const fn new(ulid: Ulid) -> Self {
                Self(ulid)
            }

            pub const fn ulid(self) -> Ulid {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(text: &str) -> Result<Self, IdError> {
                if text.len() != ULID_LEN {
                    return Err(IdError::NotUlid(text.to_owned()));
                }
                Ulid::from_string(text)
                    .map(Self)
                    .map_err(|_| IdError::NotUlid(text.to_owned()))
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = String::deserialize(deserializer)?;
                text.parse().map_err(de::Error::custom)
            }
        }

        impl JsonSchema for $name {
            fn schema_name() -> Cow<'static, str> {
                stringify!($name).into()
            }

            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                schemars::json_schema!({
                    "type": "string",
                    "pattern": ULID_PATTERN,
                    "description": concat!("The ULID of a ", $what, ", as 26 characters.")
                })
            }
        }
    };
}

ulid_id!(
    /// A workspace.
    WorkspaceId, "workspace"
);
ulid_id!(
    /// A tab.
    TabId, "tab"
);
ulid_id!(
    /// A pane.
    PaneId, "pane"
);
ulid_id!(
    /// A run of an agent.
    RunId, "run of an agent"
);
ulid_id!(
    /// An interaction (an approval, a question, a plan review or a notice).
    InteractionId, "interaction"
);
ulid_id!(
    /// A window of the app.
    WindowId, "window"
);
ulid_id!(
    /// A long-running command (`jobs` projection).
    JobId, "job"
);
ulid_id!(
    /// A group of workspaces.
    GroupId, "group of workspaces"
);
ulid_id!(
    /// An entry of the `notifications` projection.
    NotificationId, "notification"
);
ulid_id!(
    /// A client device: the app that shows windows of a machine. It is made at the first
    /// launch of the app.
    DeviceId, "client device"
);
ulid_id!(
    /// One execution of a workflow.
    ExecutionId, "workflow execution"
);

/// What a handle points to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Workspace(u32),
    Tab { workspace: u32, tab: u32 },
    Pane { workspace: u32, pane: u32 },
    Interaction(u32),
    Preview(u32),
}

impl Target {
    /// The kind of thing, for messages.
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Workspace(_) => "workspace",
            Self::Tab { .. } => "tab",
            Self::Pane { .. } => "pane",
            Self::Interaction(_) => "interaction",
            Self::Preview(_) => "preview",
        }
    }
}

/// A short handle, with the label of the machine when the object is not on the local one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Handle {
    pub machine: Option<String>,
    pub target: Target,
}

/// The longest label of a machine.
const MACHINE_MAX: usize = 32;

fn valid_machine(label: &str) -> bool {
    let mut chars = label.chars();
    label.len() <= MACHINE_MAX
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A counter: digits without a leading zero, at most 10 of them, in `u32`.
fn number(text: &str) -> Option<u32> {
    let valid = !text.is_empty()
        && text.len() <= 10
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    if valid { text.parse().ok() } else { None }
}

impl FromStr for Handle {
    type Err = IdError;

    fn from_str(text: &str) -> Result<Self, IdError> {
        let bad = || IdError::NotHandle(text.to_owned());
        let (machine, body) = match text.split_once('/') {
            Some((label, body)) if valid_machine(label) => (Some(label.to_owned()), body),
            Some(_) => return Err(bad()),
            None => (None, text),
        };
        let target = if let Some((workspace, rest)) = body.split_once(':') {
            let workspace = body_number(workspace, 'w').ok_or_else(bad)?;
            if let Some(tab) = body_number(rest, 't') {
                Target::Tab { workspace, tab }
            } else if let Some(pane) = body_number(rest, 'p') {
                Target::Pane { workspace, pane }
            } else {
                return Err(bad());
            }
        } else if let Some(n) = body_number(body, 'w') {
            Target::Workspace(n)
        } else if let Some(n) = body_number(body, 'i') {
            Target::Interaction(n)
        } else if let Some(n) = body_number(body, 'v') {
            Target::Preview(n)
        } else {
            return Err(bad());
        };
        Ok(Self { machine, target })
    }
}

/// `w3` with the letter `w`: the counter, or `None`.
fn body_number(text: &str, letter: char) -> Option<u32> {
    number(text.strip_prefix(letter)?)
}

impl fmt::Display for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(machine) = &self.machine {
            write!(f, "{machine}/")?;
        }
        match self.target {
            Target::Workspace(n) => write!(f, "w{n}"),
            Target::Tab { workspace, tab } => write!(f, "w{workspace}:t{tab}"),
            Target::Pane { workspace, pane } => write!(f, "w{workspace}:p{pane}"),
            Target::Interaction(n) => write!(f, "i{n}"),
            Target::Preview(n) => write!(f, "v{n}"),
        }
    }
}

impl Serialize for Handle {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Handle {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(de::Error::custom)
    }
}

impl JsonSchema for Handle {
    fn schema_name() -> Cow<'static, str> {
        "Handle".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^([a-z0-9][a-z0-9-]{0,31}/)?(w[0-9]+|w[0-9]+:t[0-9]+|w[0-9]+:p[0-9]+|i[0-9]+|k[0-9]+|v[0-9]+)$",
            "description": "A short handle: w3, w3:t2, w3:p5, i42, v4, with devbox/ in front on another machine."
        })
    }
}

/// What a [`Ref`] can point to.
pub trait RefKind {
    /// What the call needs, for messages.
    const WHAT: &'static str;
    /// Whether the kind can be named by its unique name.
    const NAMED: bool = false;
    /// Whether the kind has a short handle.
    const HANDLES: bool = true;
    /// Whether the handle points to this kind of thing.
    fn accepts(target: Target) -> bool;
}

macro_rules! ref_kind {
    ($id:ident, $what:literal, $named:literal, $accepts:pat) => {
        impl RefKind for $id {
            const WHAT: &'static str = $what;
            const NAMED: bool = $named;

            fn accepts(target: Target) -> bool {
                matches!(target, $accepts)
            }
        }
    };
    // A kind without a short handle.
    ($id:ident, $what:literal, $named:literal) => {
        impl RefKind for $id {
            const WHAT: &'static str = $what;
            const NAMED: bool = $named;
            const HANDLES: bool = false;

            fn accepts(_: Target) -> bool {
                false
            }
        }
    };
}

ref_kind!(WorkspaceId, "a workspace", false, Target::Workspace(_));
ref_kind!(TabId, "a tab", false, Target::Tab { .. });
ref_kind!(PaneId, "a pane", false, Target::Pane { .. });
ref_kind!(
    InteractionId,
    "an interaction",
    false,
    Target::Interaction(_)
);
ref_kind!(RunId, "an agent run", true);
ref_kind!(GroupId, "a group", false);
ref_kind!(WindowId, "a window", false);
ref_kind!(JobId, "a job", false);

/// What a [`Ref`] holds.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RefValue<K> {
    Id(K),
    Handle(Handle),
    Name(String),
}

/// A reference to an object in a call: its ULID, its short handle, or (for the kinds that have
/// one) its unique name. The core resolves it. A handle of the wrong kind is an error, and so is a
/// name for a kind that has none.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Ref<K>(pub RefValue<K>);

impl<K> Ref<K> {
    pub fn id(id: K) -> Self {
        Self(RefValue::Id(id))
    }
}

/// The longest name of an agent or a group.
const NAME_MAX: usize = 64;

fn valid_name(text: &str) -> bool {
    let mut chars = text.chars();
    text.len() <= NAME_MAX
        && chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

impl<K: RefKind + FromStr<Err = IdError>> FromStr for Ref<K> {
    type Err = IdError;

    fn from_str(text: &str) -> Result<Self, IdError> {
        if text.len() == ULID_LEN
            && let Ok(id) = text.parse::<K>()
        {
            return Ok(Self(RefValue::Id(id)));
        }
        match text.parse::<Handle>() {
            Ok(handle) if K::accepts(handle.target) => return Ok(Self(RefValue::Handle(handle))),
            Ok(handle) => {
                return Err(IdError::WrongKind {
                    given: text.to_owned(),
                    found: handle.target.kind(),
                    wanted: K::WHAT,
                });
            }
            Err(_) => {}
        }
        if K::NAMED && valid_name(text) {
            return Ok(Self(RefValue::Name(text.to_owned())));
        }
        Err(IdError::NotRef(text.to_owned(), K::WHAT))
    }
}

impl<K: fmt::Display> fmt::Display for Ref<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            RefValue::Id(id) => id.fmt(f),
            RefValue::Handle(handle) => handle.fmt(f),
            RefValue::Name(name) => f.write_str(name),
        }
    }
}

impl<K: fmt::Display> Serialize for Ref<K> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de, K: RefKind + FromStr<Err = IdError>> Deserialize<'de> for Ref<K> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(de::Error::custom)
    }
}

impl<K: RefKind + JsonSchema> JsonSchema for Ref<K> {
    fn schema_name() -> Cow<'static, str> {
        format!("Ref{}", K::schema_name()).into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        let mut forms = vec!["its ULID"];
        if K::HANDLES {
            forms.push("its short handle");
        }
        if K::NAMED {
            forms.push("its unique name");
        }
        schemars::json_schema!({
            "type": "string",
            "description": format!("A reference to {}: {}.", K::WHAT, forms.join(", or "))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ULID_TEXT: &str = "01M4F1W7EM2GN3EV6NHXZJEGGA";

    #[test]
    fn a_ulid_id_round_trips_as_text() {
        let id: PaneId = ULID_TEXT.parse().expect("a ULID");
        assert_eq!(id.to_string(), ULID_TEXT);
        let json = serde_json::to_string(&id).expect("serializes");
        assert_eq!(json, format!("\"{ULID_TEXT}\""));
        let back: PaneId = serde_json::from_str(&json).expect("decodes");
        assert_eq!(back, id);
    }

    #[test]
    fn a_text_that_is_not_a_ulid_is_refused() {
        for bad in [
            "",
            "w3",
            "01M4F1W7EM2GN3EV6NHXZJEGG",
            "01M4F1W7EM2GN3EV6NHXZJEGGAA",
            "01M4F1W7EM2GN3EV6NHXZJEGGU",
        ] {
            assert!(bad.parse::<PaneId>().is_err(), "{bad}");
        }
    }

    #[test]
    fn every_handle_form_round_trips() {
        for text in [
            "w3",
            "w3:t2",
            "w3:p5",
            "i42",
            "v4",
            "devbox/w3:p5",
            "a1-b/i1",
            "w0",
            "w4294967295",
        ] {
            let handle: Handle = text.parse().unwrap_or_else(|e| panic!("{text}: {e}"));
            assert_eq!(handle.to_string(), text);
            let json = serde_json::to_string(&handle).expect("serializes");
            let back: Handle = serde_json::from_str(&json).expect("decodes");
            assert_eq!(back, handle);
        }
    }

    #[test]
    fn a_malformed_handle_is_refused() {
        for bad in [
            "",
            "w",
            "w03",
            "w3:",
            "w3:x2",
            "w3:p",
            ":p5",
            "p5",
            "W3",
            "w3:t2:p1",
            "w3 ",
            " w3",
            "w-1",
            "w99999999999",
            "i",
            "k0x7",
            "dev box/w3",
            "/w3",
            "devbox/",
            "DevBox/w3",
            "a/b/w3",
            "w3/p5",
        ] {
            assert!(bad.parse::<Handle>().is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn a_handle_shows_its_parts() {
        let handle: Handle = "devbox/w3:p5".parse().expect("a handle");
        assert_eq!(handle.machine.as_deref(), Some("devbox"));
        assert_eq!(
            handle.target,
            Target::Pane {
                workspace: 3,
                pane: 5
            }
        );
    }

    #[test]
    fn a_ref_takes_an_id_or_a_handle_of_its_kind() {
        let by_id: Ref<PaneId> = ULID_TEXT.parse().expect("an id");
        assert!(matches!(by_id.0, RefValue::Id(_)));
        let by_handle: Ref<PaneId> = "w3:p5".parse().expect("a handle");
        assert!(matches!(by_handle.0, RefValue::Handle(_)));
        let wrong = "w3"
            .parse::<Ref<PaneId>>()
            .expect_err("a workspace is not a pane");
        assert!(matches!(
            wrong,
            IdError::WrongKind {
                found: "workspace",
                ..
            }
        ));
        assert!("i42".parse::<Ref<InteractionId>>().is_ok());
    }

    #[test]
    fn a_run_is_addressed_by_id_or_by_name_but_not_by_a_handle() {
        let by_name: Ref<RunId> = "reviewer-2".parse().expect("a name");
        assert_eq!(by_name.0, RefValue::Name("reviewer-2".to_owned()));
        assert!("w3:p5".parse::<Ref<RunId>>().is_err());
        assert!("not a name".parse::<Ref<RunId>>().is_err());
        assert!("".parse::<Ref<RunId>>().is_err());
        assert!("w3:p5".parse::<Ref<WindowId>>().is_err());
        assert!(
            "some-name".parse::<Ref<WindowId>>().is_err(),
            "a window has no name"
        );
    }

    #[test]
    fn a_ref_round_trips_through_json() {
        for text in [ULID_TEXT, "w3:p5", "devbox/w3:p5"] {
            let r: Ref<PaneId> = serde_json::from_str(&format!("\"{text}\"")).expect("decodes");
            assert_eq!(
                serde_json::to_string(&r).expect("serializes"),
                format!("\"{text}\"")
            );
        }
    }
}
