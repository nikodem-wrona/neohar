use std::sync::Arc;

use nh_proto::api::{
    calls::workflow::{WorkflowGraph, WorkflowNodeKind},
    common::Harness,
    envelope::ErrorCode,
    projections::Projection,
};

use super::{
    file, graph,
    library::Library,
    presets::{self, bare},
};
use crate::projection::ProjectionHub;

fn review() -> WorkflowGraph {
    presets::graph("Review", Some("review")).expect("preset")
}

fn texts(graph: &WorkflowGraph) -> Vec<(Option<String>, String)> {
    graph::problems(graph)
        .into_iter()
        .map(|p| (p.node, p.text))
        .collect()
}

#[test]
fn the_review_preset_is_valid_and_its_file_reads_back_the_same() {
    let graph = review();
    assert!(graph::problems(&graph).is_empty(), "{:?}", texts(&graph));
    let text = file::render(&graph);
    assert!(text.contains("[[node]]"));
    assert!(text.contains("inputs = [\n    \"code\",") || text.contains("inputs = [\"code\""));
    assert_eq!(file::parse(&text).expect("parse"), graph);
}

#[test]
fn a_file_with_an_unknown_key_is_refused_with_its_line() {
    let error =
        file::parse("name = \"X\"\n\n[[node]]\nid = \"start\"\nkind = \"manual\"\ncolour = 3\n")
            .expect_err("unknown key");
    assert!(error.contains("colour"), "{error}");
    assert!(error.contains("line 6") || error.contains("6 |"), "{error}");
}

#[test]
fn the_file_keeps_only_the_settings_of_the_kind_of_a_node() {
    let mut graph = review();
    graph.nodes[0].harness = Some(Harness::Codex);
    graph.nodes[0].prompt = "ignored".to_owned();
    let back = file::parse(&file::render(&graph)).expect("parse");
    assert_eq!(back.nodes[0].harness, None);
    assert!(back.nodes[0].prompt.is_empty());
}

#[test]
fn the_name_of_a_file_comes_from_the_name_of_the_workflow() {
    assert_eq!(file::slug("Review"), "review");
    assert_eq!(file::slug("  Fix & re-check!  "), "fix-re-check");
    assert_eq!(file::slug("***"), "workflow");
    assert!(file::is_stem("fix-re-check"));
    assert!(!file::is_stem("../x"));
    assert!(!file::is_stem(""));
}

#[test]
fn the_validation_finds_each_fault_of_a_graph() {
    let mut graph = review();
    // A second trigger, a loop, a missing harness, an empty prompt, a bad reference.
    graph.nodes.push(bare("again", WorkflowNodeKind::Manual));
    graph.nodes[1].inputs.push("report".to_owned());
    graph.nodes[2].harness = None;
    graph.nodes[3].prompt = "Read {{code.report}} and {{start.colour}}".to_owned();
    let found = texts(&graph);
    let has = |node: Option<&str>, part: &str| {
        found
            .iter()
            .any(|(n, t)| n.as_deref() == node && t.contains(part))
    };
    assert!(has(None, "more than one trigger"), "{found:?}");
    assert!(
        has(Some("logic"), "Select Claude Code or Codex"),
        "{found:?}"
    );
    assert!(
        has(Some("security"), "does not connect to this node"),
        "{found:?}"
    );
    assert!(
        has(Some("security"), "{{start.colour}} is not known"),
        "{found:?}"
    );
    assert!(found.iter().any(|(_, t)| t.contains("loop")), "{found:?}");
}

#[test]
fn a_node_that_the_trigger_cannot_reach_is_an_error() {
    let mut graph = review();
    let mut lost = bare("lost", WorkflowNodeKind::Report);
    lost.inputs = vec!["orphan".to_owned()];
    let orphan = {
        let mut node = bare("orphan", WorkflowNodeKind::Agent);
        node.harness = Some(Harness::Claude);
        node.prompt = "x".to_owned();
        node
    };
    graph.nodes.push(orphan);
    graph.nodes.push(lost);
    let found = texts(&graph);
    assert!(
        found
            .iter()
            .any(|(n, t)| n.as_deref() == Some("orphan") && t.contains("no input")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|(n, t)| n.as_deref() == Some("lost") && t.contains("cannot reach")),
        "{found:?}"
    );
}

#[test]
fn the_references_are_filled_in_and_the_unknown_ones_stay() {
    let text = "Diff {{start.branch}} against {{ start.base }}; read {{code.report}}; {{nope}}";
    let refs = graph::references(text);
    assert_eq!(
        refs.iter()
            .map(|r| (r.node.as_str(), r.field.as_str()))
            .collect::<Vec<_>>(),
        [("start", "branch"), ("start", "base"), ("code", "report")]
    );
    let filled = graph::fill(text, |r| match r.field.as_str() {
        "branch" => Some("feat/x".to_owned()),
        "base" => Some("main".to_owned()),
        _ => None,
    });
    assert_eq!(
        filled,
        "Diff feat/x against main; read {{code.report}}; {{nope}}"
    );
}

#[test]
fn nodes_start_by_column_then_from_top_to_bottom() {
    let graph = review();
    assert_eq!(
        graph::start_order(&graph),
        ["start", "code", "logic", "security", "report"]
    );
}

#[test]
fn tidy_up_puts_one_column_for_each_depth() {
    let mut graph = review();
    for node in &mut graph.nodes {
        node.x = 999;
        node.y = 999;
    }
    let tidy = graph::tidy(&graph);
    let at = |id: &str| {
        let node = tidy.nodes.iter().find(|n| n.id == id).expect("node");
        (node.x, node.y)
    };
    assert_eq!(at("start").0, 0);
    assert_eq!(at("code").0, 300);
    assert_eq!(at("report").0, 600);
    // The three reviews are in one column, one below the other.
    let ys = [at("code").1, at("logic").1, at("security").1];
    assert!(ys[0] < ys[1] && ys[1] < ys[2], "{ys:?}");
    // The trigger and the report are in the middle of the column of three.
    assert_eq!(at("start").1, at("logic").1);
    assert_eq!(at("report").1, at("logic").1);
}

/// A folder of its own for a test, removed when the test ends.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("nh-workflows-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        Self(dir)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn library(name: &str) -> (Scratch, Arc<Library>, Arc<ProjectionHub>) {
    let dir = Scratch::new(name);
    let hub = ProjectionHub::new();
    let library = Library::new(dir.path().join("workflows"), Arc::clone(&hub));
    (dir, library, hub)
}

#[test]
fn a_save_needs_the_revision_of_now_and_undo_and_redo_move_through_the_saves() {
    let (_dir, library, hub) = library("save");
    let created = library.create("Review", Some("review")).expect("create");
    assert_eq!(created.workflow, "review");
    assert!(!created.can_undo);

    let mut graph = created.graph.clone();
    graph.nodes[1].title = "Code review by Codex".to_owned();
    let saved = library
        .save("review", &created.revision, graph.clone())
        .expect("save");
    assert!(saved.can_undo);
    assert_ne!(saved.revision, created.revision);

    // A second save from the old revision is a conflict.
    let stale = library
        .save("review", &created.revision, graph)
        .expect_err("stale");
    assert_eq!(stale.code, ErrorCode::Conflict);

    let back = library.step("review", true).expect("undo");
    assert_eq!(back.graph.nodes[1].title, "Code review");
    assert!(back.can_redo);
    let again = library.step("review", false).expect("redo");
    assert_eq!(again.graph.nodes[1].title, "Code review by Codex");

    hub.flush();
    let rows = hub.snapshot(Projection::Workflows);
    assert_eq!(rows.order, ["review"]);
}

#[test]
fn rename_moves_the_file_and_duplicate_and_delete_change_the_list() {
    let (_dir, library, hub) = library("rename");
    library.create("Review", None).expect("create");
    let renamed = library.rename("review", "Deep review").expect("rename");
    assert_eq!(renamed.workflow, "deep-review");
    assert_eq!(renamed.graph.name, "Deep review");
    let copy = library.duplicate("deep-review").expect("duplicate");
    assert_eq!(copy.workflow, "deep-review-copy");
    library.delete("deep-review").expect("delete");
    hub.flush();
    assert_eq!(
        hub.snapshot(Projection::Workflows).order,
        ["deep-review-copy"]
    );
    assert_eq!(
        library.show("deep-review").expect_err("gone").code,
        ErrorCode::NotFound
    );
}

#[test]
fn a_file_with_an_error_shows_as_one_problem_and_cannot_be_opened() {
    let (dir, library, _hub) = library("broken");
    let folder = dir.path().join("workflows");
    std::fs::create_dir_all(&folder).expect("dir");
    std::fs::write(folder.join("broken.toml"), "name = ").expect("write");
    let values = library.values();
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].problems, 1);
    let error = library.show("broken").expect_err("broken");
    assert!(
        error.message.contains("Correct it by hand"),
        "{}",
        error.message
    );
}
