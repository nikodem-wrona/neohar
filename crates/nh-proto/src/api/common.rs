//! Small types that many methods and projections share.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::ids::JobId;

/// Derives what every API type needs: `Clone`, `Debug`, `PartialEq`, serde and a JSON Schema.
macro_rules! wire {
    ($($item:item)*) => {
        $(
            #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
            $item
        )*
    };
}
pub(crate) use wire;

/// A field of a change: a missing field keeps the value, `null` clears it, and a value sets it.
/// Use it with `#[serde(default, deserialize_with = "nullable")]` on an `Option<Option<T>>`.
pub fn nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

wire! {
    /// An empty result or empty params: `{}`.
    #[derive(Default)]
    pub struct Empty {}

    /// The result of a command that runs as a job. Progress is in the `jobs` projection.
    pub struct JobStarted {
        pub job: JobId,
    }

    /// A rectangle as fractions of the area of the tab, from 0 to 1. A floating pane has one.
    #[derive(Copy)]
    pub struct Rect {
        pub x: f32,
        pub y: f32,
        pub w: f32,
        pub h: f32,
    }

    /// The bounds of a window on the screen, in logical pixels.
    #[derive(Copy)]
    pub struct Bounds {
        pub x: i32,
        pub y: i32,
        pub width: u32,
        pub height: u32,
    }

    /// Which side of a pane a new pane goes on.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum SplitDirection {
        Left,
        Right,
        Up,
        Down,
    }

    /// How a split node lays out its children.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum Axis {
        /// Side by side, from left to right.
        Row,
        /// One above the other, from top to bottom.
        Column,
    }

    /// What to run in a new pane. Without it the pane runs the login shell of the user.
    #[derive(Default)]
    pub struct ProcessSpec {
        /// The program and its arguments.
        pub argv: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub cwd: Option<String>,
        /// Extra variables for the environment of the process.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        pub env: BTreeMap<String, String>,
    }

    /// How risky a command or an action is.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum Risk {
        Low,
        Medium,
        High,
        Unknown,
    }

    /// How loud a notification is.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum Urgency {
        Low,
        Normal,
        High,
    }

    /// The surface a client answers from. The delivery record keeps it.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum Via {
        Card,
        Notification,
        Tray,
        Palette,
        Cli,
        /// A rule of the policy decided (step 2). No client answered.
        Policy,
        /// The phone app.
        Phone,
    }

    /// The two harnesses of v1.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum Harness {
        Claude,
        Codex,
    }

    /// Which renderer paints a window.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum RendererKind {
        Webgl2,
        Canvas2d,
    }

    /// Where a piece of state came from, and how sure the core is.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum Source {
        /// A hook or a protocol message of the harness.
        Structured,
        /// The VT screen matched a manifest rule.
        Screen,
        /// The process tree.
        Process,
        /// The holder.
        Holder,
        /// The user said so.
        User,
        /// A transcript file.
        Transcript,
        /// The core derived it from other state.
        Derived,
    }

    /// A line range of the archive of a pane, counted from the first line that the archive holds.
    /// `to` is not part of the range.
    #[derive(Copy)]
    pub struct LineRange {
        pub from: u64,
        pub to: u64,
    }
}

/// The serde default for a `bool` that is `true`.
pub(crate) fn yes() -> bool {
    true
}

/// A text with its bytes: base64 (standard alphabet, with padding) on the wire.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bytes(pub Vec<u8>);

impl Serialize for Bytes {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use base64::Engine as _;
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use base64::Engine as _;
        let text = String::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(text.as_bytes())
            .map(Self)
            .map_err(|e| serde::de::Error::custom(format!("not base64: {e}")))
    }
}

impl JsonSchema for Bytes {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Bytes".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "contentEncoding": "base64",
            "description": "Bytes as base64 (standard alphabet, with padding)."
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_base64() {
        let bytes = Bytes(vec![0, 1, 2, 250, 255]);
        let json = serde_json::to_string(&bytes).expect("serializes");
        assert_eq!(json, "\"AAEC+v8=\"");
        assert_eq!(
            serde_json::from_str::<Bytes>(&json).expect("decodes"),
            bytes
        );
        assert!(serde_json::from_str::<Bytes>("\"not base64!\"").is_err());
    }

    #[test]
    fn a_process_spec_needs_only_argv() {
        let spec: ProcessSpec = serde_json::from_str(r#"{"argv":["htop"]}"#).expect("decodes");
        assert_eq!(spec.argv, ["htop"]);
        assert!(spec.env.is_empty() && spec.cwd.is_none());
    }
}
