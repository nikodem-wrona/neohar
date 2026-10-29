use std::{fs, path::PathBuf};

use nh_proto::api::{calls::agent::SkillSource, common::Harness};

use super::{Front, Roots, front_matter, list};

/// A new folder for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nh-skills-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("the scratch folder");
    dir
}

fn write(path: PathBuf, text: &str) {
    fs::create_dir_all(path.parent().expect("a parent")).expect("the folder");
    fs::write(path, text).expect("the file");
}

fn skill(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n")
}

fn names(skills: &[nh_proto::api::calls::agent::Skill]) -> Vec<&str> {
    skills.iter().map(|s| s.name.as_str()).collect()
}

#[test]
fn front_matter_reads_plain_quoted_and_folded_values() {
    let text = "---\nname: \"review\"\ndescription: >\n  Reviews the diff\n  of the branch.\nuser-invocable: false\nallowed-tools: Read\n---\nbody";
    assert_eq!(
        front_matter(text),
        Front {
            name: Some("review".into()),
            description: Some("Reviews the diff of the branch.".into()),
            user_invocable: false,
        }
    );
    let continued = "---\ndescription: one\n  two\nname: x\n---\n";
    assert_eq!(
        front_matter(continued).description.as_deref(),
        Some("one two")
    );
}

#[test]
fn a_file_without_front_matter_gives_nothing() {
    let front = front_matter("# A title\n\nname: not front matter\n");
    assert_eq!(front.name, None);
    assert_eq!(front.description, None);
    assert!(front.user_invocable);
}

#[test]
fn claude_lists_project_user_and_plugin_skills_and_commands() {
    let root = scratch("claude");
    let config = root.join("config");
    let repo = root.join("repo");
    let cwd = repo.join("crates/a");
    fs::create_dir_all(repo.join(".git")).expect("the repository");
    fs::create_dir_all(&cwd).expect("the pane folder");
    write(
        repo.join(".claude/skills/deploy/SKILL.md"),
        &skill("deploy", "Ships it"),
    );
    write(
        repo.join(".claude/commands/fix-ci.md"),
        "---\ndescription: Fixes CI\n---\n",
    );
    // The name of the project hides the one of the user.
    write(
        config.join("skills/deploy/SKILL.md"),
        &skill("deploy", "Old one"),
    );
    write(config.join("skills/notes/SKILL.md"), "no front matter\n");
    write(
        config.join("skills/hidden/SKILL.md"),
        "---\nname: hidden\nuser-invocable: false\n---\n",
    );
    let plugin = root.join("plugins/figma/1.0.0");
    write(
        plugin.join("skills/figma-use/SKILL.md"),
        &skill("figma-use", "Uses Figma"),
    );
    write(plugin.join("commands/sync.md"), "Syncs.\n");
    let off = root.join("plugins/off/1.0.0");
    write(off.join("skills/never/SKILL.md"), &skill("never", "Off"));
    write(
        config.join("settings.json"),
        r#"{"enabledPlugins": {"figma@market": true, "off@market": false}}"#,
    );
    let installed = serde_json::json!({
        "version": 2,
        "plugins": {
            "figma@market": [{"installPath": plugin}],
            "off@market": [{"installPath": off}],
        }
    });
    write(
        config.join("plugins/installed_plugins.json"),
        &installed.to_string(),
    );

    let roots = Roots {
        config,
        home: Some(root.join("home")),
        cwd: Some(cwd),
    };
    let skills = list(Harness::Claude, &roots, None);
    assert_eq!(
        names(&skills),
        ["deploy", "figma:figma-use", "figma:sync", "fix-ci", "notes"]
    );
    let deploy = &skills[0];
    assert_eq!(deploy.description.as_deref(), Some("Ships it"));
    assert_eq!(deploy.source, SkillSource::Project);
    assert_eq!(deploy.insert, "/deploy ");
    assert_eq!(skills[1].source, SkillSource::Plugin);
    assert_eq!(skills[4].source, SkillSource::User);
    assert_eq!(skills[4].description, None);

    // A query keeps the names that have it, and those that start with it come first.
    let skills = list(Harness::Claude, &roots, Some("N"));
    assert_eq!(names(&skills), ["notes", "figma:sync"]);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn codex_lists_its_home_the_system_skills_and_agents_folders() {
    let root = scratch("codex");
    let config = root.join("codex");
    let home = root.join("home");
    let cwd = root.join("work");
    write(
        config.join("skills/commit/SKILL.md"),
        &skill("commit", "Commits"),
    );
    write(
        config.join("skills/.system/skill-creator/SKILL.md"),
        &skill("skill-creator", "Makes skills"),
    );
    write(
        home.join(".agents/skills/herdr/SKILL.md"),
        &skill("herdr", "Herdr"),
    );
    write(
        cwd.join(".agents/skills/local/SKILL.md"),
        &skill("local", "Here"),
    );
    // Claude Code places are not places of Codex.
    write(
        cwd.join(".claude/skills/claude-only/SKILL.md"),
        &skill("claude-only", "No"),
    );

    let roots = Roots {
        config,
        home: Some(home),
        cwd: Some(cwd),
    };
    let skills = list(Harness::Codex, &roots, None);
    assert_eq!(
        names(&skills),
        ["commit", "herdr", "local", "skill-creator"]
    );
    assert_eq!(skills[0].insert, "$commit ");
    assert_eq!(skills[2].source, SkillSource::Project);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn the_home_folder_is_never_a_project() {
    let root = scratch("home");
    let home = root.join("home");
    write(
        home.join(".claude/skills/mine/SKILL.md"),
        &skill("mine", "Mine"),
    );
    let roots = Roots {
        config: home.join(".claude"),
        home: Some(home.clone()),
        cwd: Some(home),
    };
    let skills = list(Harness::Claude, &roots, None);
    assert_eq!(names(&skills), ["mine"]);
    assert_eq!(skills[0].source, SkillSource::User);
    let _ = fs::remove_dir_all(root);
}
