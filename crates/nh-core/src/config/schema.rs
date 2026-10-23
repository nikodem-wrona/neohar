//! The typed configuration.
//!
//! `defaults.toml` is the only place where a default value is written. [`Config`] is what the
//! rest of the core reads. The `Default` impls read that file, so there is one source. The types
//! have no `#[serde(default)]`: serde would call `Default` first, and `Default` deserializes. The
//! store merges the user's file over the table of the defaults before it deserializes, so no key
//! is ever missing.

use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// How the core treats a request for approval.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    Observe,
    Gate,
    Auto,
}

/// The mode of one harness: a mode of its own, or the global one (`approvals.mode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalChoice {
    Inherit,
    Observe,
    Gate,
    Auto,
}

/// What to do with the agents of the last session after a reboot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResumeOnLaunch {
    Ask,
    Always,
    Never,
}

/// A time in the file: `"30m"`, `"7d"`, `"90s"`, `"1h"`, `"500ms"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span(pub Duration);

impl Span {
    /// Parses a number and a unit: `ms`, `s`, `m`, `h` or `d`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let split = text
            .find(|c: char| !c.is_ascii_digit())
            .ok_or_else(|| format!("{text:?} has no unit (ms, s, m, h or d)"))?;
        let (number, unit) = text.split_at(split);
        let number: u64 = number
            .parse()
            .map_err(|_| format!("{text:?} does not start with a number"))?;
        let millis = match unit {
            "ms" => 1,
            "s" => 1_000,
            "m" => 60_000,
            "h" => 3_600_000,
            "d" => 86_400_000,
            other => return Err(format!("{other:?} is not a unit (ms, s, m, h or d)")),
        };
        number
            .checked_mul(millis)
            .map(|ms| Self(Duration::from_millis(ms)))
            .ok_or_else(|| format!("{text:?} is too long"))
    }

    /// The shortest text that parses back to the same time.
    pub fn text(self) -> String {
        let ms = u64::try_from(self.0.as_millis()).unwrap_or(u64::MAX);
        for (unit, size) in [
            ("d", 86_400_000),
            ("h", 3_600_000),
            ("m", 60_000),
            ("s", 1_000),
        ] {
            if ms != 0 && ms % size == 0 {
                return format!("{}{unit}", ms / size);
            }
        }
        format!("{ms}ms")
    }
}

impl Serialize for Span {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text())
    }
}

impl<'de> Deserialize<'de> for Span {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeApprovals {
    pub persist_always: bool,
    pub mode: ApprovalChoice,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexApprovals {
    pub mode: ApprovalChoice,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approvals {
    pub mode: ApprovalMode,
    pub gate_timeout: Span,
    pub claude: ClaudeApprovals,
    pub codex: CodexApprovals,
}

impl Approvals {
    /// The mode for the approvals of a harness: its own mode, else `approvals.mode`.
    pub fn mode_for(&self, harness: nh_proto::api::common::Harness) -> ApprovalMode {
        use nh_proto::api::common::Harness;
        let own = match harness {
            Harness::Claude => self.claude.mode,
            Harness::Codex => self.codex.mode,
        };
        match own {
            ApprovalChoice::Inherit => self.mode,
            ApprovalChoice::Observe => ApprovalMode::Observe,
            ApprovalChoice::Gate => ApprovalMode::Gate,
            ApprovalChoice::Auto => ApprovalMode::Auto,
        }
    }
}

/// The rules of the policy. A rule is kept as it is in the file, so that one
/// rule that is not valid does not make the whole file invalid. The policy checks each rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyConfig {
    pub rule: Vec<serde_json::Value>,
}

/// How a harness is paid for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Billing {
    /// Each token has a price. The usage shows a cost.
    Api,
    /// A subscription. The usage shows tokens and no dollars.
    Subscription,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessSettings {
    pub billing: Billing,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessTable {
    pub claude: HarnessSettings,
    pub codex: HarnessSettings,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agents {
    pub shims: bool,
    pub resume_on_launch: ResumeOnLaunch,
    /// A run whose turn ended while nobody looked gets a notification.
    pub turn_end_notice: bool,
    pub harness: HarnessTable,
}

impl Agents {
    /// How the runs of a harness are paid for.
    pub fn billing_for(&self, harness: nh_proto::api::common::Harness) -> Billing {
        use nh_proto::api::common::Harness;
        match harness {
            Harness::Claude => self.harness.claude.billing,
            Harness::Codex => self.harness.codex.billing,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Events {
    pub sync_retention: Span,
    pub audit_retention: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scrollback {
    pub max_bytes_per_pane: u64,
}

/// When `nh daemon` holds a power assertion against idle sleep.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreventSleep {
    /// While an agent run is `working` or a gate holds a hook.
    WhileAgentsWork,
    Always,
    Never,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Daemon {
    pub prevent_sleep: PreventSleep,
}

/// The Git view.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Git {
    /// The minutes between two fetches in the background. 0 turns them off.
    pub fetch_minutes: u32,
}

/// The listener of the phone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Phone {
    pub enabled: bool,
    pub port: u16,
    pub https_port: u16,
    pub url: String,
    pub contact: String,
    /// The app keeps the Mac out of idle sleep for a phone with a push subscription.
    pub keep_awake: bool,
    /// The font size of the panes on a phone, in CSS pixels.
    pub font_size: u8,
    /// The keys of the key bar of a phone, after the fixed ones.
    pub keys: Vec<String>,
}

/// Which Option keys of a Mac act as Alt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionAsAlt {
    None,
    Left,
    Right,
    Both,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ui {
    pub font_family: String,
    pub font_size: u8,
    pub macos_option_as_alt: OptionAsAlt,
    /// The chord that shows the app and the next card from any application. An empty
    /// text turns it off.
    pub global_hotkey: String,
    /// The app that opens a path of the Files tab (`fs.open`). An empty text is the
    /// default app of the path.
    pub editor: String,
    /// Whether the chrome shows the handles of tabs and panes, such as `w1:t2` and `w1:p3`.
    pub show_handles: bool,
}

/// The colours of the chrome, as CSS colours.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub bg: String,
    pub bg_raised: String,
    pub bg_hover: String,
    pub line: String,
    pub text: String,
    pub text_dim: String,
    pub accent: String,
    pub danger: String,
}

/// The reserved chords of a window. The names are the ids of the commands with `_` for `.`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keys {
    pub palette_open: String,
    pub tab_next: String,
    pub tab_previous: String,
    pub pane_focus_left: String,
    pub pane_focus_right: String,
    pub pane_focus_up: String,
    pub pane_focus_down: String,
    pub pane_split_right: String,
    pub pane_split_down: String,
    pub inbox_open: String,
    pub interaction_answer_next: String,
    pub peek_sibling: String,
    pub search_open: String,
    pub git_open: String,
    pub window_new: String,
    pub pass_through: String,
}

/// When the local core connects a machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectMode {
    /// At launch.
    Launch,
    /// Only when the person asks.
    Manual,
}

/// The fields of `[machines.<label>]`, as the file writes them.
pub const MACHINE_FIELDS: [&str; 3] = ["ssh", "session", "connect"];

/// The longest label of a machine.
pub const MAX_LABEL: usize = 32;

/// Whether `label` can name a machine: 1 to 32 characters of `a-z`, `0-9` and `-`, and not the
/// label of this machine.
pub fn valid_machine_label(label: &str) -> bool {
    (1..=MAX_LABEL).contains(&label.len())
        && label != "local"
        && label
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// One entry of `[machines]`. A machine has no default: the table is a map, and its
/// keys are the labels of the person. So the two optional fields have their defaults here and not
/// in `defaults.toml`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Machine {
    /// Any target that `ssh` accepts.
    pub ssh: String,
    /// The session on the machine.
    #[serde(default = "default_session")]
    pub session: String,
    #[serde(default = "default_connect")]
    pub connect: ConnectMode,
    /// The path of `nh` on the machine. For the tests of a debug build only; a release build
    /// ignores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_nh: Option<String>,
}

fn default_session() -> String {
    "default".to_owned()
}

fn default_connect() -> ConnectMode {
    ConnectMode::Manual
}

/// All settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// The machines of the person, by label. The order of the file is in
    /// [`super::ConfigStore::machines`].
    pub machines: std::collections::BTreeMap<String, Machine>,
    pub approvals: Approvals,
    pub policy: PolicyConfig,
    pub agents: Agents,
    pub events: Events,
    pub scrollback: Scrollback,
    pub daemon: Daemon,
    pub git: Git,
    pub phone: Phone,
    pub ui: Ui,
    pub theme: Theme,
    pub keys: Keys,
}

/// The defaults, from `defaults.toml`.
pub(crate) fn defaults_table() -> toml::Table {
    include_str!("defaults.toml")
        .parse()
        .unwrap_or_else(|e| panic!("defaults.toml does not parse: {e}"))
}

/// A section of the defaults, as its type. `defaults.toml` has every key, so no field is missing.
fn section<T: serde::de::DeserializeOwned>(path: &[&str]) -> T {
    let mut node = toml::Value::Table(defaults_table());
    for key in path {
        node = match node {
            toml::Value::Table(mut table) => table
                .remove(*key)
                .unwrap_or_else(|| toml::Value::Table(toml::Table::new())),
            other => other,
        };
    }
    node.try_into()
        .unwrap_or_else(|e| panic!("defaults.toml {}: {e}", path.join(".")))
}

macro_rules! default_from_file {
    ($($ty:ty => [$($path:literal),+];)*) => {
        $(
            impl Default for $ty {
                fn default() -> Self {
                    section(&[$($path),+])
                }
            }
        )*
    };
}

default_from_file! {
    ClaudeApprovals => ["approvals", "claude"];
    CodexApprovals => ["approvals", "codex"];
    Approvals => ["approvals"];
    PolicyConfig => ["policy"];
    Agents => ["agents"];
    Events => ["events"];
    Scrollback => ["scrollback"];
    Daemon => ["daemon"];
    Git => ["git"];
    Phone => ["phone"];
    Ui => ["ui"];
    Theme => ["theme"];
    Keys => ["keys"];
}

impl Default for Config {
    fn default() -> Self {
        toml::Value::Table(defaults_table())
            .try_into()
            .unwrap_or_else(|e| panic!("defaults.toml: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_file_has_every_key_with_its_value() {
        let config = Config::default();
        assert_eq!(config.approvals.mode, ApprovalMode::Auto);
        assert_eq!(
            config.approvals.gate_timeout.0,
            Duration::from_secs(30 * 60)
        );
        assert!(!config.approvals.claude.persist_always);
        assert_eq!(config.approvals.claude.mode, ApprovalChoice::Inherit);
        assert_eq!(config.approvals.codex.mode, ApprovalChoice::Inherit);
        assert!(config.agents.shims);
        assert_eq!(config.agents.resume_on_launch, ResumeOnLaunch::Ask);
        assert_eq!(config.agents.harness.claude.billing, Billing::Api);
        assert_eq!(
            config.events.sync_retention.0,
            Duration::from_secs(7 * 86_400)
        );
        assert_eq!(
            config.events.audit_retention.0,
            Duration::from_secs(90 * 86_400)
        );
        assert_eq!(config.scrollback.max_bytes_per_pane, 0);
        assert_eq!(config.daemon.prevent_sleep, PreventSleep::WhileAgentsWork);
        assert!(!config.phone.enabled);
        assert_eq!((config.phone.port, config.phone.https_port), (47_310, 8443));
        assert!(config.phone.keep_awake);
        assert!(config.phone.url.is_empty());
    }

    #[test]
    fn the_section_defaults_equal_the_sections_of_the_whole() {
        let config = Config::default();
        assert_eq!(config.approvals, Approvals::default());
        assert_eq!(config.events, Events::default());
    }

    #[test]
    fn a_time_has_a_number_and_a_unit() {
        for (text, ms) in [
            ("500ms", 500),
            ("90s", 90_000),
            ("30m", 1_800_000),
            ("2h", 7_200_000),
            ("7d", 604_800_000),
        ] {
            assert_eq!(Span::parse(text).expect(text).0.as_millis(), ms, "{text}");
        }
        for bad in [
            "",
            "30",
            "m",
            "1.5h",
            "-1s",
            "10x",
            "1 m ",
            "99999999999999999999s",
        ] {
            assert!(Span::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_time_prints_in_its_biggest_unit() {
        for text in ["500ms", "90s", "30m", "2h", "7d", "1d"] {
            assert_eq!(Span::parse(text).expect(text).text(), text);
        }
        assert_eq!(Span(Duration::from_secs(120)).text(), "2m");
        assert_eq!(Span(Duration::ZERO).text(), "0ms");
    }
}
