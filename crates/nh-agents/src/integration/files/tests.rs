//! The installer on real files.

use std::path::PathBuf;

use super::*;
use crate::integration::{claude, hooks::Presence};

const BIN: &str = "/Users/x/.local/bin/nh";

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("nh-integration-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("a directory");
        Self(path)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.0)
            .expect("a directory")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const USER: &str = "{\n  \"model\": \"opus\",\n  \"env\": { \"A\":   \"b\" }\n}\n";

#[test]
fn a_missing_file_is_created_with_its_directory() {
    let dir = Dir::new("create");
    let path = dir.file("nested").join(".claude").join("settings.json");
    let outcome = install_hooks(&claude::SPEC, &path, BIN, false).expect("an outcome");
    assert_eq!(outcome.action, Action::Created);
    assert_eq!(outcome.backup, None);
    assert!(outcome.diff.starts_with("--- /dev/null\n+++ "));
    let text = fs::read_to_string(&path).expect("the file");
    assert!(text.contains(" hook claude SessionStart\""), "{text}");
    assert!(
        inspect_hooks(&claude::SPEC, &path, BIN)
            .expect("status")
            .iter()
            .all(|state| state.presence == Presence::Current)
    );
}

#[test]
fn a_change_makes_a_backup_first_and_a_second_run_changes_nothing() {
    let dir = Dir::new("backup");
    let path = dir.file("settings.json");
    fs::write(&path, USER).expect("a file");
    let first = install_hooks(&claude::SPEC, &path, BIN, false).expect("an outcome");
    assert_eq!(first.action, Action::Modified);
    let backup = first.backup.expect("a backup");
    assert!(
        backup
            .to_string_lossy()
            .contains("settings.json.neohar-bak-")
    );
    assert_eq!(fs::read_to_string(&backup).expect("the backup"), USER);
    let written = fs::read_to_string(&path).expect("the file");
    assert!(written.contains("\"env\": { \"A\":   \"b\" }"));
    assert!(first.diff.contains("+  \"hooks\": {"));
    // Nothing changes the second time, and no second backup appears.
    let names = dir.names();
    let second = install_hooks(&claude::SPEC, &path, BIN, false).expect("an outcome");
    assert_eq!(second.action, Action::Unchanged);
    assert_eq!(second.backup, None);
    assert_eq!(second.diff, "");
    assert_eq!(dir.names(), names);
    assert_eq!(fs::read_to_string(&path).expect("the file"), written);
}

#[test]
fn a_dry_run_shows_the_diff_and_writes_nothing() {
    let dir = Dir::new("dry");
    let path = dir.file("settings.json");
    fs::write(&path, USER).expect("a file");
    let outcome = install_hooks(&claude::SPEC, &path, BIN, true).expect("an outcome");
    assert_eq!(outcome.action, Action::Modified);
    assert!(
        outcome
            .diff
            .contains(" hook claude PreToolUse\",\n+            \"timeout\": 1800\n"),
        "{}",
        outcome.diff
    );
    assert_eq!(fs::read_to_string(&path).expect("the file"), USER);
    assert_eq!(dir.names(), ["settings.json"]);
    let missing =
        install_hooks(&claude::SPEC, &dir.file("none.json"), BIN, true).expect("an outcome");
    assert_eq!(missing.action, Action::Created);
    assert!(!dir.file("none.json").exists());
}

#[test]
fn uninstall_brings_the_file_back_and_a_missing_file_is_unchanged() {
    let dir = Dir::new("uninstall");
    let path = dir.file("settings.json");
    fs::write(&path, USER).expect("a file");
    install_hooks(&claude::SPEC, &path, BIN, false).expect("an outcome");
    let out = uninstall_hooks(&claude::SPEC, &path, false).expect("an outcome");
    assert_eq!(out.action, Action::Modified);
    assert_eq!(fs::read_to_string(&path).expect("the file"), USER);
    // Nothing of ours is left, so a second uninstall changes nothing.
    assert_eq!(
        uninstall_hooks(&claude::SPEC, &path, false)
            .expect("an outcome")
            .action,
        Action::Unchanged
    );
    assert_eq!(
        uninstall_hooks(&claude::SPEC, &dir.file("none.json"), false)
            .expect("an outcome")
            .action,
        Action::Unchanged
    );
}

#[test]
fn a_file_that_changed_after_the_read_is_not_overwritten() {
    let dir = Dir::new("changed");
    let path = dir.file("settings.json");
    fs::write(&path, "{\"a\": 2}\n").expect("a file");
    // The writer read `{"a": 1}`, and the file says something else now.
    let error = write_atomic(&path, "{\"a\": 3}\n", Some("{\"a\": 1}\n")).expect_err("refused");
    assert!(matches!(error, Error::Changed(_)), "{error:?}");
    assert_eq!(fs::read_to_string(&path).expect("the file"), "{\"a\": 2}\n");
    assert_eq!(dir.names(), ["settings.json"], "no temporary file stays");
    // A file that appeared when none was expected is refused as well.
    let error = write_atomic(&path, "x", None).expect_err("refused");
    assert!(matches!(error, Error::Changed(_)));
}

#[test]
fn the_mode_of_the_file_stays() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Dir::new("mode");
    let path = dir.file("settings.json");
    fs::write(&path, USER).expect("a file");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("a mode");
    install_hooks(&claude::SPEC, &path, BIN, false).expect("an outcome");
    let mode = fs::metadata(&path).expect("a file").permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn a_file_that_is_not_json_is_refused_and_stays() {
    let dir = Dir::new("bad");
    let path = dir.file("settings.json");
    fs::write(&path, "// comment\n{}\n").expect("a file");
    let error = install_hooks(&claude::SPEC, &path, BIN, false).expect_err("refused");
    assert!(matches!(error, Error::Format(_)), "{error:?}");
    assert_eq!(
        fs::read_to_string(&path).expect("the file"),
        "// comment\n{}\n"
    );
    assert_eq!(dir.names(), ["settings.json"]);
    let binary = dir.file("binary.json");
    fs::write(&binary, [0xff, 0xfe, 0x00]).expect("a file");
    assert!(install_hooks(&claude::SPEC, &binary, BIN, false).is_err());
}
