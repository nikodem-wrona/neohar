use super::*;

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn scripts_come_from_package_json() {
    let project = Project::from_texts(
        Some(
            r#"{"name":"x","scripts":{"test":"vitest","build":"tsc","lint:fix":"eslint --fix ."}}"#,
        ),
        None,
        None,
        None,
    );
    assert_eq!(project.package_scripts, set(&["build", "lint:fix", "test"]));
}

#[test]
fn a_broken_package_json_adds_nothing() {
    for text in [
        "",
        "not json",
        "[]",
        r#"{"scripts":[1]}"#,
        r#"{"scripts":"x"}"#,
    ] {
        let project = Project::from_texts(Some(text), None, None, None);
        assert!(project.package_scripts.is_empty(), "{text}");
    }
}

#[test]
fn targets_come_from_a_makefile() {
    let makefile = "\
CC := gcc
VERSION ?= 1
.PHONY: all test
all: build test
build:
\tcc -o x x.c
test lint: build
\t./run
%.o: %.c
\tcc -c $<
";
    let project = Project::from_texts(None, Some(makefile), None, None);
    assert_eq!(project.make_targets, set(&["all", "build", "lint", "test"]));
}

#[test]
fn recipes_come_from_a_justfile() {
    let justfile = "\
set shell := [\"bash\", \"-c\"]
version := \"1\"
# a comment
default: test
test *args:
    cargo test {{args}}
@lint:
    cargo clippy
[private]
helper:
    echo
";
    let project = Project::from_texts(None, None, Some(justfile), None);
    assert_eq!(
        project.just_recipes,
        set(&["default", "helper", "lint", "test"])
    );
}

#[test]
fn targets_come_from_cargo_toml() {
    let cargo = r#"
[package]
name = "app"

[[bin]]
name = "tool"
path = "src/tool.rs"

[[example]]
name = "demo"

[[bench]]
name = "speed"
"#;
    let project = Project::from_texts(None, None, None, Some(cargo));
    assert_eq!(
        project.cargo_targets,
        set(&["app", "demo", "speed", "tool"])
    );
}

#[test]
fn scan_reads_the_files_of_a_directory() {
    let dir = std::env::temp_dir().join(format!("nh-rich-project-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("package.json"), r#"{"scripts":{"test":"x"}}"#).expect("file");
    std::fs::write(dir.join("justfile"), "check:\n    true\n").expect("file");
    let project = Project::scan(&dir);
    assert_eq!(project.package_scripts, set(&["test"]));
    assert_eq!(project.just_recipes, set(&["check"]));
    assert!(project.make_targets.is_empty());
    std::fs::remove_dir_all(&dir).expect("cleanup");
    // A directory that does not exist gives an empty project.
    assert_eq!(Project::scan(&dir), Project::default());
}
