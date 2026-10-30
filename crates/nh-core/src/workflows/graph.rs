//! The rules of a workflow graph: the validation, the depth of each node, the references in a
//! prompt and the layout of "Tidy up".

use std::collections::{HashMap, HashSet};

use nh_proto::api::calls::workflow::{
    WorkflowGraph, WorkflowNode, WorkflowNodeKind, WorkflowProblem,
};

/// The facts of the trigger that a prompt can name: `{{start.base}}`.
pub(crate) const TRIGGER_FIELDS: [&str; 5] = ["folder", "branch", "base", "head", "notes"];
/// The longest id of a node.
const ID_MAX: usize = 32;
/// The most nodes in one workflow.
pub(crate) const NODES_MAX: usize = 64;
/// The longest time limit of an agent node: one day.
pub(crate) const TIME_LIMIT_MAX: u32 = 24 * 60;
/// The distance between two columns of "Tidy up", in canvas units.
const COLUMN: i32 = 300;
/// The distance between two rows of "Tidy up".
const ROW: i32 = 140;

/// A reference in a prompt: `{{node.field}}`, with its place in the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Reference {
    pub start: usize,
    pub end: usize,
    pub node: String,
    pub field: String,
}

/// The references of a text, in order.
pub(crate) fn references(text: &str) -> Vec<Reference> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(open) = text[from..].find("{{").map(|i| i + from) {
        let Some(close) = text[open + 2..].find("}}").map(|i| i + open + 2) else {
            break;
        };
        let inner = text[open + 2..close].trim();
        if let Some((node, field)) = inner.split_once('.')
            && is_id(node)
            && !field.is_empty()
            && field.chars().all(|c| c.is_ascii_lowercase() || c == '_')
        {
            found.push(Reference {
                start: open,
                end: close + 2,
                node: node.to_owned(),
                field: field.to_owned(),
            });
        }
        from = close + 2;
    }
    found
}

/// Puts the values in place of the references. `value` gives `None` for a reference that stays.
pub(crate) fn fill(text: &str, value: impl Fn(&Reference) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for reference in references(text) {
        out.push_str(&text[at..reference.start]);
        match value(&reference) {
            Some(filled) => out.push_str(&filled),
            None => out.push_str(&text[reference.start..reference.end]),
        }
        at = reference.end;
    }
    out.push_str(&text[at..]);
    out
}

/// Whether a text is the id of a node: `[a-z][a-z0-9_]*`.
pub(crate) fn is_id(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && text.len() <= ID_MAX
}

/// The trigger of a graph, if it has exactly one.
pub(crate) fn trigger(graph: &WorkflowGraph) -> Option<&WorkflowNode> {
    let mut triggers = graph
        .nodes
        .iter()
        .filter(|node| node.kind == WorkflowNodeKind::Manual);
    let first = triggers.next()?;
    triggers.next().is_none().then_some(first)
}

/// The title of a node as a person reads it: its title, else its id.
pub(crate) fn title(node: &WorkflowNode) -> &str {
    if node.title.trim().is_empty() {
        &node.id
    } else {
        node.title.trim()
    }
}

/// The errors of a graph, in the order of the nodes.
pub(crate) fn problems(graph: &WorkflowGraph) -> Vec<WorkflowProblem> {
    let mut out = Vec::new();
    let mut add = |node: Option<&str>, text: String| {
        out.push(WorkflowProblem {
            node: node.map(str::to_owned),
            text,
        });
    };
    if graph.name.trim().is_empty() {
        add(None, "The workflow has no name.".to_owned());
    }
    if graph.nodes.len() > NODES_MAX {
        add(
            None,
            format!("The workflow has more than {NODES_MAX} nodes."),
        );
    }
    let triggers = graph
        .nodes
        .iter()
        .filter(|node| node.kind == WorkflowNodeKind::Manual)
        .count();
    if triggers == 0 {
        add(None, "The workflow has no trigger.".to_owned());
    } else if triggers > 1 {
        add(None, "The workflow has more than one trigger.".to_owned());
    }
    let mut seen = HashSet::new();
    for node in &graph.nodes {
        if !is_id(&node.id) {
            add(
                Some(&node.id),
                format!(
                    "The id \"{}\" is not valid. Use a lowercase letter, then lowercase letters, digits or \"_\".",
                    node.id
                ),
            );
        } else if !seen.insert(node.id.as_str()) {
            add(
                Some(&node.id),
                format!("Two nodes have the id \"{}\".", node.id),
            );
        }
    }
    let ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    for node in &graph.nodes {
        let id = Some(node.id.as_str());
        let mut inputs = HashSet::new();
        for input in &node.inputs {
            if input == &node.id {
                add(id, "The node connects to itself.".to_owned());
            } else if !ids.contains(input.as_str()) {
                add(id, format!("The input \"{input}\" is not a node."));
            } else if !inputs.insert(input.as_str()) {
                add(id, format!("The input \"{input}\" is there two times."));
            }
        }
        match node.kind {
            WorkflowNodeKind::Manual => {
                if !node.inputs.is_empty() {
                    add(id, "The trigger cannot have inputs.".to_owned());
                }
            }
            WorkflowNodeKind::Agent => {
                if node.inputs.is_empty() {
                    add(id, "The node has no input.".to_owned());
                }
                if node.harness.is_none() {
                    add(id, "Select Claude Code or Codex.".to_owned());
                }
                if node.prompt.trim().is_empty() {
                    add(id, "The prompt is empty.".to_owned());
                }
                if node
                    .time_limit_min
                    .is_some_and(|limit| limit == 0 || limit > TIME_LIMIT_MAX)
                {
                    add(
                        id,
                        format!("The time limit must be from 1 to {TIME_LIMIT_MAX} minutes."),
                    );
                }
            }
            WorkflowNodeKind::Report => {
                if node.inputs.is_empty() {
                    add(id, "The node has no input.".to_owned());
                }
            }
        }
    }
    let by_id: HashMap<&str, &WorkflowNode> = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    for node in &graph.nodes {
        for reference in references(&node.prompt)
            .into_iter()
            .chain(node.summary_prompt.iter().flat_map(|text| references(text)))
        {
            let text = format!("{{{{{}.{}}}}}", reference.node, reference.field);
            let Some(target) = by_id.get(reference.node.as_str()) else {
                add(
                    Some(&node.id),
                    format!("The reference {text} names no node."),
                );
                continue;
            };
            let connected = node.inputs.iter().any(|input| input == &reference.node)
                || target.kind == WorkflowNodeKind::Manual;
            if !connected {
                add(
                    Some(&node.id),
                    format!(
                        "The reference {text} names a node that does not connect to this node."
                    ),
                );
            } else if target.kind == WorkflowNodeKind::Manual {
                if !TRIGGER_FIELDS.contains(&reference.field.as_str()) {
                    add(
                        Some(&node.id),
                        format!(
                            "The reference {text} is not known. The trigger gives {}.",
                            TRIGGER_FIELDS.join(", ")
                        ),
                    );
                }
            } else if reference.field != "report" {
                add(
                    Some(&node.id),
                    format!(
                        "The reference {text} is not known. A node gives {{{{{}.report}}}}.",
                        reference.node
                    ),
                );
            }
        }
    }
    if let Some(cycle) = cycle(graph) {
        add(
            Some(&cycle),
            "The connections make a loop. A workflow cannot have loops.".to_owned(),
        );
    } else if let Some(start) = trigger(graph) {
        let reached = reachable(graph, &start.id);
        for node in &graph.nodes {
            if !reached.contains(node.id.as_str())
                && node.inputs.iter().all(|i| ids.contains(i.as_str()))
            {
                add(
                    Some(&node.id),
                    "The trigger cannot reach this node.".to_owned(),
                );
            }
        }
    }
    out
}

/// A node on a loop, if the graph has one.
fn cycle(graph: &WorkflowGraph) -> Option<String> {
    let depths = depths(graph);
    graph
        .nodes
        .iter()
        .find(|node| !depths.contains_key(node.id.as_str()))
        .map(|node| node.id.clone())
}

/// The nodes that the trigger reaches.
fn reachable<'a>(graph: &'a WorkflowGraph, start: &'a str) -> HashSet<&'a str> {
    let mut reached = HashSet::from([start]);
    loop {
        let before = reached.len();
        for node in &graph.nodes {
            if node
                .inputs
                .iter()
                .any(|input| reached.contains(input.as_str()))
            {
                reached.insert(node.id.as_str());
            }
        }
        if reached.len() == before {
            return reached;
        }
    }
}

/// The depth of each node: the longest path from a node without inputs. A node on a loop, or
/// after one, has no depth.
pub(crate) fn depths(graph: &WorkflowGraph) -> HashMap<&str, usize> {
    let ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    let mut depth: HashMap<&str, usize> = HashMap::new();
    loop {
        let mut changed = false;
        for node in &graph.nodes {
            if depth.contains_key(node.id.as_str()) {
                continue;
            }
            let inputs: Vec<&str> = node
                .inputs
                .iter()
                .map(String::as_str)
                .filter(|input| ids.contains(input) && *input != node.id)
                .collect();
            if inputs.iter().all(|input| depth.contains_key(input)) {
                let value = inputs
                    .iter()
                    .filter_map(|input| depth.get(input))
                    .map(|d| d + 1)
                    .max()
                    .unwrap_or(0);
                depth.insert(node.id.as_str(), value);
                changed = true;
            }
        }
        if !changed {
            return depth;
        }
    }
}

/// The ids of the nodes in the order in which they start when they are ready at the same time:
/// by column, then from top to bottom.
pub(crate) fn start_order(graph: &WorkflowGraph) -> Vec<String> {
    let depths = depths(graph);
    let mut nodes: Vec<&WorkflowNode> = graph.nodes.iter().collect();
    nodes.sort_by_key(|node| {
        (
            depths.get(node.id.as_str()).copied().unwrap_or(usize::MAX),
            node.y,
            node.x,
            node.id.clone(),
        )
    });
    nodes.into_iter().map(|node| node.id.clone()).collect()
}

/// The graph with the positions of "Tidy up": one column for each depth, and in each column an
/// order that keeps the crossings few.
pub(crate) fn tidy(graph: &WorkflowGraph) -> WorkflowGraph {
    let depths = depths(graph);
    let last = depths.values().copied().max().unwrap_or(0);
    let mut columns: Vec<Vec<&WorkflowNode>> = vec![Vec::new(); last + 2];
    for node in &graph.nodes {
        // A node on a loop goes to a last column of its own.
        let column = depths.get(node.id.as_str()).copied().unwrap_or(last + 1);
        columns[column].push(node);
    }
    let mut row_of: HashMap<String, f64> = HashMap::new();
    let mut place: HashMap<String, (i32, i32)> = HashMap::new();
    let tallest = columns.iter().map(Vec::len).max().unwrap_or(1).max(1);
    for (index, column) in columns.iter_mut().enumerate() {
        column.sort_by(|a, b| {
            let key = |node: &WorkflowNode| {
                let rows: Vec<f64> = node
                    .inputs
                    .iter()
                    .filter_map(|input| row_of.get(input))
                    .copied()
                    .collect();
                if rows.is_empty() {
                    f64::from(node.y)
                } else {
                    rows.iter().sum::<f64>() / rows.len() as f64 * f64::from(ROW)
                }
            };
            key(a)
                .partial_cmp(&key(b))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.y.cmp(&b.y))
                .then(a.id.cmp(&b.id))
        });
        let count = column.len();
        let offset = (tallest - count) as f64 / 2.0;
        for (row, node) in column.iter().enumerate() {
            let at = offset + row as f64;
            row_of.insert(node.id.clone(), at);
            let x = i32::try_from(index).unwrap_or(0) * COLUMN;
            let y = (at * f64::from(ROW)).round() as i32;
            place.insert(node.id.clone(), (x, y));
        }
    }
    let mut out = graph.clone();
    for node in &mut out.nodes {
        if let Some((x, y)) = place.get(&node.id) {
            node.x = *x;
            node.y = *y;
        }
    }
    out
}
