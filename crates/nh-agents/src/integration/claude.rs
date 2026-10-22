//! The hooks of neohar in `settings.json` of Claude Code.
//!
//! The 17 events of the table of hooks each get a group with one hook. The events that can wait
//! (`PreToolUse`, `PermissionRequest`, `Elicitation`) get a timeout of 1800 seconds, and
//! the others get the default of Claude Code.
//!
//! **No marker key.** Claude Code accepts an unknown key in a hook object (the recorded scenario
//! `marker`), but it drops the key when it saves `settings.json` itself, for example after
//! `/model`. So the hooks of neohar are told by their command, `<nh> hook claude <Event>`, as those
//! of Codex are. A hook that still has the old `"_neohar"` key has this command too, and the
//! install writes it again without the key.

use super::{
    Result,
    hooks::{self, Event, Spec},
};

pub use super::hooks::{EventState, Presence};

/// The timeout of a hook that can wait for a decision, in seconds.
const SYNC_TIMEOUT: u32 = 1800;

const fn event(name: &'static str, matcher: Option<&'static str>, sync: bool) -> Event {
    Event {
        name,
        matcher,
        timeout: if sync { Some(SYNC_TIMEOUT) } else { None },
    }
}

/// The 17 events, in the order in which the installer writes them.
pub const EVENTS: [Event; 17] = [
    event("SessionStart", Some("*"), false),
    event("UserPromptSubmit", None, false),
    event("PreToolUse", Some("*"), true),
    event("PermissionRequest", Some("*"), true),
    event("PermissionDenied", None, false),
    event("PostToolUse", Some("*"), false),
    event("PostToolUseFailure", Some("*"), false),
    event("Notification", None, false),
    event("Stop", None, false),
    event("StopFailure", Some("*"), false),
    event("SubagentStart", Some("*"), false),
    event("SubagentStop", Some("*"), false),
    event("PreCompact", Some("*"), false),
    event("PostCompact", Some("*"), false),
    event("CwdChanged", None, false),
    event("Elicitation", Some("*"), true),
    event("SessionEnd", Some("*"), false),
];

/// What the installer knows about Claude Code.
pub const SPEC: Spec = Spec {
    harness: "claude",
    events: &EVENTS,
};

/// Puts the hooks of neohar in the settings. `text` is the file, or `None` when it does not exist.
pub fn install(text: Option<&str>, bin: &str) -> Result<String> {
    hooks::install(&SPEC, text, bin)
}

/// Takes the hooks of neohar out of the settings and leaves the rest as it was.
pub fn uninstall(text: &str) -> Result<String> {
    hooks::uninstall(&SPEC, text)
}

/// Which of the 17 events have the hook of neohar.
pub fn inspect(text: Option<&str>, bin: &str) -> Result<Vec<EventState>> {
    hooks::inspect(&SPEC, text, bin)
}

#[cfg(test)]
mod tests;
