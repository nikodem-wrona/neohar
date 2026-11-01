//! The presets: the items of the node picker, and the workflows that `workflow.create` can start
//! from.

use nh_proto::api::{
    calls::workflow::{
        WorkflowGraph, WorkflowNode, WorkflowNodeKind, WorkflowNodePreset, WorkflowPreset,
        WorkflowPresets,
    },
    common::Harness,
};

const CODE_REVIEW: &str = "\
Review the changes of the branch {{start.branch}} against {{start.base}}. Read the diff with git, and read the code around it.
Look for bugs, missing error handling, missing tests, and code that is hard to read or to change.
For each finding, give the file and the line, what is wrong, and a fix.";

const LOGIC_REVIEW: &str = "\
The author describes the change so:
{{start.notes}}

Check that the changes of the branch {{start.branch}} against {{start.base}} do this, and only this.
Look for cases that the change does not handle, rules of the domain that it breaks, and behavior that a user does not expect.
For each finding, give the file and the line, the case, and what must happen.";

const SECURITY_REVIEW: &str = "\
Do a security review of the changes of the branch {{start.branch}} against {{start.base}}.
Look for injection, unsafe handling of paths and of input, secrets in the code or in logs, missing checks of permissions, and unsafe dependencies.
For each finding, give the file and the line, the risk, how bad it is, and a fix.";

fn agent(id: &str, title: &str, harness: Harness, prompt: &str) -> WorkflowNode {
    WorkflowNode {
        id: id.to_owned(),
        kind: WorkflowNodeKind::Agent,
        title: title.to_owned(),
        harness: Some(harness),
        model: None,
        effort: None,
        can_edit: false,
        time_limit_min: None,
        prompt: prompt.to_owned(),
        summary_by: None,
        summary_prompt: None,
        inputs: Vec::new(),
        x: 0,
        y: 0,
    }
}

/// A node of a kind, with no settings.
pub(crate) fn bare(id: &str, kind: WorkflowNodeKind) -> WorkflowNode {
    WorkflowNode {
        id: id.to_owned(),
        kind,
        title: String::new(),
        harness: None,
        model: None,
        effort: None,
        can_edit: false,
        time_limit_min: None,
        prompt: String::new(),
        summary_by: None,
        summary_prompt: None,
        inputs: Vec::new(),
        x: 0,
        y: 0,
    }
}

/// The trigger of a new workflow.
pub(crate) fn trigger() -> WorkflowNode {
    WorkflowNode {
        title: "Manual trigger".to_owned(),
        y: 140,
        ..bare("start", WorkflowNodeKind::Manual)
    }
}

/// The items of the node picker and the preset workflows.
pub(crate) fn presets() -> WorkflowPresets {
    let node = |id: &str, label: &str, node: WorkflowNode| WorkflowNodePreset {
        id: id.to_owned(),
        label: label.to_owned(),
        node,
    };
    WorkflowPresets {
        nodes: vec![
            node(
                "claude",
                "Claude Code agent",
                WorkflowNode {
                    title: "Claude Code".to_owned(),
                    harness: Some(Harness::Claude),
                    ..bare("", WorkflowNodeKind::Agent)
                },
            ),
            node(
                "codex",
                "Codex agent",
                WorkflowNode {
                    title: "Codex".to_owned(),
                    harness: Some(Harness::Codex),
                    ..bare("", WorkflowNodeKind::Agent)
                },
            ),
            node(
                "report",
                "Report",
                WorkflowNode {
                    title: "Report".to_owned(),
                    ..bare("", WorkflowNodeKind::Report)
                },
            ),
            node(
                "code_review",
                "Code review",
                agent("", "Code review", Harness::Codex, CODE_REVIEW),
            ),
            node(
                "logic_review",
                "Business-logic review",
                agent("", "Business-logic review", Harness::Claude, LOGIC_REVIEW),
            ),
            node(
                "security_review",
                "Security review",
                agent("", "Security review", Harness::Claude, SECURITY_REVIEW),
            ),
        ],
        workflows: vec![WorkflowPreset {
            id: "review".to_owned(),
            label: "Review: code, business logic and security, then one report".to_owned(),
        }],
    }
}

/// The graph of a new workflow: a preset, or only the trigger.
pub(crate) fn graph(name: &str, preset: Option<&str>) -> Option<WorkflowGraph> {
    let nodes = match preset {
        None => vec![trigger()],
        Some("review") => {
            let at = |mut node: WorkflowNode, inputs: &[&str], x: i32, y: i32| {
                node.inputs = inputs.iter().map(|&input| input.to_owned()).collect();
                node.x = x;
                node.y = y;
                node
            };
            vec![
                trigger(),
                at(
                    agent("code", "Code review", Harness::Codex, CODE_REVIEW),
                    &["start"],
                    300,
                    0,
                ),
                at(
                    agent(
                        "logic",
                        "Business-logic review",
                        Harness::Claude,
                        LOGIC_REVIEW,
                    ),
                    &["start"],
                    300,
                    140,
                ),
                at(
                    agent(
                        "security",
                        "Security review",
                        Harness::Claude,
                        SECURITY_REVIEW,
                    ),
                    &["start"],
                    300,
                    280,
                ),
                at(
                    WorkflowNode {
                        title: "Review report".to_owned(),
                        summary_by: Some(Harness::Claude),
                        ..bare("report", WorkflowNodeKind::Report)
                    },
                    &["code", "logic", "security"],
                    600,
                    140,
                ),
            ]
        }
        Some(_) => return None,
    };
    Some(WorkflowGraph {
        name: name.to_owned(),
        nodes,
    })
}
