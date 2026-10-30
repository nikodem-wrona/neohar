use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

use serde_json::json;

use super::*;
use crate::ErrorCode;

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nh-config-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a directory");
    dir
}

fn store_with(name: &str, text: Option<&str>) -> (ConfigStore, PathBuf) {
    let dir = dir(name);
    let path = dir.join(FILE_NAME);
    if let Some(text) = text {
        fs::write(&path, text).expect("a file");
    }
    (ConfigStore::load(path), dir)
}

fn source(store: &ConfigStore, section: &str, key: &str) -> SettingSource {
    store.sections()[section].entries[key].source
}

#[test]
fn without_a_file_every_setting_has_its_default_and_no_warning() {
    let (store, dir) = store_with("none", None);
    assert_eq!(*store.config(), Config::default());
    assert!(store.warnings().is_empty());
    let sections = store.sections();
    assert_eq!(
        sections.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "agents",
            "approvals",
            "daemon",
            "events",
            "git",
            "keys",
            "phone",
            "policy",
            "scrollback",
            "theme",
            "ui",
            "workflows"
        ]
    );
    assert_eq!(sections["approvals"].entries["mode"].value, json!("auto"));
    assert_eq!(
        sections["approvals"].entries["gate_timeout"].value,
        json!("30m")
    );
    assert_eq!(
        sections["approvals"].entries["claude.persist_always"].value,
        json!(false)
    );
    assert!(sections.values().all(|s| {
        s.entries
            .values()
            .all(|e| e.source == SettingSource::Default)
    }));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_file_is_merged_over_the_defaults_and_marks_its_keys() {
    let (store, dir) = store_with(
        "merge",
        Some("[approvals]\nmode = \"gate\"\n\n[phone]\nfont_size = 14\nport = 50000\n"),
    );
    let config = store.config();
    assert_eq!(config.approvals.mode, ApprovalMode::Gate);
    assert_eq!(config.phone.font_size, 14);
    assert_eq!(config.phone.port, 50_000);
    assert_eq!(
        config.phone.contact, "",
        "a key of a section that the file has keeps its default"
    );
    assert_eq!(config.agents, Config::default().agents);
    assert_eq!(source(&store, "approvals", "mode"), SettingSource::File);
    assert_eq!(source(&store, "phone", "port"), SettingSource::File);
    assert_eq!(source(&store, "phone", "contact"), SettingSource::Default);
    assert_eq!(source(&store, "agents", "shims"), SettingSource::Default);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_key_that_this_version_does_not_know_is_a_warning_and_nothing_else() {
    let (store, dir) = store_with(
        "unknown",
        Some("[approvals]\nmood = \"gate\"\nmode = \"observe\"\n\n[future]\nthing = 1\n"),
    );
    assert_eq!(store.config().approvals.mode, ApprovalMode::Observe);
    let warnings = store.warnings();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(warnings.iter().any(|w| w.contains("approvals.mood")));
    assert!(warnings.iter().any(|w| w.contains("future.thing")));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_key_of_an_older_version_loads_without_a_warning() {
    let old = "[input]\nconfirm_multiline_paste_to_agents = false\n\n[search]\n\
               index_transcripts = true\n\n[update]\nmanifest_check = false\n\n\
               [tasks]\nports = 2\nport_range = [41000, 41999]\nsetup = [\"bun install\"]\n\n\
               [tasks.env]\nAPP_ENV = \"dev\"\n\n[keys]\nsearch_open = \"mod+shift+f\"\n\n\
               [approvals]\nmode = \"observe\"\n";
    let (store, dir) = store_with("retired", Some(old));
    assert_eq!(store.config().approvals.mode, ApprovalMode::Observe);
    assert!(store.warnings().is_empty(), "{:?}", store.warnings());
    let sections = store.sections();
    for section in ["input", "search", "update", "tasks"] {
        assert!(!sections.contains_key(section), "{section}");
    }
    // A change of the file with the old keys reloads like any other, and so does a set.
    fs::write(store.path(), format!("{old}\n[agents]\nshims = false\n")).expect("a file");
    let report = store.reload();
    assert_eq!(report.changed, ["agents.shims"]);
    assert!(report.errors.is_empty() && store.warnings().is_empty());
    store.set("phone.font_size", json!(14)).expect("a set");
    assert_eq!(store.config().phone.font_size, 14);
    assert!(store.warnings().is_empty(), "{:?}", store.warnings());
    // The old keys are no settings: a set of one is refused.
    assert_eq!(
        store.set("input.confirm_multiline_paste_to_agents", json!(true)),
        Err(ConfigError::UnknownKey(
            "input.confirm_multiline_paste_to_agents".to_owned()
        ))
    );
    assert_eq!(
        store.set("tasks.ports", json!(1)),
        Err(ConfigError::UnknownKey("tasks.ports".to_owned()))
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_bad_file_at_the_start_means_the_defaults_and_a_warning() {
    let (store, dir) = store_with("bad-start", Some("[approvals]\nmode = \"sometimes\"\n"));
    assert_eq!(*store.config(), Config::default());
    let warnings = store.warnings();
    assert!(
        warnings[0].contains("config.toml") && warnings[0].contains("sometimes"),
        "{warnings:?}"
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_reload_picks_up_a_change_and_names_the_settings_that_changed() {
    let (store, dir) = store_with("reload", Some("[approvals]\nmode = \"gate\"\n"));
    fs::write(
        store.path(),
        "[approvals]\nmode = \"observe\"\n\n[scrollback]\nmax_bytes_per_pane = 1000000\n",
    )
    .expect("a file");
    let report = store.reload();
    assert_eq!(
        report.changed,
        ["approvals.mode", "scrollback.max_bytes_per_pane"]
    );
    assert!(report.errors.is_empty());
    assert_eq!(store.config().approvals.mode, ApprovalMode::Observe);
    assert_eq!(store.config().scrollback.max_bytes_per_pane, 1_000_000);
    // Nothing changed: nothing to report, and the file is not parsed again.
    assert_eq!(store.reload(), ReloadReport::default());
    // A key that leaves the file is back at its default.
    fs::write(store.path(), "").expect("a file");
    assert_eq!(
        store.reload().changed,
        ["approvals.mode", "scrollback.max_bytes_per_pane"]
    );
    assert_eq!(*store.config(), Config::default());
    // The file goes away.
    fs::write(store.path(), "[agents]\nshims = false\n").expect("a file");
    assert_eq!(store.reload().changed, ["agents.shims"]);
    fs::remove_file(store.path()).expect("removed");
    assert_eq!(store.reload().changed, ["agents.shims"]);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_bad_file_after_a_good_one_keeps_the_last_good_configuration_and_warns() {
    let (store, dir) = store_with("keep-good", Some("[approvals]\nmode = \"gate\"\n"));
    fs::write(store.path(), "[approvals\nmode = ").expect("a file");
    let report = store.reload();
    assert!(report.changed.is_empty());
    assert_eq!(report.errors.len(), 1);
    assert!(
        report.errors[0].starts_with("config.toml:"),
        "{:?}",
        report.errors
    );
    assert_eq!(
        store.config().approvals.mode,
        ApprovalMode::Gate,
        "the last good file is in force"
    );
    assert_eq!(store.warnings(), report.errors);
    // The same bad text is not reported again.
    assert_eq!(store.reload(), ReloadReport::default());
    // A wrong value is bad too.
    fs::write(store.path(), "[events]\nsync_retention = \"forever\"\n").expect("a file");
    let report = store.reload();
    assert!(
        report.errors[0].contains("sync_retention") || report.errors[0].contains("forever"),
        "{:?}",
        report.errors
    );
    assert_eq!(store.config().approvals.mode, ApprovalMode::Gate);
    // A good file clears the warning.
    fs::write(store.path(), "[approvals]\nmode = \"observe\"\n").expect("a file");
    let report = store.reload();
    assert_eq!(report.changed, ["approvals.mode"]);
    assert!(store.warnings().is_empty());
    let _ = fs::remove_dir_all(dir);
}

const ORIGINAL: &str = "# My neohar config
[approvals]
# how requests are handled
mode = \"observe\"   # keep this
gate_timeout = \"10m\"

[phone]
font_size = 14 # larger text
";

#[test]
fn set_keeps_the_comments_and_the_order_of_the_file() {
    let (store, dir) = store_with("golden", Some(ORIGINAL));
    store.set("approvals.mode", json!("gate")).expect("a set");
    let text = fs::read_to_string(store.path()).expect("the file");
    assert_eq!(
        text,
        "# My neohar config
[approvals]
# how requests are handled
mode = \"gate\"   # keep this
gate_timeout = \"10m\"

[phone]
font_size = 14 # larger text
"
    );
    // A key that the file has not: it goes to the end of its table.
    store
        .set("phone.contact", json!("mailto:ada@example.com"))
        .expect("a set");
    let text = fs::read_to_string(store.path()).expect("the file");
    assert!(
        text.contains("font_size = 14 # larger text\ncontact = \"mailto:ada@example.com\"\n"),
        "{text}"
    );
    // A section that the file has not: a new table at the end.
    store
        .set("events.sync_retention", json!("2d"))
        .expect("a set");
    let text = fs::read_to_string(store.path()).expect("the file");
    assert!(
        text.ends_with("[events]\nsync_retention = \"2d\"\n"),
        "{text}"
    );
    assert!(text.starts_with("# My neohar config\n[approvals]\n"));
    // The comments of the first edit are still there.
    assert!(text.contains("# how requests are handled") && text.contains("# keep this"));
    assert_eq!(
        store.config().events.sync_retention.0,
        Duration::from_secs(2 * 86_400)
    );
    assert_eq!(store.config().phone.contact, "mailto:ada@example.com");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn set_writes_a_file_that_did_not_exist_with_mode_0600() {
    use std::os::unix::fs::PermissionsExt;
    let dir = dir("create").join("deeper");
    let store = ConfigStore::load(dir.join(FILE_NAME));
    store.set("agents.shims", json!(false)).expect("a set");
    assert_eq!(
        fs::read_to_string(store.path()).expect("the file"),
        "[agents]\nshims = false\n"
    );
    let mode = |p: &Path| fs::metadata(p).expect("metadata").permissions().mode() & 0o777;
    assert_eq!(mode(store.path()), 0o600);
    assert_eq!(mode(&dir), 0o700);
    let names: Vec<_> = fs::read_dir(&dir)
        .expect("a directory")
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(names.len(), 1, "no temporary file stays: {names:?}");
    let _ = fs::remove_dir_all(dir.parent().expect("a parent"));
}

#[test]
fn set_checks_the_whole_result_before_it_writes() {
    let (store, dir) = store_with("validate", Some(ORIGINAL));
    for (key, value) in [
        ("approvals.mode", json!("sometimes")),
        ("approvals.mode", json!(3)),
        ("events.sync_retention", json!("7")),
        ("phone.font_size", json!("two")),
        ("phone.font_size", json!(300)),
        ("phone.port", json!([1])),
        ("agents.shims", json!({"a": 1})),
    ] {
        let error = store
            .set(key, value.clone())
            .expect_err(&format!("{key} = {value}"));
        assert!(matches!(error, ConfigError::Invalid(_)), "{key}: {error}");
        assert_eq!(
            fs::read_to_string(store.path()).expect("the file"),
            ORIGINAL,
            "{key} changed the file"
        );
    }
    assert_eq!(
        store.set("approvals.nothing", json!(1)),
        Err(ConfigError::UnknownKey("approvals.nothing".to_owned()))
    );
    assert_eq!(
        store.set("approvals", json!(1)),
        Err(ConfigError::UnknownKey("approvals".to_owned())),
        "a section is not a setting"
    );
    assert_eq!(store.config().approvals.mode, ApprovalMode::Observe);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn set_refuses_to_edit_a_file_with_a_syntax_error() {
    let (store, dir) = store_with("syntax", Some("[approvals\nmode = "));
    let before = fs::read_to_string(store.path()).expect("the file");
    let error = store
        .set("agents.shims", json!(false))
        .expect_err("a bad file");
    assert!(matches!(error, ConfigError::Syntax(_)), "{error}");
    assert_eq!(fs::read_to_string(store.path()).expect("the file"), before);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn null_removes_the_key_and_an_empty_table_goes_with_it() {
    let (store, dir) = store_with(
        "reset",
        Some("[approvals]\nmode = \"gate\"\n\n[phone]\nfont_size = 14\ncontact = \"x\"\n"),
    );
    store.set("approvals.mode", Value::Null).expect("a reset");
    let text = fs::read_to_string(store.path()).expect("the file");
    assert!(!text.contains("approvals"), "{text}");
    assert_eq!(store.config().approvals.mode, ApprovalMode::Auto);
    assert_eq!(source(&store, "approvals", "mode"), SettingSource::Default);
    store.set("phone.font_size", Value::Null).expect("a reset");
    let text = fs::read_to_string(store.path()).expect("the file");
    assert_eq!(text, "\n[phone]\ncontact = \"x\"\n");
    // A key that is not in the file is already at its default.
    store
        .set("events.audit_retention", Value::Null)
        .expect("nothing to reset");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_source_is_ui_while_the_file_has_the_value_that_set_wrote() {
    let (store, dir) = store_with("sources", Some("[approvals]\nmode = \"gate\"\n"));
    assert_eq!(source(&store, "approvals", "mode"), SettingSource::File);
    store
        .set("approvals.mode", json!("observe"))
        .expect("a set");
    store.set("agents.shims", json!(false)).expect("a set");
    assert_eq!(source(&store, "approvals", "mode"), SettingSource::Ui);
    assert_eq!(source(&store, "agents", "shims"), SettingSource::Ui);
    // The user edits the file by hand: a key with another value is the file's now.
    let text = fs::read_to_string(store.path())
        .expect("the file")
        .replace("observe", "auto");
    fs::write(store.path(), text).expect("a file");
    store.reload();
    assert_eq!(source(&store, "approvals", "mode"), SettingSource::File);
    assert_eq!(
        source(&store, "agents", "shims"),
        SettingSource::Ui,
        "this key did not change"
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_settings_projection_has_a_row_for_each_section() {
    let (store, dir) = store_with("projection", Some("[approvals]\nmode = \"gate\"\n"));
    let hub = ProjectionHub::new();
    store.publish(&hub);
    hub.flush();
    let published = hub.snapshot(Projection::Settings);
    assert_eq!(published.order.len(), 12);
    let approvals = &published.items["approvals"];
    assert_eq!(
        approvals["entries"]["mode"],
        json!({"value": "gate", "source": "file"})
    );
    assert_eq!(
        approvals["entries"]["gate_timeout"],
        json!({"value": "30m", "source": "default"})
    );
    store
        .set("approvals.mode", json!("observe"))
        .expect("a set");
    store.publish(&hub);
    hub.flush();
    assert_eq!(
        hub.snapshot(Projection::Settings).items["approvals"]["entries"]["mode"]["source"],
        "ui"
    );
    let _ = fs::remove_dir_all(dir);
}

/// How long a test waits for the system to set up or stop a watch. On macOS, `fseventsd` does
/// these operations for all processes one at a time. Each operation of a test binary takes more
/// time when the directory of the binary has more files: about 1 s with the 600 000 files that
/// `target/debug/deps` collects over months. Under `mise run test`, many cores start watches at
/// the same time, and one operation waited more than 30 s.
const READY: Duration = Duration::from_secs(120);

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done() {
        assert!(Instant::now() < deadline, "waited for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn the_watcher_reloads_after_the_file_settles_and_reports_once() {
    let (store, dir) = store_with("watch", Some("[approvals]\nmode = \"gate\"\n"));
    let store = Arc::new(store);
    let reports = Arc::new(Mutex::new(Vec::<ReloadReport>::new()));
    let sink = Arc::clone(&reports);
    let watcher = store
        .watch(move |report| {
            sink.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(report.clone())
        })
        .expect("the thread starts");
    // The system needs a while to set the watch up, much longer on a machine that is busy. The
    // thread then looks at the file once more after `START_LAG`. Start the writes after both, so
    // that the test sees the events of the watch and not those reads.
    watcher.wait_ready(READY).expect("the watch is set up");
    std::thread::sleep(START_LAG + Duration::from_millis(300));
    // Many quick writes: one reload, with the last content.
    for n in 0..6 {
        let mode = if n % 2 == 0 { "observe" } else { "auto" };
        fs::write(
            store.path(),
            format!(
                "[approvals]\nmode = \"{mode}\"\n[phone]\nfont_size = {}\n",
                n + 1
            ),
        )
        .expect("a file");
        std::thread::sleep(Duration::from_millis(20));
    }
    wait_until("the reload", || {
        !reports
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
    });
    std::thread::sleep(Duration::from_millis(600));
    let seen = reports
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert!(
        seen[0].changed.contains(&"phone.font_size".to_owned()),
        "{seen:?}"
    );
    assert_eq!(store.config().phone.font_size, 6);
    assert_eq!(store.config().approvals.mode, ApprovalMode::Auto);

    // A bad file is reported as an error, and the good state stays.
    fs::write(store.path(), "[approvals\n").expect("a file");
    wait_until("the error", || {
        reports.lock().unwrap_or_else(PoisonError::into_inner).len() == 2
    });
    assert_eq!(store.config().phone.font_size, 6);
    assert!(
        !reports.lock().unwrap_or_else(PoisonError::into_inner)[1]
            .errors
            .is_empty()
    );

    // The write of `set` does not make a second report: the text is known already.
    fs::write(store.path(), "[phone]\nfont_size = 4\n").expect("a file");
    wait_until("the third reload", || {
        reports.lock().unwrap_or_else(PoisonError::into_inner).len() == 3
    });
    store.set("phone.font_size", json!(5)).expect("a set");
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(
        reports.lock().unwrap_or_else(PoisonError::into_inner).len(),
        3
    );

    watcher.stop();
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_watcher_ends_when_it_stops_and_dropping_does_not_wait() {
    let (store, dir) = store_with("drop", None);
    let store = Arc::new(store);
    // The call returns before the operating system has set the watch up.
    let started = Instant::now();
    let watcher = store.watch(|_| {}).expect("the thread starts");
    let to_start = started.elapsed();
    watcher.wait_ready(READY).expect("the watch is set up");
    let stopping = Instant::now();
    watcher.stop();
    eprintln!(
        "watch returned in {to_start:?}, stop took {:?}",
        stopping.elapsed()
    );
    // The stop of a watch waits for `fseventsd` too (see `READY`). The test finds a hang, and not
    // a slow stop.
    assert!(stopping.elapsed() < READY);
    // A watcher that is dropped at once does not block the caller.
    let dropping = Instant::now();
    drop(store.watch(|_| {}).expect("the thread starts"));
    assert!(
        dropping.elapsed() < Duration::from_secs(1),
        "{:?}",
        dropping.elapsed()
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_change_made_before_the_watch_is_ready_is_found_after_it() {
    let (store, dir) = store_with("catch-up", Some("[agents]\nshims = true\n"));
    let store = Arc::new(store);
    let reports = Arc::new(Mutex::new(Vec::<ReloadReport>::new()));
    let sink = Arc::clone(&reports);
    // The file changes right after the call, while the watch is not set up.
    let watcher = store
        .watch(move |report| {
            sink.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(report.clone())
        })
        .expect("the thread starts");
    fs::write(store.path(), "[agents]\nshims = false\n").expect("a file");
    watcher.wait_ready(READY).expect("the watch is set up");
    wait_until("the report of the change", || {
        !reports
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
    });
    assert!(!store.config().agents.shims);
    watcher.stop();
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_polling_watch_reloads_a_file_that_changed_in_place() {
    // The watch of `nh`, which has no FSEvents.
    let (store, dir) = store_with("poll", Some("[phone]\nfont_size = 1\n"));
    let store = Arc::new(store);
    let reports = Arc::new(Mutex::new(Vec::<ReloadReport>::new()));
    let sink = Arc::clone(&reports);
    let watcher = store
        .watch_with(crate::watch::Kind::Poll, move |report| {
            sink.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(report.clone());
        })
        .expect("the thread starts");
    watcher.wait_ready(READY).expect("the watch is set up");
    std::thread::sleep(START_LAG + Duration::from_millis(300));
    fs::write(store.path(), "[phone]\nfont_size = 3\n").expect("a write in place");
    wait_until("the reload", || store.config().phone.font_size == 3);
    assert!(
        reports
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|r| r.changed.contains(&"phone.font_size".to_owned()))
    );
    watcher.stop();
    let _ = fs::remove_dir_all(dir);
}

fn labels(store: &ConfigStore) -> Vec<String> {
    store.machines().into_iter().map(|m| m.label).collect()
}

#[test]
fn the_machines_of_the_file_keep_the_order_of_the_file_and_get_their_defaults() {
    let (store, dir) = store_with(
        "machines",
        Some(
            "[machines.studio]\nssh = \"me@studio\"\nconnect = \"launch\"\n\n\
             [machines.build]\nssh = \"build.example.com\"\nsession = \"ci\"\n",
        ),
    );
    assert!(store.warnings().is_empty(), "{:?}", store.warnings());
    // Not alphabetical: `studio` comes first in the file.
    assert_eq!(labels(&store), ["studio", "build"]);
    let machines = store.machines();
    assert_eq!(machines[0].machine.ssh, "me@studio");
    assert_eq!(machines[0].machine.session, "default");
    assert_eq!(machines[0].machine.connect, ConnectMode::Launch);
    assert_eq!(machines[1].machine.session, "ci");
    assert_eq!(machines[1].machine.connect, ConnectMode::Manual);
    // The settings projection shows them as a section, with the source of each key.
    let sections = store.sections();
    assert_eq!(
        sections["machines"].entries["studio.ssh"].value,
        json!("me@studio")
    );
    assert_eq!(
        source(&store, "machines", "studio.ssh"),
        SettingSource::File
    );
    assert_eq!(
        source(&store, "machines", "build.connect"),
        SettingSource::Default
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_machine_that_cannot_work_is_ignored_with_a_warning_and_the_others_stay() {
    let (store, dir) = store_with(
        "machines-bad",
        Some(
            "[machines.local]\nssh = \"x\"\n\n[machines.Big]\nssh = \"x\"\n\n\
             [machines.empty]\nssh = \" \"\n\n[machines.ok]\nssh = \"ok\"\nnope = 1\n",
        ),
    );
    assert_eq!(labels(&store), ["ok"]);
    let warnings = store.warnings().join("\n");
    for expected in [
        "[machines.local] is ignored",
        "[machines.Big] is ignored",
        "[machines.empty] is ignored: ssh is empty",
        "machines.ok.nope is not a setting of this version",
    ] {
        assert!(warnings.contains(expected), "{expected:?} in {warnings}");
    }
    assert!(
        !warnings.contains("machines.ok.ssh"),
        "a field of a machine is not unknown: {warnings}"
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn set_adds_changes_and_removes_a_machine_and_checks_the_label() {
    let (store, dir) = store_with("machines-set", Some("[machines.first]\nssh = \"one\"\n"));
    store
        .set(
            "machines.second",
            json!({"ssh": "two", "session": "s2", "connect": "launch"}),
        )
        .expect("a whole machine");
    assert_eq!(labels(&store), ["first", "second"]);
    store
        .set("machines.first.connect", json!("launch"))
        .expect("one field");
    assert_eq!(store.machines()[0].machine.connect, ConnectMode::Launch);
    assert_eq!(
        source(&store, "machines", "first.connect"),
        SettingSource::Ui
    );
    // The order of the file stays after the writes.
    let text = fs::read_to_string(store.path()).expect("the file");
    assert!(text.find("first").expect("first") < text.find("second").expect("second"));

    for (key, value) in [
        ("machines.local", json!({"ssh": "x"})),
        ("machines.Upper.ssh", json!("x")),
        ("machines.a_b.ssh", json!("x")),
        ("machines.first.nope", json!("x")),
        ("machines.first.ssh.deeper", json!("x")),
        ("machines.first", json!("a text")),
        ("machines.third", json!({"session": "no-ssh"})),
        ("machines.third", json!({"ssh": "x", "nope": 1})),
        ("machines.first.connect", json!("sometimes")),
    ] {
        let error = store.set(key, value.clone()).expect_err(key);
        let rpc = RpcError::from(error);
        assert!(
            rpc.code == ErrorCode::InvalidParams,
            "{key} = {value}: {rpc}"
        );
    }
    assert_eq!(labels(&store), ["first", "second"], "nothing was written");

    store.set("machines.first", Value::Null).expect("removed");
    assert_eq!(labels(&store), ["second"]);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_observer_hears_a_change_of_the_machines_and_nothing_else() {
    let (store, dir) = store_with("machines-observer", None);
    let seen: Arc<Mutex<Vec<Vec<String>>>> = Arc::default();
    {
        let seen = Arc::clone(&seen);
        store.on_machines_changed(Arc::new(move |machines| {
            seen.lock()
                .expect("lock")
                .push(machines.into_iter().map(|m| m.label).collect());
        }));
    }
    store
        .set("approvals.mode", json!("gate"))
        .expect("an ordinary setting");
    assert!(seen.lock().expect("lock").is_empty(), "no machine changed");
    store
        .set("machines.studio", json!({"ssh": "s"}))
        .expect("a machine");
    // The same value again is no change.
    store
        .set("machines.studio.ssh", json!("s"))
        .expect("the same");
    // A hand edit and a reload.
    fs::write(
        store.path(),
        "[machines.studio]\nssh = \"s\"\n[machines.other]\nssh = \"o\"\n",
    )
    .expect("an edit");
    assert!(store.reload().errors.is_empty());
    store.set("machines.studio", Value::Null).expect("removed");
    assert_eq!(
        *seen.lock().expect("lock"),
        [vec!["studio"], vec!["studio", "other"], vec!["other"]]
    );
    let _ = fs::remove_dir_all(dir);
}
