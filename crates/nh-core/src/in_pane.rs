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
//! Input is the other path: a key typed into the dialog of a harness answers it. So a client
//! inside a pane may not type into a pane ([`input_target`]) whose run waits for an answer: an
//! open approval, question or plan review ([`waits_for_answer`]). This includes its own pane.
//! Input to a pane that waits for nothing stays open, so an agent can still prompt an idle agent.
//!
//! A program that leaves the process tree of its pane (a session of its own, then a parent that
//! ends) gets past this check. The sandbox of a harness must not reach the sockets in the runtime
//! directory of neohar.

use std::collections::HashSet;

use nh_proto::api::{
    calls::agent::Placement,
    ids::{PaneId, Ref, RunId},
    model::InteractionKind,
};
use serde_json::Value;

use crate::{ErrorCode, error::RpcError, runtime::Inner};

/// The end of each refusal.
const OUTSIDE: &str = "Run the command outside a neohar pane, or use the app";

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
    match method {
        // The methods that answer or release an interaction, change the rules of the policy, pair
        // a phone with this machine, or trust a repository: an agent that wrote the settings of a
        // repository must not be the one that trusts them.
        "interaction.answer"
        | "interaction.dismiss"
        | "policy.add"
        | "policy.remove"
        | "phone.pair_begin"
        | "phone.publish"
        | "git.trust"
        | "git.untrust" => Some(format!(
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

/// The pane or the run that a method of input types into.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Target {
    Pane(Ref<PaneId>),
    Run(Ref<RunId>),
}

/// The target of a method that types into a pane, or `None` for a method that types nothing. These
/// are the methods of input that a client of the CLI may call: text, bytes, a prompt, a reply, an
/// interrupt (a key too), and an agent that starts in an existing pane.
pub(crate) fn input_target(method: &str, params: &Value) -> Option<Target> {
    let field = |name: &str| params.get(name).cloned();
    match method {
        "pane.paste" | "pane.send_bytes" => field("pane")
            .and_then(|value| serde_json::from_value(value).ok())
            .map(Target::Pane),
        "agent.prompt" | "agent.interrupt" => field("agent")
            .and_then(|value| serde_json::from_value(value).ok())
            .map(Target::Run),
        "agent.reply" => field("run")
            .and_then(|value| serde_json::from_value(value).ok())
            .map(Target::Run),
        "agent.start" | "agent.resume" => match field("placement")
            .and_then(|value| serde_json::from_value::<Placement>(value).ok())
        {
            Some(Placement::Pane { pane }) => Some(Target::Pane(pane)),
            _ => None,
        },
        _ => None,
    }
}

/// Fails when the target of a method of input waits for an answer: its pane, or its run, has an
/// open approval, question or plan review. A target that does not exist passes, and the method
/// itself says so.
pub(crate) async fn waits_for_answer(
    inner: &Inner,
    method: &str,
    target: Target,
) -> Result<(), RpcError> {
    let waits = inner
        .actor
        .update(move |parts| {
            let runs = &parts.state.runs;
            let (pane, run) = match &target {
                Target::Pane(reference) => (parts.state.layout.resolve_pane(reference).ok(), None),
                Target::Run(reference) => {
                    let run = runs.resolve(reference);
                    let pane = run
                        .and_then(|id| runs.by_id.get(&id))
                        .and_then(|found| found.pane);
                    (pane, run)
                }
            };
            parts.state.interactions.open().iter().any(|interaction| {
                let pane_of_run = interaction
                    .run
                    .and_then(|id| runs.by_id.get(&id))
                    .and_then(|found| found.pane);
                matches!(
                    interaction.kind,
                    InteractionKind::Approval
                        | InteractionKind::Question
                        | InteractionKind::PlanReview
                ) && (run.is_some_and(|id| interaction.run == Some(id))
                    || pane
                        .is_some_and(|id| interaction.pane == Some(id) || pane_of_run == Some(id)))
            })
        })
        .await
        .map_err(|error| RpcError::internal(error.to_string()))?;
    if waits {
        return Err(RpcError::new(
            ErrorCode::PermissionDenied,
            format!(
                "a program in a neohar pane may not call {method} while the pane waits for an \
                 answer to an approval, a question or a plan. {OUTSIDE}"
            ),
        ));
    }
    Ok(())
}

/// What the core reads once about the process of a client: its parents and its session. They do
/// not change while the process lives, but the panes of the core do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Lineage {
    pub pid: i32,
    pub ancestors: Vec<i32>,
    pub session: Option<i32>,
}

impl Lineage {
    pub fn of(pid: i32) -> Self {
        Self {
            pid,
            ancestors: nh_agents::process::ancestors(pid),
            session: nh_agents::process::session_of(pid),
        }
    }

    /// Whether the process runs in one of these panes.
    pub fn in_panes(&self, panes: &Panes) -> bool {
        inside(self.pid, &self.ancestors, self.session, panes)
    }
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
#[cfg(test)]
pub(crate) fn runs_in_pane(pid: i32, panes: &Panes) -> bool {
    Lineage::of(pid).in_panes(panes)
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
            "git.trust",
            "git.untrust",
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
    fn the_methods_of_input_name_the_pane_or_the_run_that_they_type_into() {
        let pane = |text: &str| {
            Some(Target::Pane(
                serde_json::from_value(json!(text)).expect("a ref"),
            ))
        };
        let run = |text: &str| {
            Some(Target::Run(
                serde_json::from_value(json!(text)).expect("a ref"),
            ))
        };
        assert_eq!(
            input_target(
                "pane.send_bytes",
                &json!({"pane": "w1:p2", "input_id": 1, "bytes": "MQ0="})
            ),
            pane("w1:p2")
        );
        assert_eq!(
            input_target(
                "pane.paste",
                &json!({"pane": "w1:p2", "input_id": 1, "text": "1"})
            ),
            pane("w1:p2")
        );
        assert_eq!(
            input_target("agent.prompt", &json!({"agent": "a1", "text": "go"})),
            run("a1")
        );
        assert_eq!(
            input_target("agent.interrupt", &json!({"agent": "a1"})),
            run("a1")
        );
        assert_eq!(
            input_target(
                "agent.reply",
                &json!({"run": "a1", "input_id": 1, "text": "go"})
            ),
            run("a1")
        );
        // An agent that starts in an existing pane types its command there.
        assert_eq!(
            input_target(
                "agent.start",
                &json!({"harness": "claude", "placement": {"type": "pane", "pane": "w1:p2"}})
            ),
            pane("w1:p2")
        );
        assert_eq!(
            input_target(
                "agent.start",
                &json!({"harness": "claude", "placement": {"type": "new_tab", "workspace": "w1"}})
            ),
            None
        );
        for method in ["pane.read", "pane.list", "interaction.list", "config.set"] {
            assert_eq!(
                input_target(method, &json!({"pane": "w1:p2"})),
                None,
                "{method}"
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
