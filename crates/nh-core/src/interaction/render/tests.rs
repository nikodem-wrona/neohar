use std::collections::HashMap;

use nh_proto::api::model::{InteractionKind, InteractionPayload};
use serde_json::json;

use super::*;

struct World {
    project: Project,
    files: HashMap<PathBuf, String>,
    allow: Vec<String>,
}

impl World {
    fn new() -> Self {
        Self {
            project: Project::from_texts(
                Some(r#"{"scripts":{"test":"vitest","deploy":"./d.sh"}}"#),
                None,
                None,
                None,
            ),
            files: HashMap::new(),
            allow: Vec::new(),
        }
    }

    fn render(&self, kind: InteractionKind, raw: &Value) -> Rendered {
        let read = |path: &Path| self.files.get(path).cloned();
        let ctx = RenderCtx {
            workspace: Path::new("/work/app"),
            cwd: Path::new("/work/app"),
            home: Some(Path::new("/home/me")),
            project: &self.project,
            network_allow: &self.allow,
            peek: vec!["$ pnpm test".to_owned()],
            read_file: &read,
        };
        render(kind, raw, &ctx)
    }
}

fn approval(rendered: &Rendered) -> &ApprovalPayload {
    let InteractionPayload::Approval(payload) = &rendered.payload else {
        panic!("not an approval");
    };
    payload
}

#[test]
fn a_shell_command_gets_its_words_risk_and_reasons() {
    let world = World::new();
    let rendered = world.render(
        InteractionKind::Approval,
        &json!({"tool": "Bash", "command": "pnpm test", "paths": [], "input": {"command": "pnpm test"}}),
    );
    assert_eq!(rendered.title, "Run `pnpm test`");
    assert_eq!(rendered.risk, Risk::Low);
    let payload = approval(&rendered);
    assert_eq!(payload.tool, "Bash");
    let command = payload.command.as_ref().expect("a command");
    assert_eq!(command.argv, ["pnpm", "test"]);
    assert_eq!(command.display.text, "pnpm test");
    assert!(!command.reasons.is_empty());
    assert_eq!(payload.cwd.as_deref(), Some("/work/app"));
    assert_eq!(payload.peek, ["$ pnpm test"]);
    assert!(payload.diff.is_none() && payload.input.is_none());
}

#[test]
fn a_dangerous_command_is_high_risk() {
    let world = World::new();
    let rendered = world.render(
        InteractionKind::Approval,
        &json!({"tool": "Bash", "command": "rm -rf build && git push --force", "paths": []}),
    );
    assert_eq!(rendered.risk, Risk::High);
    assert_eq!(
        approval(&rendered)
            .command
            .as_ref()
            .expect("command")
            .policy_hint,
        None
    );
}

#[test]
fn a_long_command_has_a_short_one_line_title() {
    let world = World::new();
    let line = format!("echo {}\nsecond line", "a".repeat(200));
    let rendered = world.render(
        InteractionKind::Approval,
        &json!({"tool": "Bash", "command": line, "paths": []}),
    );
    assert!(rendered.title.starts_with("Run `echo aaa"));
    assert!(rendered.title.chars().count() < 100, "{}", rendered.title);
    assert!(!rendered.title.contains('\n'));
    assert!(rendered.title.ends_with("…`"));
}

#[test]
fn the_words_of_a_command_in_a_list_are_joined() {
    let world = World::new();
    let rendered = world.render(
        InteractionKind::Approval,
        &json!({"tool": "exec_command", "command": ["git", "status"], "paths": []}),
    );
    assert_eq!(rendered.risk, Risk::Low);
    assert_eq!(
        approval(&rendered).command.as_ref().expect("command").argv,
        ["git", "status"]
    );
}

#[test]
fn an_edit_gets_a_diff_of_the_file() {
    let mut world = World::new();
    world.files.insert(
        PathBuf::from("/work/app/src/a.rs"),
        "fn a() {\n    one();\n}\n".to_owned(),
    );
    let rendered = world.render(
        InteractionKind::Approval,
        &json!({
            "tool": "Edit",
            "command": null,
            "paths": ["src/a.rs"],
            "input": {"file_path": "src/a.rs", "old_string": "one();", "new_string": "two();"},
        }),
    );
    assert_eq!(rendered.title, "Edit `src/a.rs`");
    assert_eq!(rendered.risk, Risk::Unknown);
    let payload = approval(&rendered);
    assert!(payload.command.is_none());
    let diff = payload.diff.as_ref().expect("a diff");
    assert_eq!(diff.files[0].path, "src/a.rs");
    // The file was found, so the lines have their numbers.
    assert_eq!(diff.files[0].hunks[0].header, "@@ -1,3 +1,3 @@");
    assert!(payload.input.is_none());
}

#[test]
fn a_multi_edit_and_a_write_get_diffs() {
    let world = World::new();
    let multi = world.render(
        InteractionKind::Approval,
        &json!({
            "tool": "MultiEdit",
            "paths": ["a.txt"],
            "input": {"file_path": "a.txt", "edits": [
                {"old_string": "a", "new_string": "b"},
                {"old_string": "c", "new_string": "d", "replace_all": true},
            ]},
        }),
    );
    let diff = approval(&multi).diff.as_ref().expect("a diff").clone();
    assert_eq!(diff.files[0].hunks.len(), 2);
    let write = world.render(
        InteractionKind::Approval,
        &json!({"tool": "Write", "paths": ["new.txt"], "input": {"file_path": "new.txt", "content": "hello\n"}}),
    );
    assert_eq!(write.title, "Write `new.txt`");
    let diff = approval(&write).diff.as_ref().expect("a diff");
    assert_eq!(diff.files[0].status, nh_proto::api::rich::FileStatus::Added);
}

#[test]
fn a_change_outside_the_workspace_or_of_secrets_is_high_risk() {
    let world = World::new();
    for path in [
        "/etc/hosts",
        "../other/x.rs",
        ".env",
        "/work/app/config/credentials.json",
    ] {
        let rendered = world.render(
            InteractionKind::Approval,
            &json!({"tool": "Write", "paths": [path], "input": {"file_path": path, "content": "x"}}),
        );
        assert_eq!(rendered.risk, Risk::High, "{path}");
    }
}

#[test]
fn a_tool_without_a_card_shows_its_input_cut_at_a_cap() {
    let world = World::new();
    let rendered = world.render(
        InteractionKind::Approval,
        &json!({"tool": "mcp__db__query", "command": null, "paths": [], "input": {"sql": "select 1"}}),
    );
    assert_eq!(rendered.title, "mcp__db__query");
    assert_eq!(rendered.risk, Risk::Unknown);
    assert_eq!(approval(&rendered).input, Some(json!({"sql": "select 1"})));
    let big = "x".repeat(INPUT_CAP * 2);
    let rendered = world.render(
        InteractionKind::Approval,
        &json!({"tool": "mcp__db__query", "paths": [], "input": {"sql": big}}),
    );
    let Some(Value::String(cut)) = &approval(&rendered).input else {
        panic!("a cut input is a text");
    };
    assert!(cut.len() <= INPUT_CAP + 8, "{}", cut.len());
}

#[test]
fn a_question_has_its_options() {
    let world = World::new();
    let rendered = world.render(
        InteractionKind::Question,
        &json!({"tool": "AskUserQuestion", "questions": [
            {"question": "Which library?", "header": "Library", "multiSelect": false,
             "options": [{"label": "A", "description": "first"}, {"label": "B"}]},
            {"question": "Which flags?", "multiSelect": true, "options": [{"label": "x"}]},
        ]}),
    );
    assert_eq!(rendered.title, "Which library?");
    let InteractionPayload::Question(payload) = rendered.payload else {
        panic!("not a question");
    };
    assert_eq!(payload.questions.len(), 2);
    assert_eq!(
        payload.questions[0].options[0].description.as_deref(),
        Some("first")
    );
    assert_eq!(payload.questions[0].header.as_deref(), Some("Library"));
    assert!(payload.questions[1].multi_select);
}

#[test]
fn an_mcp_elicitation_is_one_question_with_free_text() {
    let world = World::new();
    let rendered = world.render(
        InteractionKind::Question,
        &json!({"source": "elicitation", "server": "db", "message": "Which table?", "schema": {}}),
    );
    let InteractionPayload::Question(payload) = rendered.payload else {
        panic!("not a question");
    };
    assert_eq!(payload.questions[0].question, "Which table?");
    assert_eq!(payload.questions[0].header.as_deref(), Some("db"));
    assert!(payload.questions[0].options.is_empty());
}

#[test]
fn a_plan_is_sanitized_html() {
    let world = World::new();
    let rendered = world.render(
        InteractionKind::PlanReview,
        &json!({"tool": "ExitPlanMode", "plan_md": "# Plan\n\n<script>alert(1)</script>\n- one", "plan_file": "/tmp/plan.md"}),
    );
    assert_eq!(rendered.title, "Review the plan");
    let InteractionPayload::PlanReview(payload) = rendered.payload else {
        panic!("not a plan");
    };
    assert!(payload.plan.html.contains("<h1>Plan</h1>"));
    assert!(!payload.plan.html.contains("script"));
    assert_eq!(payload.plan_path.as_deref(), Some("/tmp/plan.md"));
}

#[test]
fn a_notice_is_text() {
    let world = World::new();
    let rendered = world.render(
        InteractionKind::Notice,
        &json!({"text": "Rate limited. Resets at noon.", "resets_at": 1_760_000_000_000_i64}),
    );
    let InteractionPayload::Notice(payload) = rendered.payload else {
        panic!("not a notice");
    };
    assert_eq!(payload.text, "Rate limited. Resets at noon.");
    assert_eq!(payload.resets_at.map(|t| t.0), Some(1_760_000_000_000));
}

#[test]
fn missing_fields_make_a_plain_card_and_not_a_panic() {
    let world = World::new();
    for kind in [
        InteractionKind::Approval,
        InteractionKind::Question,
        InteractionKind::PlanReview,
        InteractionKind::Notice,
    ] {
        for raw in [
            json!({}),
            Value::Null,
            json!({"tool": 5, "command": 5, "paths": "x"}),
        ] {
            let rendered = world.render(kind, &raw);
            assert!(!rendered.title.is_empty(), "{kind:?} {raw}");
        }
    }
}
