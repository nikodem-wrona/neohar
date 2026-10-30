//! The TOML file of a workflow: `<config>/workflows/<name>.toml`.
//!
//! The file has the name of the workflow and one `[[node]]` table for each node. `inputs` holds
//! the connections and `pos` the position on the canvas, so a diff of the file stays easy to
//! read. The core refuses a key that it does not know, and tells the line.

use nh_proto::api::{
    calls::workflow::{WorkflowGraph, WorkflowNode, WorkflowNodeKind},
    common::Harness,
};
use serde::{Deserialize, Serialize};

/// The extension of a workflow file.
pub(crate) const EXTENSION: &str = "toml";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Doc {
    name: String,
    #[serde(default, rename = "node", skip_serializing_if = "Vec::is_empty")]
    nodes: Vec<Node>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Node {
    id: String,
    kind: WorkflowNodeKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    harness: Option<Harness>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    effort: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    can_edit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    time_limit_min: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    summary_by: Option<Harness>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    summary_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    inputs: Vec<String>,
    #[serde(default)]
    pos: [i32; 2],
    #[serde(default, skip_serializing_if = "String::is_empty")]
    prompt: String,
}

const fn is_false(value: &bool) -> bool {
    !*value
}

/// Reads the text of a file. The error tells the key and the line.
pub(crate) fn parse(text: &str) -> Result<WorkflowGraph, String> {
    let doc: Doc = toml::from_str(text).map_err(|error| error.to_string().trim().to_owned())?;
    Ok(WorkflowGraph {
        name: doc.name,
        nodes: doc.nodes.into_iter().map(node_of).collect(),
    })
}

/// The text of a file for a graph.
pub(crate) fn render(graph: &WorkflowGraph) -> String {
    let doc = Doc {
        name: graph.name.clone(),
        nodes: graph.nodes.iter().map(file_node).collect(),
    };
    // A graph of strings, numbers and lists always serializes.
    toml::to_string_pretty(&doc).unwrap_or_default()
}

fn node_of(node: Node) -> WorkflowNode {
    WorkflowNode {
        id: node.id,
        kind: node.kind,
        title: node.title,
        harness: node.harness,
        model: node.model,
        effort: node.effort,
        can_edit: node.can_edit,
        time_limit_min: node.time_limit_min,
        prompt: node.prompt,
        summary_by: node.summary_by,
        summary_prompt: node.summary_prompt,
        inputs: node.inputs,
        x: node.pos[0],
        y: node.pos[1],
    }
}

fn file_node(node: &WorkflowNode) -> Node {
    let agent = node.kind == WorkflowNodeKind::Agent;
    let report = node.kind == WorkflowNodeKind::Report;
    Node {
        id: node.id.clone(),
        kind: node.kind,
        title: node.title.clone(),
        harness: node.harness.filter(|_| agent),
        model: node.model.clone().filter(|_| agent),
        effort: node.effort.clone().filter(|_| agent),
        can_edit: node.can_edit && agent,
        time_limit_min: node.time_limit_min.filter(|_| agent),
        summary_by: node.summary_by.filter(|_| report),
        summary_prompt: node.summary_prompt.clone().filter(|_| report),
        inputs: node.inputs.clone(),
        pos: [node.x, node.y],
        prompt: if agent {
            node.prompt.clone()
        } else {
            String::new()
        },
    }
}

/// The name of a file for the name of a workflow: lowercase letters, digits and `-`.
pub(crate) fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "workflow".to_owned()
    } else {
        out
    }
}

/// Whether a text is the name of a workflow file without the extension.
pub(crate) fn is_stem(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// The revision of a text: 16 hex characters of its BLAKE3 hash.
pub(crate) fn revision(text: &str) -> String {
    let hash = blake3::hash(text.as_bytes());
    hash.to_hex()[..16].to_owned()
}
