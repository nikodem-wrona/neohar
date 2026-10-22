//! The hooks of neohar in `hooks.json` of Codex.
//!
//! The file has the shape of the one of Claude Code, with no matcher. The 12 events of the table
//! each get a group with one hook. A hook that can wait (`PreToolUse`, `PermissionRequest`) gets a
//! timeout of 1800 seconds, `SessionEnd` and `Interrupt` get 3 (Codex cuts them to 3 and warns at
//! every start), and the others get 10.
//!
//! **Trust.** Codex runs a hook that is not managed only after the user has reviewed its exact
//! definition. It records the review under `[hooks.state."<file>:<event>:<group>:<hook>"]` in
//! `config.toml`. The installer never writes that table. It reads it to say which hooks are trusted
//! ([`trust`]), and the user gives the trust with `/hooks` in Codex.
//!
//! **No extra key.** No recording tests whether Codex keeps an unknown key in a hook object, and
//! Codex keeps a hash of each definition. So the hooks of neohar have no marker key. They are told
//! by their command, `<nh> hook codex <Event>`. The definitions stay byte-stable across upgrades of
//! neohar, so the trust survives them.

use super::{
    Result,
    hooks::{self, Event, EventState, Spec},
};

/// The timeout of a hook that can wait for a decision, in seconds.
const SYNC_TIMEOUT: u32 = 1800;

/// The timeout of `SessionEnd` and `Interrupt`: Codex cuts them to 3 seconds.
const SHORT_TIMEOUT: u32 = 3;

/// The timeout of the other hooks, in seconds. The shim answers in milliseconds.
const DEFAULT_TIMEOUT: u32 = 10;

const fn event(name: &'static str, timeout: u32) -> Event {
    Event {
        name,
        matcher: None,
        timeout: Some(timeout),
    }
}

/// The 12 events, in the order in which the installer writes them.
pub const EVENTS: [Event; 12] = [
    event("SessionStart", DEFAULT_TIMEOUT),
    event("UserPromptSubmit", DEFAULT_TIMEOUT),
    event("PreToolUse", SYNC_TIMEOUT),
    event("PermissionRequest", SYNC_TIMEOUT),
    event("PostToolUse", DEFAULT_TIMEOUT),
    event("Stop", DEFAULT_TIMEOUT),
    event("Interrupt", SHORT_TIMEOUT),
    event("SubagentStart", DEFAULT_TIMEOUT),
    event("SubagentStop", DEFAULT_TIMEOUT),
    event("PreCompact", DEFAULT_TIMEOUT),
    event("PostCompact", DEFAULT_TIMEOUT),
    event("SessionEnd", SHORT_TIMEOUT),
];

/// What the installer knows about Codex.
pub const SPEC: Spec = Spec {
    harness: "codex",
    events: &EVENTS,
};

/// Puts the hooks of neohar in `hooks.json`. `text` is the file, or `None` when it is not there.
pub fn install(text: Option<&str>, bin: &str) -> Result<String> {
    hooks::install(&SPEC, text, bin)
}

/// Takes the hooks of neohar out of `hooks.json` and leaves the rest as it was.
pub fn uninstall(text: &str) -> Result<String> {
    hooks::uninstall(&SPEC, text)
}

/// Which of the 12 events have the hook of neohar.
pub fn inspect(text: Option<&str>, bin: &str) -> Result<Vec<EventState>> {
    hooks::inspect(&SPEC, text, bin)
}

/// The hooks of neohar that Codex trusts, from `[hooks.state]` in `config.toml`. `hooks_path` is
/// the path of `hooks.json` as Codex writes it in the key. Only the hooks that are in the file are
/// listed. A hook is trusted when its entry has a hash. The hash of a definition that changed since
/// the review is stale, and Codex asks again at its next start. neohar cannot tell that, because
/// it does not know the algorithm of the hash.
pub fn trust(config: &str, hooks_path: &str, states: &[EventState]) -> Vec<(&'static str, bool)> {
    let table: Option<toml::Table> = toml::from_str(config).ok();
    let state = table
        .as_ref()
        .and_then(|t| t.get("hooks"))
        .and_then(|h| h.get("state"))
        .and_then(toml::Value::as_table);
    states
        .iter()
        .filter_map(|event| {
            let (group, hook) = event.place?;
            let key = format!("{hooks_path}:{}:{group}:{hook}", snake_case(event.name));
            let trusted = state
                .and_then(|state| state.get(&key))
                .and_then(|entry| entry.get("trusted_hash"))
                .and_then(toml::Value::as_str)
                .is_some_and(|hash| !hash.is_empty());
            Some((event.name, trusted))
        })
        .collect()
}

/// `PreToolUse` as `pre_tool_use`: the form that Codex writes in the keys of `[hooks.state]`.
fn snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Whether `config.toml` allows hooks: `[features] hooks` is on unless the file turns it off.
pub fn hooks_feature_on(config: &str) -> bool {
    toml::from_str::<toml::Table>(config)
        .ok()
        .and_then(|table| {
            table
                .get("features")
                .and_then(|f| f.get("hooks"))
                .and_then(toml::Value::as_bool)
        })
        .unwrap_or(true)
}

#[cfg(test)]
mod tests;
