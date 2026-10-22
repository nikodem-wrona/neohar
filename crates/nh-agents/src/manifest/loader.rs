//! Loads the manifests: the built-in ones, and then the files of the user in a
//! directory. A file with the id of a manifest that is in force changes it (a deep merge), and a
//! file with `extends` starts from the manifest that it names. A file that does not parse, or that
//! fails a check, is left out with a note, and the manifest below it stays in force.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{
    Entry, Loaded, Manifest, Source, Warning,
    merge::{deep_merge, unknown_keys},
    validate::check,
};

/// The manifests that are compiled in, by id.
const BUILTINS: &[(&str, &str)] = &[
    ("claude", include_str!("../../manifests/claude.toml")),
    ("codex", include_str!("../../manifests/codex.toml")),
];

/// Makes a manifest from a merged table, and lists the keys of `given` that the types do not
/// know. An error is why the table is not a manifest.
fn assemble(merged: &toml::Table, given: &toml::Table) -> Result<(Manifest, Vec<String>), String> {
    let manifest: Manifest = toml::Value::Table(merged.clone())
        .try_into()
        .map_err(|e: toml::de::Error| e.to_string())?;
    let problems = check(&manifest);
    if !problems.is_empty() {
        return Err(problems.join("; "));
    }
    let known = toml::Table::try_from(&manifest).map_err(|e| e.to_string())?;
    Ok((manifest, unknown_keys(given, &known)))
}

/// The manifests that are compiled in. They are right, and a test says so.
pub fn builtin() -> Loaded {
    let mut loaded = Loaded::default();
    for (name, text) in BUILTINS {
        let table: toml::Table = match text.parse() {
            Ok(table) => table,
            Err(error) => {
                loaded.warnings.push(Warning {
                    file: None,
                    message: format!("the built-in manifest {name} does not parse: {error}"),
                });
                continue;
            }
        };
        match assemble(&table, &table) {
            Ok((manifest, _)) => {
                loaded.manifests.insert(
                    manifest.id.clone(),
                    Entry {
                        manifest,
                        source: Source::Builtin,
                        raw: table,
                    },
                );
            }
            Err(error) => loaded.warnings.push(Warning {
                file: None,
                message: format!("the built-in manifest {name} is not right: {error}"),
            }),
        }
    }
    loaded
}

/// Writes the built-in manifests into `dir`, for a person who wants to read them or to copy one.
/// The files are not read back.
pub fn write_builtins(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    for (name, text) in BUILTINS {
        fs::write(dir.join(format!("{name}.toml")), text)?;
    }
    Ok(())
}

/// Whether a file of the user is in force now, or waits for the manifest that it extends.
enum Applied {
    Done,
    Waits,
}

fn warn(loaded: &mut Loaded, path: &Path, message: impl Into<String>) {
    loaded.warnings.push(Warning {
        file: Some(path.to_owned()),
        message: message.into(),
    });
}

/// Puts one file of the user on the manifests in force.
fn apply(loaded: &mut Loaded, path: &Path, table: &toml::Table) -> Applied {
    let Some(id) = table.get("id").and_then(toml::Value::as_str) else {
        warn(loaded, path, "has no id, so neohar leaves it out");
        return Applied::Done;
    };
    let extends = table.get("extends").and_then(toml::Value::as_str);
    let base = match extends {
        Some(parent) if parent == id => {
            warn(loaded, path, "extends itself, so neohar leaves it out");
            return Applied::Done;
        }
        Some(parent) => match loaded.manifests.get(parent) {
            Some(entry) => Some(entry.raw.clone()),
            None => return Applied::Waits,
        },
        None => loaded.manifests.get(id).map(|entry| entry.raw.clone()),
    };
    let mut merged = base.unwrap_or_default();
    deep_merge(&mut merged, table.clone());
    match assemble(&merged, table) {
        Ok((manifest, unknown)) => {
            for key in unknown {
                warn(
                    loaded,
                    path,
                    format!(
                        "the key {key} is not part of the manifest format of this version, so neohar ignores it"
                    ),
                );
            }
            loaded.manifests.insert(
                manifest.id.clone(),
                Entry {
                    manifest,
                    source: Source::User(path.to_owned()),
                    raw: merged,
                },
            );
        }
        Err(error) => warn(
            loaded,
            path,
            format!("is not right, so neohar leaves it out: {error}"),
        ),
    }
    Applied::Done
}

/// The files `*.toml` of a directory, by name. A directory that is not there has none.
fn toml_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml") && path.is_file())
        .collect();
    files.sort();
    files
}

/// The manifests in force: the built-in ones, and then the files in `user_dir` by name. A file
/// that extends a manifest of another file waits until that one is in force.
pub fn load(user_dir: Option<&Path>) -> Loaded {
    let mut loaded = builtin();
    let Some(dir) = user_dir else {
        return loaded;
    };
    let mut pending: Vec<(PathBuf, toml::Table)> = Vec::new();
    for path in toml_files(dir) {
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                warn(&mut loaded, &path, format!("cannot be read: {error}"));
                continue;
            }
        };
        match text.parse::<toml::Table>() {
            Ok(table) => pending.push((path, table)),
            Err(error) => warn(
                &mut loaded,
                &path,
                format!("is not TOML, so neohar leaves it out: {error}"),
            ),
        }
    }
    // A file can extend a file that comes later by name, so the files go round until none moves.
    loop {
        let before = pending.len();
        let mut waiting = Vec::new();
        for (path, table) in pending {
            match apply(&mut loaded, &path, &table) {
                Applied::Done => {}
                Applied::Waits => waiting.push((path, table)),
            }
        }
        pending = waiting;
        if pending.is_empty() || pending.len() == before {
            break;
        }
    }
    for (path, table) in pending {
        let parent = table
            .get("extends")
            .and_then(toml::Value::as_str)
            .unwrap_or("?");
        warn(
            &mut loaded,
            &path,
            format!("extends {parent}, which is not in force, so neohar leaves it out"),
        );
    }
    loaded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::extract;

    /// A directory with the files of a test, removed at the end.
    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("nh-agents-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("a directory");
            Self(path)
        }

        fn write(&self, file: &str, text: &str) {
            fs::write(self.0.join(file), text).expect("a file");
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_built_in_manifests_are_right() {
        let loaded = builtin();
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let claude = loaded.get("claude").expect("claude");
        assert_eq!(claude.name.as_deref(), Some("Claude Code"));
        assert_eq!(claude.priority(), 100);
        assert_eq!(claude.detect.process.len(), 4);
        assert_eq!(claude.capabilities.len(), 1);
        assert_eq!(claude.states["hook:Stop"], "idle");
        let codex = loaded.get("codex").expect("codex");
        assert_eq!(codex.detect.remote_loop.as_deref(), Some("codex_daemon"));
        assert_eq!(
            codex.resume.as_ref().map(|r| r.argv.clone()),
            Some(vec![
                "codex".to_owned(),
                "resume".to_owned(),
                "{resume_id}".to_owned()
            ])
        );
        assert_eq!(codex.yolo.as_ref().map(|y| y.sequences.len()), Some(4));
    }

    #[test]
    fn the_version_expressions_match_what_the_harnesses_print() {
        // The strings that the golden recordings show.
        let loaded = builtin();
        let claude = loaded
            .get("claude")
            .and_then(|m| m.version.clone())
            .expect("a rule");
        let regex = regex::Regex::new(&claude.regex).expect("a regex");
        assert_eq!(
            extract(&regex, "2.1.292 (Claude Code)")
                .map(|v| v.to_string())
                .as_deref(),
            Some("2.1.292")
        );
        let codex = loaded
            .get("codex")
            .and_then(|m| m.version.clone())
            .expect("a rule");
        let regex = regex::Regex::new(&codex.regex).expect("a regex");
        assert_eq!(
            extract(&regex, "codex-cli 0.156.1")
                .map(|v| v.to_string())
                .as_deref(),
            Some("0.156.1")
        );
    }

    #[test]
    fn the_fingerprint_changes_with_the_content_and_only_then() {
        let loaded = builtin();
        let claude = loaded.manifests["claude"].fingerprint();
        assert_eq!(claude.len(), 16);
        assert_eq!(claude, builtin().manifests["claude"].fingerprint());
        assert_ne!(claude, loaded.manifests["codex"].fingerprint());
        let dir = Dir::new("fingerprint");
        dir.write(
            "claude.toml",
            "schema = 1\nid = \"claude\"\n[ui]\ninterrupt_keys = [\"x\"]\n",
        );
        let changed = load(Some(&dir.0));
        assert_ne!(claude, changed.manifests["claude"].fingerprint());
    }

    #[test]
    fn the_json_schema_names_the_keys() {
        let schema = Manifest::json_schema().to_string();
        for key in [
            "min_version",
            "remote_loop",
            "script_regex",
            "argv_with_session",
            "sequences",
        ] {
            assert!(schema.contains(key), "{key} is not in the schema");
        }
    }

    #[test]
    fn a_wrapper_extends_a_manifest_and_keeps_what_it_does_not_change() {
        let dir = Dir::new("wrapper");
        dir.write(
            "cc.toml",
            r#"
schema = 1
id = "cc"
extends = "claude"
[detect]
priority = 200
[[detect.process]]
argv_regex = '(^|/)cc( |$)'
[launch]
argv = ["cc"]
[resume]
argv = ["cc", "--resume", "{resume_id}"]
"#,
        );
        let loaded = load(Some(&dir.0));
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let cc = loaded.get("cc").expect("the wrapper");
        assert_eq!(cc.extends.as_deref(), Some("claude"));
        assert_eq!(cc.priority(), 200);
        // Its own rule replaces the rules of claude, because arrays replace.
        assert_eq!(cc.detect.process.len(), 1);
        assert_eq!(
            cc.detect.process[0].argv_regex.as_deref(),
            Some("(^|/)cc( |$)")
        );
        assert_eq!(
            cc.launch.as_ref().map(|l| l.argv.clone()),
            Some(vec!["cc".to_owned()])
        );
        // The launch keeps the other keys of claude, and the rest is inherited.
        assert_eq!(
            cc.launch
                .as_ref()
                .and_then(|l| l.session_id_format.clone())
                .as_deref(),
            Some("uuid")
        );
        assert_eq!(
            cc.capabilities,
            loaded.get("claude").expect("claude").capabilities
        );
        assert_eq!(
            cc.transcript,
            loaded.get("claude").expect("claude").transcript
        );
        assert_eq!(
            cc.screen.as_ref().map(|s| s.manifest.as_str()),
            Some("claude")
        );
        // Claude itself is as it was.
        assert_eq!(loaded.get("claude").expect("claude").priority(), 100);
        assert!(matches!(loaded.manifests["cc"].source, Source::User(_)));
    }

    #[test]
    fn a_file_with_the_id_of_a_manifest_changes_a_field_and_keeps_the_rest() {
        let dir = Dir::new("override");
        dir.write(
            "claude.toml",
            "schema = 1\nid = \"claude\"\n[launch]\nargv = [\"claude\", \"--model\", \"opus\"]\n[ui]\ninterrupt_keys = [\"ctrl+c\"]\n",
        );
        let loaded = load(Some(&dir.0));
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let claude = loaded.get("claude").expect("claude");
        assert_eq!(
            claude.launch.as_ref().map(|l| l.argv.clone()),
            Some(vec![
                "claude".to_owned(),
                "--model".to_owned(),
                "opus".to_owned()
            ])
        );
        assert_eq!(
            claude.ui.as_ref().map(|u| u.interrupt_keys.clone()),
            Some(vec!["ctrl+c".to_owned()])
        );
        // The quit keys of the built-in stay, and so do the detection rules and the name.
        assert_eq!(claude.ui.as_ref().map(|u| u.quit_keys.len()), Some(2));
        assert_eq!(claude.detect.process.len(), 4);
        assert_eq!(claude.name.as_deref(), Some("Claude Code"));
        assert!(matches!(loaded.manifests["claude"].source, Source::User(_)));
    }

    #[test]
    fn a_file_that_is_not_right_is_left_out_and_the_built_in_stays() {
        let dir = Dir::new("invalid");
        dir.write(
            "a-regex.toml",
            "schema = 1\nid = \"claude\"\n[[detect.process]]\nargv_regex = '('\n",
        );
        dir.write(
            "b-range.toml",
            "schema = 1\nid = \"codex\"\n[[capabilities]]\nversions = 'about two'\n",
        );
        dir.write("c-id.toml", "schema = 1\nid = \"Bad Id\"\n");
        dir.write("d-syntax.toml", "schema = [\n");
        dir.write("e-no-id.toml", "schema = 1\nname = \"x\"\n");
        dir.write(
            "f-version.toml",
            "schema = 1\nid = \"claude\"\n[version]\ncommand = [\"claude\"]\nregex = 'no group'\n",
        );
        dir.write(
            "g-state.toml",
            "schema = 1\nid = \"claude\"\n[states]\n\"hook:Stop\" = \"asleep\"\n",
        );
        dir.write("h-schema.toml", "schema = 2\nid = \"claude\"\n");
        let loaded = load(Some(&dir.0));
        assert_eq!(loaded.warnings.len(), 8, "{:#?}", loaded.warnings);
        let text: Vec<String> = loaded.warnings.iter().map(ToString::to_string).collect();
        assert!(
            text.iter()
                .any(|w| w.contains("a-regex.toml") && w.contains("regular expression"))
        );
        assert!(
            text.iter()
                .any(|w| w.contains("b-range.toml") && w.contains("range of versions"))
        );
        assert!(
            text.iter()
                .any(|w| w.contains("c-id.toml") && w.contains("id"))
        );
        assert!(
            text.iter()
                .any(|w| w.contains("d-syntax.toml") && w.contains("not TOML"))
        );
        assert!(
            text.iter()
                .any(|w| w.contains("e-no-id.toml") && w.contains("no id"))
        );
        assert!(
            text.iter()
                .any(|w| w.contains("f-version.toml") && w.contains("group named v"))
        );
        assert!(
            text.iter()
                .any(|w| w.contains("g-state.toml") && w.contains("not a state"))
        );
        assert!(
            text.iter()
                .any(|w| w.contains("h-schema.toml") && w.contains("schema 2"))
        );
        // The built-ins are as they were, and from the built-in source.
        assert!(matches!(loaded.manifests["claude"].source, Source::Builtin));
        assert!(matches!(loaded.manifests["codex"].source, Source::Builtin));
        assert_eq!(loaded.manifests.len(), 2);
    }

    #[test]
    fn an_unknown_key_is_a_warning_and_the_file_still_loads() {
        let dir = Dir::new("unknown");
        dir.write(
            "claude.toml",
            "schema = 1\nid = \"claude\"\nfuture = 1\n[launch]\nargv = [\"claude\"]\nnext = { a = 1 }\n",
        );
        let loaded = load(Some(&dir.0));
        let text: Vec<String> = loaded.warnings.iter().map(ToString::to_string).collect();
        assert_eq!(text.len(), 2, "{text:?}");
        assert!(text.iter().any(|w| w.contains("the key future")));
        assert!(text.iter().any(|w| w.contains("the key launch.next")));
        assert!(matches!(loaded.manifests["claude"].source, Source::User(_)));
    }

    #[test]
    fn extends_works_across_files_in_any_order_and_a_missing_parent_is_a_warning() {
        let dir = Dir::new("order");
        // `a` comes first by name, and extends `b`, which extends claude.
        dir.write(
            "a.toml",
            "schema = 1\nid = \"a\"\nextends = \"b\"\nname = \"A\"\n",
        );
        dir.write(
            "b.toml",
            "schema = 1\nid = \"b\"\nextends = \"claude\"\nname = \"B\"\n",
        );
        dir.write("c.toml", "schema = 1\nid = \"c\"\nextends = \"nothing\"\n");
        dir.write("d.toml", "schema = 1\nid = \"d\"\nextends = \"d\"\n");
        dir.write("e.toml", "schema = 1\nid = \"e\"\nextends = \"f\"\n");
        dir.write("f.toml", "schema = 1\nid = \"f\"\nextends = \"e\"\n");
        let loaded = load(Some(&dir.0));
        let a = loaded.get("a").expect("a");
        assert_eq!(a.name.as_deref(), Some("A"));
        assert_eq!(a.launch, loaded.get("claude").expect("claude").launch);
        assert!(loaded.get("b").is_some());
        for id in ["c", "d", "e", "f"] {
            assert!(loaded.get(id).is_none(), "{id}");
        }
        assert_eq!(loaded.warnings.len(), 4, "{:#?}", loaded.warnings);
    }

    #[test]
    fn a_directory_that_is_not_there_gives_only_the_built_ins() {
        let loaded = load(Some(Path::new("/nonexistent/neohar/harnesses")));
        assert!(loaded.warnings.is_empty());
        assert_eq!(loaded.manifests.len(), 2);
        let none = load(None);
        assert_eq!(none.manifests.len(), 2);
    }

    #[test]
    fn the_built_ins_can_be_written_for_reference() {
        let dir = Dir::new("reference");
        let target = dir.0.join("harnesses").join("builtin");
        write_builtins(&target).expect("written");
        let text = fs::read_to_string(target.join("claude.toml")).expect("a file");
        assert!(text.contains("id = \"claude\""));
        // The copy is a manifest that loads as it is.
        let copy: toml::Table = text.parse().expect("TOML");
        assert!(assemble(&copy, &copy).is_ok());
    }
}
