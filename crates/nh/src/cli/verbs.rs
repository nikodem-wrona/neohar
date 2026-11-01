//! The verbs of the command line, one for each method that a person can call.
//!
//! The table below says which method a verb calls and which params are positional. The options
//! come from the schema of the params: every other property is a `--flag` with the name of the
//! property, so the command line cannot lag behind the API. A few verbs build their params in a
//! special way ([`Custom`]); they are the ones where a person wants words and not JSON.

use std::collections::HashSet;

use clap::{Arg, ArgAction, ArgMatches, Command};
use nh_proto::api::common::Bytes;
use serde_json::{Map, Value, json};

use super::output::CliError;

/// What a flag takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A switch.
    Bool,
    Num,
    Str,
    /// The flag can be given again.
    List,
    /// A JSON value: an object, or an enum with data.
    Json,
    /// Any value: JSON if it parses, else the text.
    Any,
}

/// How a verb builds its params, when the schema is not enough.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Custom {
    None,
    /// `interaction answer I42 allow`.
    Answer,
    /// `agent start claude --pane w1:p2`.
    AgentStart,
    /// `pane send-keys w1:p2 "ls" Enter`.
    SendKeys,
    /// `fs list [path]`: a relative path and a path under `~` become absolute. Without a path,
    /// the current directory.
    List,
    /// `git status`: the worktree of `--path`, made absolute, or of the current directory. With
    /// `--window`, the worktree of the Git view of that window.
    Here,
    /// `workflow execute review`: the folder of `--folder`, made absolute, or the current
    /// directory. `--wait` waits for the end of the execution.
    Folder,
}

pub struct Verb {
    pub noun: &'static str,
    pub name: &'static str,
    pub method: &'static str,
    pub about: &'static str,
    /// The params that are positional, in order. A name that ends in `...` takes the rest.
    pub pos: &'static [&'static str],
    /// Options that the schema does not show (a params type with a flattened enum).
    pub extra: &'static [(&'static str, Kind)],
    pub custom: Custom,
}

const fn verb(
    noun: &'static str,
    name: &'static str,
    method: &'static str,
    about: &'static str,
    pos: &'static [&'static str],
) -> Verb {
    Verb {
        noun,
        name,
        method,
        about,
        pos,
        extra: &[],
        custom: Custom::None,
    }
}

const fn special(
    noun: &'static str,
    name: &'static str,
    method: &'static str,
    about: &'static str,
    pos: &'static [&'static str],
    custom: Custom,
) -> Verb {
    Verb {
        custom,
        ..verb(noun, name, method, about, pos)
    }
}

/// The nouns, with a line for the help.
pub const NOUNS: &[(&str, &str)] = &[
    ("session", "The session of the core"),
    ("machine", "This machine and the machines it connects to"),
    ("workspace", "Workspaces"),
    ("group", "Groups of workspaces"),
    ("tab", "Tabs"),
    ("layout", "The layout of the panes of a tab"),
    ("pane", "Panes"),
    ("agent", "Agents: the runs of Claude Code and Codex"),
    (
        "harness",
        "Claude Code and Codex on this machine: installed, version, login",
    ),
    (
        "interaction",
        "What agents ask: approvals, questions, plans",
    ),
    ("phone", "Phones that supervise this machine"),
    ("fs", "The file system of the machine"),
    (
        "git",
        "Git repositories, worktrees, branches and history, as the Git view shows them",
    ),
    (
        "workflow",
        "Workflows of agents and their executions, as the Workflows view shows them",
    ),
    (
        "hermes",
        "Chats with Hermes Agent, as the Hermes view shows them",
    ),
    ("config", "The configuration"),
    ("policy", "Rules that decide approvals"),
    ("integration", "The hooks of the harnesses"),
    ("jobs", "Long commands"),
];

pub const VERBS: &[Verb] = &[
    // --- the session. `session info` is a command of `args.rs`.
    verb(
        "session",
        "stop-everything",
        "session.stop_everything",
        "Stop every program of every pane, as \"Quit and stop everything\" does",
        &[],
    ),
    verb(
        "session",
        "shutdown",
        "session.shutdown",
        "End the core of a daemon. The panes keep running",
        &[],
    ),
    verb("machine", "info", "machine.info", "Show the machine", &[]),
    verb(
        "machine",
        "list",
        "machine.list",
        "List this machine and the machines of config.toml with their links",
        &[],
    ),
    verb(
        "machine",
        "connect",
        "machine.connect",
        "Open the link to a machine of config.toml",
        &["machine"],
    ),
    verb(
        "machine",
        "disconnect",
        "machine.disconnect",
        "End the link to a machine. Its programs keep running",
        &["machine"],
    ),
    verb(
        "machine",
        "open-window",
        "machine.open_window",
        "Open a window of a machine, and connect it first",
        &["machine"],
    ),
    // --- workspaces and groups
    verb(
        "workspace",
        "create",
        "workspace.create",
        "Make a workspace with one tab",
        &["name"],
    ),
    verb(
        "workspace",
        "list",
        "workspace.list",
        "List the workspaces",
        &[],
    ),
    verb(
        "workspace",
        "show",
        "workspace.show",
        "Show a workspace with its tabs",
        &["workspace"],
    ),
    verb(
        "workspace",
        "rename",
        "workspace.rename",
        "Rename a workspace",
        &["workspace", "name"],
    ),
    verb(
        "workspace",
        "close",
        "workspace.close",
        "Close a workspace",
        &["workspace"],
    ),
    verb(
        "workspace",
        "move",
        "workspace.move",
        "Move a workspace to a group or before another",
        &["workspace"],
    ),
    verb("group", "create", "group.create", "Make a group", &["name"]),
    verb("group", "list", "group.list", "List the groups", &[]),
    verb("group", "show", "group.show", "Show a group", &["group"]),
    verb(
        "group",
        "rename",
        "group.rename",
        "Rename a group",
        &["group", "name"],
    ),
    verb("group", "close", "group.close", "Close a group", &["group"]),
    verb("group", "move", "group.move", "Move a group", &["group"]),
    // --- tabs and the layout
    verb(
        "tab",
        "create",
        "tab.create",
        "Make a tab in a workspace",
        &["workspace"],
    ),
    verb(
        "tab",
        "rename",
        "tab.rename",
        "Rename a tab",
        &["tab", "name"],
    ),
    verb("tab", "close", "tab.close", "Close a tab", &["tab"]),
    verb("tab", "move", "tab.move", "Move a tab", &["tab"]),
    verb(
        "tab",
        "select",
        "tab.select",
        "Show a tab in a window",
        &["tab"],
    ),
    verb(
        "layout",
        "set-ratios",
        "layout.set_ratios",
        "Set the shares of a split",
        &["tab", "path"],
    ),
    verb(
        "layout",
        "equalize",
        "layout.equalize",
        "Give the panes of a split equal shares",
        &["tab"],
    ),
    verb(
        "layout",
        "zoom",
        "layout.zoom",
        "Zoom a pane, or show all panes",
        &["tab"],
    ),
    // --- panes
    verb("pane", "list", "pane.list", "List the panes", &[]),
    verb("pane", "show", "pane.show", "Show a pane", &["pane"]),
    verb(
        "pane",
        "split",
        "pane.split",
        "Split a pane",
        &["pane", "direction"],
    ),
    verb("pane", "close", "pane.close", "Close a pane", &["pane"]),
    verb(
        "pane",
        "move",
        "pane.move",
        "Move a pane to a tab",
        &["pane", "tab"],
    ),
    verb(
        "pane",
        "float",
        "pane.float",
        "Float a pane over the tab",
        &["pane"],
    ),
    verb(
        "pane",
        "unfloat",
        "pane.unfloat",
        "Put a floating pane back in the layout",
        &["pane"],
    ),
    verb(
        "pane",
        "popout",
        "pane.popout",
        "Move a pane to a window of its own",
        &["pane"],
    ),
    verb(
        "pane",
        "restart",
        "pane.restart",
        "Start the program of a pane again",
        &["pane"],
    ),
    verb(
        "pane",
        "paste",
        "pane.paste",
        "Paste text into a pane",
        &["pane", "text"],
    ),
    special(
        "pane",
        "send-keys",
        "pane.send_bytes",
        "Type keys into a pane: text, and names such as Enter, Escape, C-c",
        &["pane", "keys..."],
        Custom::SendKeys,
    ),
    Verb {
        extra: &[("delta", Kind::Num), ("top_line", Kind::Num)],
        ..verb(
            "pane",
            "scroll",
            "pane.scroll",
            "Scroll the viewport of a pane",
            &["pane"],
        )
    },
    verb(
        "pane",
        "copy",
        "pane.copy",
        "Copy the selection of a pane",
        &["pane"],
    ),
    verb(
        "pane",
        "read",
        "pane.read",
        "Print the text of a pane",
        &["pane"],
    ),
    // --- agents
    verb("agent", "list", "agent.list", "List the agents", &[]),
    verb("agent", "show", "agent.show", "Show an agent", &["agent"]),
    special(
        "agent",
        "start",
        "agent.start",
        "Start an agent in a new pane, a new tab or an idle shell",
        &["harness"],
        Custom::AgentStart,
    ),
    verb(
        "agent",
        "choices",
        "agent.choices",
        "List the models and the effort levels that a start can choose",
        &["harness"],
    ),
    verb(
        "agent",
        "stop",
        "agent.stop",
        "Ask an agent to end",
        &["agent"],
    ),
    verb(
        "agent",
        "interrupt",
        "agent.interrupt",
        "Interrupt the turn of an agent",
        &["agent"],
    ),
    verb(
        "agent",
        "prompt",
        "agent.prompt",
        "Type a prompt into an agent",
        &["agent", "text"],
    ),
    verb(
        "agent",
        "reply",
        "agent.reply",
        "Send the next prompt to an agent whose turn ended",
        &["run", "text"],
    ),
    verb(
        "agent",
        "resume",
        "agent.resume",
        "Start an agent that ended with its session",
        &["agent"],
    ),
    verb(
        "agent",
        "wait",
        "agent.wait",
        "Wait until an agent is idle, has ended, or needs you",
        &["agent", "until"],
    ),
    verb(
        "agent",
        "link",
        "agent.link",
        "Bind a pane to its agent",
        &["pane"],
    ),
    verb(
        "agent",
        "unlink",
        "agent.unlink",
        "Break the link of an agent",
        &["agent"],
    ),
    verb(
        "agent",
        "move",
        "agent.move",
        "Move an agent to another place on the board",
        &["agent", "index"],
    ),
    // --- harnesses
    verb(
        "harness",
        "refresh",
        "harness.refresh",
        "Find the harnesses again and ask each one for its login state",
        &["harness"],
    ),
    // --- interactions
    verb(
        "interaction",
        "list",
        "interaction.list",
        "List the interactions",
        &[],
    ),
    verb(
        "interaction",
        "show",
        "interaction.show",
        "Show an interaction",
        &["interaction"],
    ),
    special(
        "interaction",
        "answer",
        "interaction.answer",
        "Answer an interaction: allow, allow-always or deny for an approval",
        &["interaction", "choice"],
        Custom::Answer,
    ),
    verb(
        "interaction",
        "dismiss",
        "interaction.dismiss",
        "Close an interaction without an answer",
        &["interaction"],
    ),
    verb(
        "interaction",
        "next",
        "interaction.next",
        "Show the interaction that is next",
        &[],
    ),
    verb(
        "interaction",
        "reveal",
        "interaction.reveal",
        "Show an interaction in a window",
        &["interaction"],
    ),
    verb(
        "phone",
        "pair-begin",
        "phone.pair_begin",
        "Offer a pairing to a phone: print the URL of the QR code and the short code",
        &[],
    ),
    verb(
        "phone",
        "pair-cancel",
        "phone.pair_cancel",
        "End the offer of a pairing",
        &[],
    ),
    verb(
        "phone",
        "remove",
        "phone.remove",
        "Remove a paired phone and close its connections",
        &["device"],
    ),
    verb(
        "phone",
        "rename",
        "phone.rename",
        "Change the name of a paired phone",
        &["device", "name"],
    ),
    verb(
        "phone",
        "publish",
        "phone.publish",
        "Put the phone listener on the tailnet with tailscale serve (HTTPS, never a Funnel)",
        &[],
    ),
    verb(
        "phone",
        "unpublish",
        "phone.unpublish",
        "Remove the mapping of tailscale serve for the phone listener",
        &[],
    ),
    verb(
        "phone",
        "push-test",
        "phone.push_test",
        "Send the push \"Test notification\" to a paired phone",
        &["device"],
    ),
    special(
        "fs",
        "list",
        "fs.list",
        "List the folders and files in a folder of the machine, or in the current directory",
        &["path"],
        Custom::List,
    ),
    // --- workflows
    verb(
        "workflow",
        "list",
        "workflow.list",
        "List the workflow files of ~/.config/neohar/workflows",
        &[],
    ),
    special(
        "workflow",
        "execute",
        "workflow.execute",
        "Execute a workflow in a folder, or in the current directory. --wait waits for the end",
        &["workflow"],
        Custom::Folder,
    ),
    verb(
        "workflow",
        "executions",
        "workflow.executions",
        "List the executions of the workflows, the newest first",
        &[],
    ),
    verb(
        "workflow",
        "show",
        "workflow.execution",
        "Show an execution: the state, the verdict and the report of each node",
        &["execution"],
    ),
    verb(
        "workflow",
        "report",
        "workflow.report",
        "Show a report of an execution: report.md, or the report of one node with --node",
        &["execution"],
    ),
    verb(
        "workflow",
        "retry",
        "workflow.retry",
        "Start a node of an execution that failed again",
        &["execution", "node"],
    ),
    verb(
        "workflow",
        "skip",
        "workflow.skip",
        "Skip a node of an execution that failed or waits; the nodes after it go on",
        &["execution", "node"],
    ),
    verb(
        "workflow",
        "cancel",
        "workflow.cancel",
        "Cancel an execution and stop its agents",
        &["execution"],
    ),
    // --- hermes
    verb(
        "hermes",
        "check",
        "hermes.check",
        "Check that Hermes Agent runs here, and read its approvals.mode",
        &[],
    ),
    verb(
        "hermes",
        "start",
        "hermes.start",
        "Start a chat with Hermes in a folder (an absolute path)",
        &["cwd"],
    ),
    verb(
        "hermes",
        "prompt",
        "hermes.prompt",
        "Send a prompt to a chat; during a turn it waits for the end of the turn",
        &["chat", "text"],
    ),
    verb(
        "hermes",
        "cancel",
        "hermes.cancel",
        "Stop the turn of a chat",
        &["chat"],
    ),
    verb(
        "hermes",
        "answer",
        "hermes.answer",
        "Answer an approval of a chat with one of its options",
        &["chat", "request", "option"],
    ),
    verb(
        "hermes",
        "set",
        "hermes.set",
        "Change the model or the mode of a chat",
        &["chat"],
    ),
    verb(
        "hermes",
        "retry",
        "hermes.retry",
        "Start hermes acp again for a chat that is disconnected",
        &["chat"],
    ),
    verb(
        "hermes",
        "close",
        "hermes.close",
        "Close a chat; the session stays in Hermes",
        &["chat"],
    ),
    // --- git
    verb(
        "git",
        "repos",
        "git.repos",
        "List the git repositories that neohar knows: of the workspaces, the panes and the ones you added",
        &[],
    ),
    special(
        "git",
        "add",
        "git.repo_add",
        "Add the repository of a folder, or of the current directory, to the Repository menu",
        &["path"],
        Custom::List,
    ),
    special(
        "git",
        "trust",
        "git.trust",
        "Trust the repository of a folder, or of the current directory: neohar runs git in it only after you trust it",
        &["path"],
        Custom::List,
    ),
    special(
        "git",
        "untrust",
        "git.untrust",
        "Take back the trust of the repository of a folder, or of the current directory",
        &["path"],
        Custom::List,
    ),
    special(
        "git",
        "status",
        "git.status",
        "Show the branch, its upstream, the agents and the changed files of the worktree of the current directory",
        &[],
        Custom::Here,
    ),
    special(
        "git",
        "worktrees",
        "git.worktrees",
        "List the worktrees of the repository with their branches and agents",
        &[],
        Custom::Here,
    ),
    special(
        "git",
        "branches",
        "git.branches",
        "List the local and remote branches: the current one, the recent ones, then the others",
        &[],
        Custom::Here,
    ),
    special(
        "git",
        "log",
        "git.log",
        "List the commits of HEAD, the newest first",
        &[],
        Custom::Here,
    ),
    special(
        "git",
        "show",
        "git.show",
        "Show a commit with its message and its files",
        &["commit"],
        Custom::Here,
    ),
    special(
        "git",
        "diff",
        "git.diff",
        "Show the diff of a changed file, or of a file of a commit with --commit",
        &["file"],
        Custom::Here,
    ),
    special(
        "git",
        "switch",
        "git.switch",
        "Switch the worktree to a branch. It asks first when an agent works in the worktree: --force switches anyway",
        &["branch"],
        Custom::Here,
    ),
    special(
        "git",
        "unstash",
        "git.unstash",
        "Put the changes of a stash that a switch left back into the worktree",
        &["stash"],
        Custom::Here,
    ),
    special(
        "git",
        "fetch",
        "git.fetch",
        "Fetch the remote of the branch, as a job",
        &[],
        Custom::Here,
    ),
    special(
        "git",
        "pull",
        "git.pull",
        "Move the branch to its upstream with a fast-forward only, as a job",
        &[],
        Custom::Here,
    ),
    // --- configuration and policy
    verb(
        "config",
        "get",
        "config.get",
        "Show the settings",
        &["section"],
    ),
    verb(
        "config",
        "set",
        "config.set",
        "Set a value in config.toml",
        &["key", "value"],
    ),
    verb(
        "config",
        "reload",
        "config.reload",
        "Read config.toml again",
        &[],
    ),
    verb("policy", "list", "policy.list", "List the rules", &[]),
    verb(
        "policy",
        "add",
        "policy.add",
        "Add a rule, as JSON",
        &["rule"],
    ),
    verb(
        "policy",
        "remove",
        "policy.remove",
        "Remove a rule",
        &["id"],
    ),
    verb(
        "policy",
        "suggest",
        "policy.suggest",
        "Suggest rules from the answers so far",
        &[],
    ),
    verb(
        "integration",
        "list",
        "integration.list",
        "List the integrations",
        &[],
    ),
    verb(
        "integration",
        "install",
        "integration.install",
        "Install the hooks of a harness",
        &["target"],
    ),
    verb(
        "integration",
        "status",
        "integration.status",
        "Show what is installed",
        &["target"],
    ),
    verb(
        "integration",
        "uninstall",
        "integration.uninstall",
        "Remove the hooks of a harness",
        &["target"],
    ),
    verb(
        "integration",
        "doctor",
        "integration.doctor",
        "Check a harness with a live round trip",
        &["target"],
    ),
    verb(
        "integration",
        "capabilities",
        "integration.capabilities",
        "Show what each harness version can do",
        &[],
    ),
    verb("jobs", "cancel", "jobs.cancel", "Cancel a job", &["job"]),
];

/// The methods that the command line reaches without a verb of the table: the commands that
/// `args.rs` builds itself, and `askpass.ask`, which only `nh askpass` calls. The coverage
/// test reads the list.
#[cfg(test)]
pub const BUILT_IN: &[&str] = &[
    "session.info",
    "askpass.ask",
    "doctor.run",
    "debug.bundle",
    "events.subscribe",
    "events.unsubscribe",
    "projection.subscribe",
    "projection.unsubscribe",
    "projection.versions",
];

/// The methods of the table and the built-in commands.
#[cfg(test)]
pub fn covered() -> HashSet<&'static str> {
    VERBS
        .iter()
        .map(|v| v.method)
        .chain(BUILT_IN.iter().copied())
        .collect()
}

// --- the schema ------------------------------------------------------------------------------

/// One property of the params of a method.
#[derive(Clone, Debug)]
pub struct Prop {
    pub key: String,
    pub kind: Kind,
    pub required: bool,
    pub help: String,
    /// The default of a switch is true: the flag turns it off.
    pub default_true: bool,
}

fn resolve<'a>(doc: &'a Value, schema: &'a Value) -> &'a Value {
    let mut current = schema;
    for _ in 0..8 {
        let Some(reference) = current.get("$ref").and_then(Value::as_str) else {
            break;
        };
        let name = reference.rsplit('/').next().unwrap_or("");
        match doc["$defs"].get(name) {
            Some(target) => current = target,
            None => break,
        }
    }
    current
}

fn types_of(schema: &Value) -> Vec<&str> {
    match schema.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(list)) => list.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

/// What a property of a schema takes.
pub fn kind_of(doc: &Value, schema: &Value) -> Kind {
    let schema = resolve(doc, schema);
    if let Some(options) = schema.get("anyOf").and_then(Value::as_array) {
        let first = options
            .iter()
            .find(|o| types_of(resolve(doc, o)) != ["null"] && !o.is_null());
        return first.map_or(Kind::Any, |o| kind_of(doc, o));
    }
    if schema.get("oneOf").is_some() {
        // An enum of words is a string. An enum with data is JSON.
        let all_words = schema["oneOf"].as_array().is_some_and(|options| {
            options
                .iter()
                .all(|o| o.get("const").is_some() && o.get("properties").is_none())
        });
        return if all_words { Kind::Str } else { Kind::Json };
    }
    let types = types_of(schema);
    if types.contains(&"boolean") {
        Kind::Bool
    } else if types.contains(&"integer") || types.contains(&"number") {
        Kind::Num
    } else if types.contains(&"string") {
        Kind::Str
    } else if types.contains(&"array") {
        let items = schema.get("items").map(|i| resolve(doc, i));
        if items.is_some_and(|i| types_of(i).contains(&"string")) {
            Kind::List
        } else {
            Kind::Json
        }
    } else if types.contains(&"object") {
        Kind::Json
    } else {
        Kind::Any
    }
}

/// The properties of the params of a method, in the order of the schema.
pub fn props(doc: &Value, method: &str) -> Vec<Prop> {
    let Some(entry) = doc["methods"]
        .as_array()
        .and_then(|list| list.iter().find(|m| m["name"] == method))
    else {
        return Vec::new();
    };
    let params = resolve(doc, &entry["params"]);
    let required: Vec<&str> = params["required"]
        .as_array()
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let Some(properties) = params["properties"].as_object() else {
        return Vec::new();
    };
    properties
        .iter()
        .map(|(key, schema)| Prop {
            key: key.clone(),
            kind: kind_of(doc, schema),
            required: required.contains(&key.as_str()),
            help: schema
                .get("description")
                .and_then(Value::as_str)
                .or_else(|| {
                    resolve(doc, schema)
                        .get("description")
                        .and_then(Value::as_str)
                })
                .unwrap_or("")
                .lines()
                .next()
                .unwrap_or("")
                .to_owned(),
            default_true: schema.get("default") == Some(&Value::Bool(true)),
        })
        .collect()
}

fn kebab(key: &str) -> String {
    key.replace('_', "-")
}

/// A string that lives as long as the program. The command line is built once per run.
fn leak(text: impl Into<String>) -> &'static str {
    Box::leak(text.into().into_boxed_str())
}

/// Flags that a verb adds by hand.
fn sugar(custom: Custom) -> Vec<Arg> {
    let text =
        |name: &'static str, help: &'static str| Arg::new(name).long(kebab_static(name)).help(help);
    match custom {
        Custom::None | Custom::SendKeys => Vec::new(),
        Custom::Answer => vec![
            text("message", "The text for a deny: the model sees it"),
            Arg::new("stop")
                .long("stop")
                .action(ArgAction::SetTrue)
                .help("With deny: the turn ends"),
            Arg::new("persist")
                .long("persist")
                .action(ArgAction::SetTrue)
                .help("With allow-always: write the rule to the settings of the harness"),
            text(
                "decision",
                "The whole decision as JSON, for a question or a plan review",
            ),
        ],
        Custom::AgentStart => vec![
            text("pane", "Start in this pane, which must be an idle shell"),
            text("split", "Start in a new pane beside this pane"),
            text(
                "direction",
                "With --split: left, right, up or down (default: right)",
            ),
            text("new-tab", "Start in a new tab of this workspace"),
        ],
        Custom::List | Custom::Here => Vec::new(),
        Custom::Folder => vec![
            Arg::new("wait")
                .long("wait")
                .action(ArgAction::SetTrue)
                .help("Wait for the end of the execution, then print the path of report.md"),
        ],
    }
}

fn kebab_static(name: &'static str) -> &'static str {
    if name.contains('_') {
        leak(kebab(name))
    } else {
        name
    }
}

/// The clap command of a verb, with the options from the schema.
fn command_of(doc: &Value, verb: &Verb) -> Command {
    let all = props(doc, verb.method);
    let mut command = Command::new(verb.name).about(verb.about);
    let mut sugar_keys: HashSet<&str> = HashSet::new();
    match verb.custom {
        Custom::Answer => sugar_keys.extend(["decision", "via"]),
        Custom::AgentStart => sugar_keys.extend(["placement"]),
        Custom::SendKeys => sugar_keys.extend(["bytes", "input_id"]),
        Custom::List | Custom::Here | Custom::Folder | Custom::None => {}
    }
    let positional = |key: &str| verb.pos.iter().any(|p| p.trim_end_matches("...") == key);
    // Positionals, in the order of the table.
    for key in verb.pos {
        let rest = key.ends_with("...");
        let name = key.trim_end_matches("...");
        let required = all
            .iter()
            .find(|p| p.key == name)
            .is_some_and(|p| p.required)
            || (verb.custom == Custom::Answer && name == "choice")
            || (verb.custom == Custom::SendKeys && name == "keys");
        let help = all
            .iter()
            .find(|p| p.key == name)
            .map_or("", |p| p.help.as_str());
        let mut arg = Arg::new(leak(name))
            .value_name(leak(name.to_uppercase()))
            .help(leak(help));
        if verb.custom == Custom::AgentStart {
            // `harness` is what the person types; the rest comes from flags.
            arg = arg.required(name == "harness");
        } else if verb.custom == Custom::List {
            // Without a path, the current directory.
            arg = arg.required(false);
        } else {
            arg = arg.required(required);
        }
        if rest {
            arg = arg.num_args(1..).action(ArgAction::Append);
        }
        command = command.arg(arg);
    }
    // Options: every other property.
    for prop in &all {
        // The option `--machine` is the global one: it sends the whole command to a
        // machine. The param `machine` of `workspace.create` would be the same name with
        // another meaning.
        if positional(&prop.key) || sugar_keys.contains(prop.key.as_str()) || prop.key == "machine"
        {
            continue;
        }
        let long = kebab(&prop.key);
        let mut arg = Arg::new(leak(prop.key.clone())).help(leak(prop.help.clone()));
        arg = match prop.kind {
            Kind::Bool if prop.default_true => arg
                .long(leak(format!("no-{long}")))
                .action(ArgAction::SetTrue),
            Kind::Bool => arg.long(leak(long)).action(ArgAction::SetTrue),
            Kind::List => arg
                .long(leak(long))
                .action(ArgAction::Append)
                .value_name("VALUE"),
            Kind::Num => arg
                .long(leak(long))
                .value_name("NUMBER")
                .allow_negative_numbers(true),
            Kind::Json | Kind::Any => arg.long(leak(long)).value_name("JSON"),
            Kind::Str => arg.long(leak(long)).value_name("TEXT"),
        };
        // A required property that is not positional is a flag that must be there. The sugar of
        // a verb may stand in for it, so the check happens when the params are built.
        command = command.arg(arg);
    }
    for (key, kind) in verb.extra {
        let arg = Arg::new(*key).long(kebab_static(key));
        command = command.arg(match kind {
            Kind::Bool => arg.action(ArgAction::SetTrue),
            _ => arg.value_name("NUMBER").allow_negative_numbers(true),
        });
    }
    for arg in sugar(verb.custom) {
        command = command.arg(arg);
    }
    command
}

/// The commands of the nouns: `nh workspace …`, `nh pane …`, and the rest.
pub fn nouns(doc: &Value) -> Vec<Command> {
    NOUNS
        .iter()
        .map(|(noun, about)| {
            let mut command = Command::new(*noun)
                .about(*about)
                .subcommand_required(true)
                .arg_required_else_help(true);
            for verb in VERBS.iter().filter(|v| v.noun == *noun) {
                command = command.subcommand(command_of(doc, verb));
            }
            command
        })
        .collect()
}

pub fn is_noun(name: &str) -> bool {
    NOUNS.iter().any(|(noun, _)| *noun == name)
}

// --- the params ------------------------------------------------------------------------------

fn usage(message: impl Into<String>) -> CliError {
    CliError::Usage(message.into())
}

/// A value of a flag as JSON.
fn convert(kind: Kind, key: &str, text: &str) -> Result<Value, CliError> {
    match kind {
        Kind::Str => Ok(Value::String(text.to_owned())),
        Kind::Num => text
            .parse::<i64>()
            .map(Value::from)
            .or_else(|_| text.parse::<u64>().map(Value::from))
            .or_else(|_| {
                text.parse::<f64>()
                    .ok()
                    .and_then(serde_json::Number::from_f64)
                    .map(Value::Number)
                    .ok_or(())
            })
            .map_err(|()| usage(format!("--{} needs a number, not {text:?}", kebab(key)))),
        Kind::Json => serde_json::from_str(text)
            .map_err(|e| usage(format!("--{} is not JSON: {e}", kebab(key)))),
        Kind::Any => {
            Ok(serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned())))
        }
        Kind::Bool | Kind::List => Ok(Value::String(text.to_owned())),
    }
}

/// The time in microseconds, for the id of an input that the CLI sends. Two commands do not share
/// one, and the ids of a pane grow.
fn input_id() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| u64::try_from(d.as_micros()).unwrap_or(1))
}

/// The bytes that a name of a key stands for. `None` when the word is text.
pub fn key_bytes(name: &str) -> Option<Vec<u8>> {
    let fixed: &[(&str, &[u8])] = &[
        ("Enter", b"\r"),
        ("Return", b"\r"),
        ("Tab", b"\t"),
        ("Escape", b"\x1b"),
        ("Esc", b"\x1b"),
        ("Space", b" "),
        ("BSpace", b"\x7f"),
        ("Backspace", b"\x7f"),
        ("Delete", b"\x1b[3~"),
        ("Up", b"\x1b[A"),
        ("Down", b"\x1b[B"),
        ("Right", b"\x1b[C"),
        ("Left", b"\x1b[D"),
        ("Home", b"\x1b[H"),
        ("End", b"\x1b[F"),
        ("PageUp", b"\x1b[5~"),
        ("PageDown", b"\x1b[6~"),
    ];
    if let Some((_, bytes)) = fixed
        .iter()
        .find(|(word, _)| word.eq_ignore_ascii_case(name))
    {
        return Some(bytes.to_vec());
    }
    let mut parts = name.splitn(2, '-');
    let (modifier, rest) = (parts.next()?, parts.next()?);
    let mut chars = rest.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    match modifier {
        "C" | "c" if c.is_ascii_alphabetic() => Some(vec![(c.to_ascii_uppercase() as u8) & 0x1f]),
        "C" | "c" if c == '[' => Some(vec![0x1b]),
        "M" | "m" => {
            let mut bytes = vec![0x1b];
            bytes.extend(c.to_string().into_bytes());
            Some(bytes)
        }
        _ => None,
    }
}

/// The params of a verb from what the person typed.
pub fn params(
    doc: &Value,
    verb: &Verb,
    matches: &ArgMatches,
    cwd: &std::path::Path,
) -> Result<Value, CliError> {
    let all = props(doc, verb.method);
    let mut map = Map::new();
    let given = |key: &str| matches.try_get_one::<String>(key).ok().flatten().cloned();
    // Positionals.
    for key in verb.pos {
        let name = key.trim_end_matches("...");
        if verb.custom != Custom::None && !all.iter().any(|p| p.key == name) {
            continue;
        }
        let Some(prop) = all.iter().find(|p| p.key == name) else {
            continue;
        };
        if key.ends_with("...") {
            if let Ok(Some(values)) = matches.try_get_many::<String>(name) {
                map.insert(name.to_owned(), json!(values.cloned().collect::<Vec<_>>()));
            }
        } else if let Some(text) = given(name) {
            map.insert(name.to_owned(), convert(prop.kind, name, &text)?);
        }
    }
    // Options from the schema.
    for prop in &all {
        if verb
            .pos
            .iter()
            .any(|p| p.trim_end_matches("...") == prop.key)
        {
            continue;
        }
        match prop.kind {
            Kind::Bool => {
                if matches.try_get_one::<bool>(&prop.key).ok().flatten() == Some(&true) {
                    map.insert(prop.key.clone(), Value::Bool(!prop.default_true));
                }
            }
            Kind::List => {
                if let Ok(Some(values)) = matches.try_get_many::<String>(&prop.key) {
                    map.insert(prop.key.clone(), json!(values.cloned().collect::<Vec<_>>()));
                }
            }
            kind => {
                if let Some(text) = given(&prop.key) {
                    map.insert(prop.key.clone(), convert(kind, &prop.key, &text)?);
                }
            }
        }
    }
    for (key, kind) in verb.extra {
        if let Some(text) = given(key) {
            map.insert((*key).to_owned(), convert(*kind, key, &text)?);
        }
    }
    match verb.custom {
        Custom::None => {}
        Custom::Answer => answer(&mut map, matches)?,
        Custom::AgentStart => agent_start(&mut map, matches)?,
        Custom::SendKeys => send_keys(&mut map, matches)?,
        Custom::List => {
            let path =
                given("path").map_or_else(|| cwd.to_path_buf(), |text| absolute_of(&text, cwd));
            map.insert("path".to_owned(), Value::String(path.display().to_string()));
        }
        Custom::Folder => {
            let folder =
                given("folder").map_or_else(|| cwd.to_path_buf(), |text| absolute_of(&text, cwd));
            map.insert(
                "folder".to_owned(),
                Value::String(folder.display().to_string()),
            );
        }
        Custom::Here => {
            if !map.contains_key("window") {
                let path =
                    given("path").map_or_else(|| cwd.to_path_buf(), |text| absolute_of(&text, cwd));
                map.insert("path".to_owned(), Value::String(path.display().to_string()));
            }
        }
    }
    if all.iter().any(|p| p.key == "input_id") && !map.contains_key("input_id") {
        map.insert("input_id".to_owned(), json!(input_id()));
    }
    // A property that must be there and is not: tell which flag.
    for prop in &all {
        if prop.required && !map.contains_key(&prop.key) {
            return Err(usage(format!(
                "the option --{} is needed",
                kebab(&prop.key)
            )));
        }
    }
    Ok(Value::Object(map))
}

/// A path for the core, which needs it absolute: a relative path starts in the current directory,
/// and `~` is the home folder of the person.
fn absolute_of(text: &str, cwd: &std::path::Path) -> std::path::PathBuf {
    let home = || std::env::var_os("HOME").map(std::path::PathBuf::from);
    if text == "~" {
        return home().unwrap_or_else(|| cwd.to_path_buf());
    }
    if let (Some(rest), Some(home)) = (text.strip_prefix("~/"), home()) {
        return home.join(rest);
    }
    cwd.join(text)
}

fn answer(map: &mut Map<String, Value>, matches: &ArgMatches) -> Result<(), CliError> {
    map.insert("via".to_owned(), json!("cli"));
    if let Ok(Some(text)) = matches.try_get_one::<String>("decision") {
        let decision: Value = serde_json::from_str(text)
            .map_err(|e| usage(format!("--decision is not JSON: {e}")))?;
        map.insert("decision".to_owned(), decision);
        return Ok(());
    }
    let choice = matches
        .get_one::<String>("choice")
        .map_or("", String::as_str)
        .to_ascii_lowercase();
    let message = matches.get_one::<String>("message").cloned();
    let flag = |name: &str| matches.get_flag(name);
    let mut decision = Map::new();
    decision.insert("kind".to_owned(), json!("approval"));
    match choice.as_str() {
        "allow" | "yes" | "approve" => {
            decision.insert("behavior".to_owned(), json!("allow"));
        }
        "allow-always" | "always" => {
            decision.insert("behavior".to_owned(), json!("allow"));
            decision.insert("always".to_owned(), json!(true));
            if flag("persist") {
                decision.insert("persist".to_owned(), json!(true));
            }
        }
        "deny" | "no" | "reject" => {
            decision.insert("behavior".to_owned(), json!("deny"));
            if flag("stop") {
                decision.insert("stop".to_owned(), json!(true));
            }
        }
        other => {
            return Err(usage(format!(
                "the choice {other:?} is not allow, allow-always or deny; use --decision for a question or a plan"
            )));
        }
    }
    if let Some(message) = message {
        decision.insert("message".to_owned(), Value::String(message));
    }
    map.insert("decision".to_owned(), Value::Object(decision));
    Ok(())
}

fn agent_start(map: &mut Map<String, Value>, matches: &ArgMatches) -> Result<(), CliError> {
    if let Some(harness) = matches.get_one::<String>("harness") {
        map.insert("harness".to_owned(), json!(harness));
    }
    let get = |name: &str| matches.get_one::<String>(name).cloned();
    let direction = get("direction").unwrap_or_else(|| "right".to_owned());
    let placement = match (get("pane"), get("split"), get("new-tab")) {
        (Some(pane), None, None) => json!({"type": "pane", "pane": pane}),
        (None, Some(pane), None) => json!({"type": "split", "pane": pane, "direction": direction}),
        (None, None, Some(workspace)) => json!({"type": "new_tab", "workspace": workspace}),
        (None, None, None) => {
            return Err(usage("say where: --pane, --split or --new-tab"));
        }
        _ => return Err(usage("give only one of --pane, --split and --new-tab")),
    };
    map.insert("placement".to_owned(), placement);
    Ok(())
}

fn send_keys(map: &mut Map<String, Value>, matches: &ArgMatches) -> Result<(), CliError> {
    if let Some(pane) = matches.get_one::<String>("pane") {
        map.insert("pane".to_owned(), json!(pane));
    }
    let mut bytes = Vec::new();
    for word in matches.get_many::<String>("keys").into_iter().flatten() {
        match key_bytes(word) {
            Some(key) => bytes.extend(key),
            None => bytes.extend(word.as_bytes()),
        }
    }
    // The wire type of `pane.send_bytes` writes the bytes as base64.
    let bytes = serde_json::to_value(Bytes(bytes))
        .map_err(|e| CliError::Failed(format!("the keys do not encode: {e}")))?;
    map.insert("bytes".to_owned(), bytes);
    map.insert("input_id".to_owned(), json!(input_id()));
    Ok(())
}

#[cfg(test)]
mod tests;
