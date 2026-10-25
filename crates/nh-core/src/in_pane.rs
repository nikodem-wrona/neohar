//! The clients of the API socket that run inside a pane of this core.
//!
//! A program in a pane runs as the user, and the environment of the pane names the socket
//! (`NEOHAR_SOCKET`). So a tool of an agent can connect to `api.sock` as the CLI does, and a client
//! of the CLI may answer interactions and change the policy. The approval gate is not a sandbox, but
//! the core stops this plain path. For a method of [`refusal`], the core reads the process of the
//! peer and its parents. When a holder of a pane of this core is one of them, or the client is in
//! the session of the child of a pane, the core refuses the call. The other methods stay open, so
//! `nh config set ui.font_size 14` and `nh integration install` work in a pane.
//!
//! A program that leaves the process tree of its pane (a session of its own, then a parent that
//! ends) gets past this check. The sandbox of a harness must not reach the sockets in the runtime
//! directory of neohar.

use std::collections::HashSet;

use serde_json::Value;

use crate::runtime::Inner;

/// The keys of the configuration that a program in a pane may not set: they weaken the approvals,
/// or they open this machine to other devices. An entry names a key or a table of keys.
const GUARDED_KEYS: &[&str] = &[
    "approvals",
    "policy",
    "phone",
    "machines",
    "agents.shims",
    "events.audit_retention",
];

/// Why a client inside a pane may not make this call, or `None` when it may.
pub(crate) fn refusal(method: &str, params: &Value) -> Option<String> {
    const OUTSIDE: &str = "Run the command outside a neohar pane, or use the app";
    match method {
        // The methods that answer or release an interaction, change the rules of the policy, or
        // pair a phone with this machine.
        "interaction.answer"
        | "interaction.dismiss"
        | "policy.add"
        | "policy.remove"
        | "phone.pair_begin"
        | "phone.publish" => Some(format!(
            "a program in a neohar pane may not call {method}. {OUTSIDE}"
        )),
        "config.set" => {
            let key = params.get("key").and_then(Value::as_str).unwrap_or("");
            guarded_key(key)
                .then(|| format!("a program in a neohar pane may not set {key}. {OUTSIDE}"))
        }
        _ => None,
    }
}

/// Whether a key of `config.set` is a guarded key, is in a guarded table, or holds one.
fn guarded_key(key: &str) -> bool {
    let key = key.trim().trim_matches('.');
    key.is_empty()
        || GUARDED_KEYS.iter().any(|guarded| {
            key == *guarded
                || key.starts_with(&format!("{guarded}."))
                || guarded.starts_with(&format!("{key}."))
        })
}

/// The processes of the panes of this core.
#[derive(Debug, Default)]
pub(crate) struct Panes {
    /// The holders (`nh hold`).
    pub holders: HashSet<i32>,
    /// The children that the holders started. Each one leads a session of its own.
    pub children: HashSet<i32>,
}

impl Panes {
    /// The processes of the panes that the core has now.
    pub fn of(inner: &Inner) -> Self {
        let panes = crate::pane::lock(&inner.panes);
        let mut out = Self::default();
        for pane in panes.values() {
            out.holders.insert(pane.holder_pid());
            out.children.insert(pane.child_pid());
        }
        out.holders.retain(|pid| *pid > 1);
        out.children.retain(|pid| *pid > 1);
        out
    }
}

/// Whether the process `pid`, with these parents and this session, runs in a pane.
fn inside(pid: i32, ancestors: &[i32], session: Option<i32>, panes: &Panes) -> bool {
    let of_pane = |p: &i32| panes.holders.contains(p) || panes.children.contains(p);
    of_pane(&pid)
        || ancestors.iter().any(of_pane)
        || session.is_some_and(|sid| panes.children.contains(&sid))
}

/// Whether the process `pid` runs in a pane of the core: a holder or the child of a pane is one of
/// its parents, or the process is in the session of the child of a pane. The session finds a
/// process whose parent ended, because a process keeps its session when it moves to `launchd`.
pub(crate) fn runs_in_pane(pid: i32, panes: &Panes) -> bool {
    if panes.holders.is_empty() && panes.children.is_empty() {
        return false;
    }
    let ancestors = nh_agents::process::ancestors(pid);
    let session = nh_agents::process::session_of(pid);
    inside(pid, &ancestors, session, panes)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn panes() -> Panes {
        Panes {
            holders: [100].into(),
            children: [101].into(),
        }
    }

    #[test]
    fn a_process_below_a_holder_or_in_the_session_of_a_child_is_in_a_pane() {
        let panes = panes();
        // A tool of an agent: the agent, the shell, the child of the pane, the holder.
        assert!(inside(300, &[250, 101, 100], Some(101), &panes));
        // A program that started a session of its own is still below the holder.
        assert!(inside(300, &[250, 101, 100], Some(300), &panes));
        // A program whose parents ended keeps the session of the child of the pane.
        assert!(inside(300, &[], Some(101), &panes));
        // A terminal of another app.
        assert!(!inside(300, &[200, 50], Some(200), &panes));
        assert!(!inside(300, &[], None, &panes));
    }

    #[test]
    fn this_process_is_not_in_a_pane_of_a_core_without_panes() {
        let me = i32::try_from(std::process::id()).expect("a pid");
        assert!(!runs_in_pane(me, &Panes::default()));
        // A pane whose holder is a parent of this process: the test sees itself as inside.
        let parents = nh_agents::process::ancestors(me);
        if let Some(parent) = parents.first() {
            let panes = Panes {
                holders: [*parent].into(),
                children: HashSet::new(),
            };
            assert!(runs_in_pane(me, &panes));
        }
    }

    #[test]
    fn a_pane_may_not_answer_release_or_weaken_the_approvals() {
        for method in [
            "interaction.answer",
            "interaction.dismiss",
            "policy.add",
            "policy.remove",
            "phone.pair_begin",
            "phone.publish",
        ] {
            let why = refusal(method, &json!({})).expect(method);
            assert!(why.contains(method), "{why}");
            assert!(why.contains("outside a neohar pane"), "{why}");
        }
        for key in [
            "approvals.mode",
            "approvals.claude.persist_always",
            "approvals.gate_timeout",
            "policy.rule",
            "phone.enabled",
            "phone.url",
            "machines.studio",
            "machines.studio.ssh",
            "agents.shims",
            "events.audit_retention",
            "",
        ] {
            assert!(
                refusal("config.set", &json!({"key": key, "value": 1})).is_some(),
                "{key}"
            );
        }
    }

    #[test]
    fn a_pane_may_still_read_set_harmless_keys_and_install_the_hooks() {
        for key in [
            "ui.font_family",
            "ui.font_size",
            "theme.accent",
            "agents.resume_on_launch",
            "events.sync_retention",
            "scrollback.max_bytes_per_pane",
        ] {
            assert_eq!(
                refusal("config.set", &json!({"key": key, "value": 1})),
                None,
                "{key}"
            );
        }
        for method in [
            "integration.install",
            "interaction.list",
            "interaction.show",
            "policy.list",
            "policy.suggest",
            "config.get",
            "session.info",
            "pane.list",
        ] {
            assert_eq!(refusal(method, &json!({})), None, "{method}");
        }
    }
}
