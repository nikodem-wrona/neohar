//! The configuration: `~/.config/neohar/config.toml`.
//!
//! - **Defaults** come from `defaults.toml`, compiled in. The user file holds only what the user
//!   changed, and it is merged over the defaults.
//! - **Provenance.** Every key has a source: `default`, `file`, or `ui`. A key is `ui` when
//!   `config.set` wrote its value in this session and the file still has that value.
//! - **`config.set`** edits the file with `toml_edit`, so comments and order stay. It checks the
//!   whole result against the typed [`Config`] before it writes, and it writes through a temporary
//!   file and a rename. A `null` removes the key, so that the default is in force again.
//! - **Reload.** A watcher sees a change of the file, waits 200 ms for it to settle, and reloads.
//!   If the file is not valid, the last good configuration stays, and the error is a warning.
//! - **The `settings` projection** has one key for each section, with the value and the source of
//!   each setting.

pub(crate) mod schema;

use std::{
    collections::BTreeMap,
    fs::{self, DirBuilder, File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        mpsc::{Receiver, RecvTimeoutError, Sender, channel},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use arc_swap::ArcSwap;
use nh_proto::api::{
    model::{SettingEntry, SettingSource, SettingsSection},
    projections::Projection,
};
use serde_json::Value;

pub use schema::{
    ApprovalMode, Billing, Config, ConnectMode, MAX_LABEL, Machine, ResumeOnLaunch, Span,
    valid_machine_label,
};

use crate::{ErrorCode, error::RpcError, projection::ProjectionHub};

/// The name of the file.
pub const FILE_NAME: &str = "config.toml";

/// How long the watcher waits for a change of the file to settle.
pub const DEBOUNCE: Duration = Duration::from_millis(200);

/// How long after the setup of a watch the file is read once more (see [`ConfigStore::watch`]).
pub const START_LAG: Duration = Duration::from_millis(500);

/// Why a `set` did not work.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("no setting {0:?}")]
    UnknownKey(String),
    #[error("{0}")]
    Invalid(String),
    #[error("config.toml has a syntax error, so neohar does not change it: {0}")]
    Syntax(String),
    #[error("cannot write config.toml: {0}")]
    Io(String),
}

impl From<ConfigError> for RpcError {
    fn from(error: ConfigError) -> Self {
        let code = match &error {
            ConfigError::UnknownKey(_) | ConfigError::Invalid(_) | ConfigError::Syntax(_) => {
                ErrorCode::InvalidParams
            }
            ConfigError::Io(_) => ErrorCode::Internal,
        };
        Self::new(code, error.to_string())
    }
}

/// Publishes the `settings` projection and puts the notes about the file in `health`. Call it at
/// the start and after each change of the configuration.
pub fn publish_all(config: &ConfigStore, hub: &ProjectionHub, health: &crate::health::Health) {
    config.publish(hub);
    health.set_warnings(
        "config",
        config.warnings(),
        Some("The last good configuration stays in force."),
    );
}

/// What a reload found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReloadReport {
    /// The settings whose effective value changed, as dotted paths.
    pub changed: Vec<String>,
    /// Why the file could not be used. The last good configuration is in force.
    pub errors: Vec<String>,
}

#[derive(Default)]
struct Inner {
    /// The file was read at least once.
    loaded: bool,
    /// The text of the file that was last handled, valid or not. `None`: there was no file.
    seen: Option<String>,
    /// The values of the file, by dotted path.
    file: BTreeMap<String, Value>,
    /// The values that `config.set` wrote, by dotted path.
    ui: BTreeMap<String, Value>,
    /// The effective values, by dotted path.
    effective: BTreeMap<String, Value>,
    warnings: Vec<String>,
    /// The labels of `[machines]` in the order of the file.
    machine_order: Vec<String>,
}

/// The configuration of the core.
pub struct ConfigStore {
    path: PathBuf,
    current: ArcSwap<Config>,
    inner: Mutex<Inner>,
    /// Called with the machines after a change of `[machines]`, outside the lock of the store.
    machines_changed: Mutex<Option<MachinesObserver>>,
}

/// What the store calls when the table `[machines]` changes.
pub type MachinesObserver = Arc<dyn Fn(Vec<MachineConfig>) + Send + Sync>;

/// One machine of `config.toml` with its label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineConfig {
    pub label: String,
    pub machine: Machine,
}

/// The machines of a configuration in the order of the file. A machine that the order does not
/// name (it cannot happen after a load) comes last, by label.
fn machines_of(config: &Config, order: &[String]) -> Vec<MachineConfig> {
    let mut out: Vec<MachineConfig> = order
        .iter()
        .filter_map(|label| {
            config.machines.get(label).map(|machine| MachineConfig {
                label: label.clone(),
                machine: machine.clone(),
            })
        })
        .collect();
    for (label, machine) in &config.machines {
        if !order.contains(label) {
            out.push(MachineConfig {
                label: label.clone(),
                machine: machine.clone(),
            });
        }
    }
    out
}

/// The labels under `[machines]` in the order of the text. A text that does not parse has none.
fn machine_order(text: Option<&str>) -> Vec<String> {
    let Some(doc) = text.and_then(|text| text.parse::<toml_edit::DocumentMut>().ok()) else {
        return Vec::new();
    };
    doc.get("machines")
        .and_then(toml_edit::Item::as_table_like)
        .map(|table| table.iter().map(|(label, _)| label.to_owned()).collect())
        .unwrap_or_default()
}

/// Whether `path` is a field of a machine: `machines.<label>.<field>`. The label is checked
/// apart, so a bad label gets its own message.
fn is_machine_leaf(path: &str) -> bool {
    let mut parts = path.split('.');
    parts.next() == Some("machines")
        && parts.next().is_some()
        && parts.next().is_some_and(allowed_machine_field)
        && parts.next().is_none()
}

/// The fields of a machine that a file may have. `remote_nh` is for the tests of a debug build.
fn allowed_machine_field(field: &str) -> bool {
    schema::MACHINE_FIELDS.contains(&field) || (cfg!(debug_assertions) && field == "remote_nh")
}

/// Drops the machines that cannot work, and says why. The other machines stay.
fn clean_machines(config: &mut Config) -> Vec<String> {
    let mut problems = Vec::new();
    config.machines.retain(|label, machine| {
        if !valid_machine_label(label) {
            problems.push(format!(
                "config.toml: [machines.{label}] is ignored: a label has 1 to {MAX_LABEL} characters of a-z, 0-9 and -, and it is not \"local\""
            ));
            return false;
        }
        if machine.ssh.trim().is_empty() {
            problems.push(format!("config.toml: [machines.{label}] is ignored: ssh is empty"));
            return false;
        }
        if machine.remote_nh.is_some() && !cfg!(debug_assertions) {
            problems.push(format!(
                "config.toml: machines.{label}.remote_nh is for tests, so neohar ignores it"
            ));
            machine.remote_nh = None;
        }
        true
    });
    problems
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Turns a tree of tables into dotted paths. An array is a value.
fn flatten(prefix: &str, value: &Value, into: &mut BTreeMap<String, Value>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten(&path, inner, into);
            }
        }
        other => {
            into.insert(prefix.to_owned(), other.clone());
        }
    }
}

fn table_to_json(table: &toml::Table) -> Value {
    serde_json::to_value(table).unwrap_or(Value::Null)
}

/// Puts `over` on `base`. Tables merge, everything else replaces.
fn merge(base: &mut toml::Table, over: toml::Table) {
    for (key, value) in over {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(into)), toml::Value::Table(from)) => merge(into, from),
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

/// Keys that an older version had and that this version ignores. A file that still has one is
/// not wrong: the key gets one line in the log and no warning in `health`.
const RETIRED: [&str; 3] = [
    "input.confirm_multiline_paste_to_agents",
    "search.index_transcripts",
    "update.manifest_check",
];

/// Tables that an older version had, with all their keys: `tasks` held the settings of the task
/// workspaces, which neohar no longer has. They are ignored as the keys of [`RETIRED`] are.
const RETIRED_TABLES: [&str; 1] = ["tasks"];

fn is_retired(path: &str) -> bool {
    RETIRED.contains(&path)
        || RETIRED_TABLES.iter().any(|table| {
            path.strip_prefix(table)
                .is_some_and(|rest| rest.starts_with('.'))
        })
}

/// What a file makes: the typed configuration, the effective values, and the keys it has that
/// this version does not know.
struct Built {
    config: Config,
    effective: BTreeMap<String, Value>,
    file: BTreeMap<String, Value>,
    unknown: Vec<String>,
    /// The keys of [`RETIRED`] and [`RETIRED_TABLES`] that the file has.
    retired: Vec<String>,
    /// What is wrong with the rules of the policy. The other rules work.
    problems: Vec<String>,
}

impl Built {
    /// The warnings for the health banner: the keys that this version ignores, and the rules that
    /// are not valid.
    fn warnings(&self) -> Vec<String> {
        let mut out = unknown_warnings(&self.unknown);
        out.extend(self.problems.clone());
        out
    }
}

fn build(file: &toml::Table) -> Result<Built, String> {
    let defaults = schema::defaults_table();
    let mut default_leaves = BTreeMap::new();
    flatten("", &table_to_json(&defaults), &mut default_leaves);
    let mut merged = defaults;
    merge(&mut merged, file.clone());
    let mut config: Config = toml::Value::Table(merged)
        .try_into()
        .map_err(|e: toml::de::Error| e.to_string())?;
    let machine_problems = clean_machines(&mut config);
    let mut effective = BTreeMap::new();
    flatten(
        "",
        &serde_json::to_value(&config).map_err(|e| e.to_string())?,
        &mut effective,
    );
    let mut file_values = BTreeMap::new();
    flatten("", &table_to_json(file), &mut file_values);
    let (retired, unknown): (Vec<String>, Vec<String>) = file_values
        .keys()
        .filter(|path| !default_leaves.contains_key(*path) && !is_machine_leaf(path))
        .cloned()
        .partition(|path| is_retired(path));
    let mut problems =
        crate::policy::compile_config(&config, crate::policy::calls::home().as_deref()).1;
    problems.extend(machine_problems);
    Ok(Built {
        config,
        effective,
        file: file_values,
        unknown,
        retired,
        problems,
    })
}

fn unknown_warnings(unknown: &[String]) -> Vec<String> {
    unknown
        .iter()
        .map(|path| {
            format!("config.toml: {path} is not a setting of this version, so neohar ignores it")
        })
        .collect()
}

impl ConfigStore {
    /// Reads the file. A missing file means the defaults. A file that is not valid also means
    /// the defaults, with a warning.
    pub fn load(path: PathBuf) -> Self {
        let store = Self {
            path,
            current: ArcSwap::from_pointee(Config::default()),
            inner: Mutex::new(Inner::default()),
            machines_changed: Mutex::new(None),
        };
        {
            let mut inner = lock(&store.inner);
            let mut defaults = BTreeMap::new();
            flatten(
                "",
                &serde_json::to_value(Config::default()).unwrap_or(Value::Null),
                &mut defaults,
            );
            inner.effective = defaults;
        }
        store.reload();
        store
    }

    /// The path of the file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The configuration in force. A reader does not wait for a writer.
    pub fn config(&self) -> Arc<Config> {
        self.current.load_full()
    }

    /// What is wrong with the file now.
    pub fn warnings(&self) -> Vec<String> {
        lock(&self.inner).warnings.clone()
    }

    fn read_file(&self) -> Result<Option<String>, String> {
        match fs::read_to_string(&self.path) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("cannot read {}: {e}", self.path.display())),
        }
    }

    /// Reads the file again. A file that did not change is not parsed again. If the file is not
    /// valid, the configuration in force stays, and the report says why.
    ///
    /// The read happens under the lock, as in [`ConfigStore::set`]. Else a reload can read the
    /// file, a `set` can write a newer one, and the reload then puts the older text in force.
    pub fn reload(&self) -> ReloadReport {
        let before = self.machines();
        let report = self.reload_inner();
        self.notify_machines(before);
        report
    }

    fn reload_inner(&self) -> ReloadReport {
        let mut inner = lock(&self.inner);
        let text = match self.read_file() {
            Ok(text) => text,
            Err(error) => {
                inner.warnings = vec![error.clone()];
                return ReloadReport {
                    changed: Vec::new(),
                    errors: vec![error],
                };
            }
        };
        if inner.loaded && inner.seen == text {
            return ReloadReport::default();
        }
        inner.loaded = true;
        let table = match text.as_deref() {
            None => Ok(toml::Table::new()),
            Some(text) => text
                .parse::<toml::Table>()
                .map_err(|e| format!("config.toml: {e}")),
        };
        let built = table.and_then(|table| build(&table).map_err(|e| format!("config.toml: {e}")));
        let order = machine_order(text.as_deref());
        inner.seen = text;
        match built {
            Err(error) => {
                inner.warnings = vec![error.clone()];
                ReloadReport {
                    changed: Vec::new(),
                    errors: vec![error],
                }
            }
            Ok(built) => {
                if !built.retired.is_empty() {
                    tracing::warn!(
                        keys = ?built.retired,
                        "config.toml has settings of an older version; neohar ignores them"
                    );
                }
                let changed = diff(&inner.effective, &built.effective);
                inner.warnings = built.warnings();
                inner
                    .ui
                    .retain(|path, value| built.file.get(path) == Some(value));
                inner.file = built.file;
                inner.effective = built.effective;
                inner.machine_order = order;
                self.current.store(Arc::new(built.config));
                ReloadReport {
                    changed,
                    errors: Vec::new(),
                }
            }
        }
    }

    /// The sections for the `settings` projection and for `config.get`: for each section the
    /// settings with their value and source.
    pub fn sections(&self) -> BTreeMap<String, SettingsSection> {
        let inner = lock(&self.inner);
        let mut sections: BTreeMap<String, SettingsSection> = BTreeMap::new();
        for (path, value) in &inner.effective {
            let (section, key) = path.split_once('.').unwrap_or(("general", path.as_str()));
            let source = if inner
                .ui
                .get(path)
                .is_some_and(|v| inner.file.get(path) == Some(v))
            {
                SettingSource::Ui
            } else if inner.file.contains_key(path) {
                SettingSource::File
            } else {
                SettingSource::Default
            };
            sections
                .entry(section.to_owned())
                .or_insert_with(|| SettingsSection {
                    entries: BTreeMap::new(),
                })
                .entries
                .insert(
                    key.to_owned(),
                    SettingEntry {
                        value: value.clone(),
                        source,
                    },
                );
        }
        sections
    }

    /// Puts the sections into the `settings` projection.
    pub fn publish(&self, hub: &ProjectionHub) {
        for (name, section) in self.sections() {
            hub.set(
                Projection::Settings,
                &name,
                serde_json::to_value(section).unwrap_or(Value::Null),
            );
        }
    }

    /// Sets a setting by its dotted path, for example `approvals.mode`. `null` removes the key
    /// from the file, so that the default is in force again.
    ///
    /// A machine has dotted keys that `defaults.toml` cannot list, because its labels are the
    /// person's: `machines.<label>.<field>` sets one field, and `machines.<label>` takes an object
    /// with the fields, or `null` to remove the machine.
    pub fn set(&self, key: &str, value: Value) -> Result<(), ConfigError> {
        let before = self.machines();
        let result = self.set_inner(key, value);
        if result.is_ok() {
            self.notify_machines(before);
        }
        result
    }

    fn set_inner(&self, key: &str, value: Value) -> Result<(), ConfigError> {
        let mut inner = lock(&self.inner);
        let mut defaults = BTreeMap::new();
        flatten("", &table_to_json(&schema::defaults_table()), &mut defaults);
        let machine = machine_key(key)?;
        if machine.is_none() && !defaults.contains_key(key) {
            return Err(ConfigError::UnknownKey(key.to_owned()));
        }
        // Edit the text that is on disk now, so that an edit by hand between two sets stays.
        let text = self
            .read_file()
            .map_err(ConfigError::Io)?
            .unwrap_or_default();
        let mut doc: toml_edit::DocumentMut = text
            .parse()
            .map_err(|e: toml_edit::TomlError| ConfigError::Syntax(e.to_string()))?;
        if matches!(machine, Some(MachineKey::Whole)) {
            remove_key(doc.as_table_mut(), key);
            match &value {
                Value::Null => {}
                Value::Object(fields) => {
                    for (field, field_value) in fields {
                        if !allowed_machine_field(field) {
                            return Err(ConfigError::UnknownKey(format!("{key}.{field}")));
                        }
                        insert_key(doc.as_table_mut(), &format!("{key}.{field}"), field_value)?;
                    }
                }
                _ => {
                    return Err(ConfigError::Invalid(format!(
                        "{key} is a table: give an object with ssh, session and connect, or null"
                    )));
                }
            }
        } else if value.is_null() {
            remove_key(doc.as_table_mut(), key);
        } else {
            insert_key(doc.as_table_mut(), key, &value)?;
        }
        let new_text = doc.to_string();
        let table = new_text
            .parse::<toml::Table>()
            .map_err(|e| ConfigError::Invalid(e.to_string()))?;
        let built = build(&table).map_err(|e| ConfigError::Invalid(format!("{key}: {e}")))?;
        write_atomic(&self.path, &new_text).map_err(|e| ConfigError::Io(e.to_string()))?;

        if value.is_null() {
            inner.ui.remove(key);
        } else if let Some(effective) = built.effective.get(key) {
            inner.ui.insert(key.to_owned(), effective.clone());
        }
        inner.ui.retain(|path, v| built.file.get(path) == Some(v));
        inner.loaded = true;
        inner.seen = Some(new_text);
        inner.warnings = built.warnings();
        inner.file = built.file;
        inner.effective = built.effective;
        inner.machine_order = machine_order(inner.seen.as_deref());
        self.current.store(Arc::new(built.config));
        Ok(())
    }

    /// The machines of `config.toml` in the order of the file.
    pub fn machines(&self) -> Vec<MachineConfig> {
        let inner = lock(&self.inner);
        machines_of(&self.current.load(), &inner.machine_order)
    }

    /// Sets the function that the store calls, with the machines, after each change of
    /// `[machines]`. It runs on the thread that made the change and must not wait.
    pub fn on_machines_changed(&self, observer: MachinesObserver) {
        *lock(&self.machines_changed) = Some(observer);
    }

    fn notify_machines(&self, before: Vec<MachineConfig>) {
        let after = self.machines();
        if after == before {
            return;
        }
        let observer = lock(&self.machines_changed).clone();
        if let Some(observer) = observer {
            observer(after);
        }
    }

    /// Starts a thread that watches the file. When it changes and settles, the file is read
    /// again, and `on_change` gets the report if anything changed or the file is not valid.
    ///
    /// The call returns at once. The operating system needs time to set up a watch (on macOS
    /// about half a second, and several seconds when the machine is busy), and the start of the
    /// core must not wait for it. The thread sets the watch up, then reads the file once more for
    /// a change that came in the meantime, and once more after [`START_LAG`], because a watch can
    /// miss a write in its first moments. [`ConfigWatcher::wait_ready`] waits for the setup only.
    pub fn watch(
        self: &Arc<Self>,
        on_change: impl Fn(&ReloadReport) + Send + 'static,
    ) -> std::io::Result<ConfigWatcher> {
        self.watch_with(crate::watch::Kind::of_build(), on_change)
    }

    /// Like [`ConfigStore::watch`], with a watcher of `kind`.
    pub fn watch_with(
        self: &Arc<Self>,
        kind: crate::watch::Kind,
        on_change: impl Fn(&ReloadReport) + Send + 'static,
    ) -> std::io::Result<ConfigWatcher> {
        let dir = self
            .path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
        let file_name = self.path.file_name().map(std::ffi::OsStr::to_owned);
        let (tx, rx) = channel::<Message>();
        let (ready_tx, ready_rx) = channel::<Result<(), String>>();
        let events = tx.clone();
        let store = Arc::clone(self);
        let thread = std::thread::Builder::new()
            .name("nh-config-watch".to_owned())
            .spawn(move || {
                let started = crate::watch::watch_dir(kind, &dir, move |change| {
                    let _ = events.send(Message::Event(change));
                });
                // The watcher stays alive in this scope until the loop ends.
                let _watcher = match started {
                    Ok(watcher) => watcher,
                    Err(error) => {
                        tracing::warn!(%error, "config.toml is not watched; use config.reload");
                        let _ = ready_tx.send(Err(error.to_string()));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                let handle = |report: ReloadReport| {
                    if !report.changed.is_empty() || !report.errors.is_empty() {
                        on_change(&report);
                    }
                };
                // A change between the start of the core and the start of the watch.
                handle(store.reload());
                // The system starts a watch a moment late, and a write in the first moments after
                // it can get no event (seen on macOS under load). So look at the file again once
                // the moment has passed. Events that come meanwhile are not needed: the read sees
                // the file as it is.
                let settled = Instant::now() + START_LAG;
                loop {
                    match rx.recv_timeout(settled.saturating_duration_since(Instant::now())) {
                        Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                        Ok(Message::Event(_)) => {}
                        Err(RecvTimeoutError::Timeout) => break,
                    }
                }
                handle(store.reload());
                let touches = |change: &crate::watch::Change| {
                    change
                        .paths
                        .iter()
                        .any(|p| p.file_name() == file_name.as_deref())
                };
                'watching: while let Ok(first) = rx.recv() {
                    match first {
                        Message::Stop => break,
                        Message::Event(event) if !touches(&event) => continue,
                        Message::Event(_) => {}
                    }
                    // Wait until the file has been quiet for the debounce time.
                    loop {
                        match rx.recv_timeout(DEBOUNCE) {
                            Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => {
                                break 'watching;
                            }
                            Ok(Message::Event(_)) => {}
                            Err(RecvTimeoutError::Timeout) => break,
                        }
                    }
                    handle(store.reload());
                }
            })?;
        Ok(ConfigWatcher {
            stop: tx,
            ready: Mutex::new(Readiness {
                channel: ready_rx,
                answer: None,
            }),
            thread: Some(thread),
        })
    }
}

/// What a key of `[machines]` names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MachineKey {
    /// `machines.<label>`: the whole table of a machine.
    Whole,
    /// `machines.<label>.<field>`.
    Field,
}

/// Reads a key that starts with `machines.`. Another key gives `None`.
fn machine_key(key: &str) -> Result<Option<MachineKey>, ConfigError> {
    let Some(rest) = key.strip_prefix("machines.") else {
        return Ok(None);
    };
    let parts: Vec<&str> = rest.split('.').collect();
    if !valid_machine_label(parts[0]) {
        return Err(ConfigError::Invalid(format!(
            "{:?} cannot name a machine: use 1 to {MAX_LABEL} characters of a-z, 0-9 and -, and not \"local\"",
            parts[0]
        )));
    }
    match parts.as_slice() {
        [_] => Ok(Some(MachineKey::Whole)),
        [_, field] if allowed_machine_field(field) => Ok(Some(MachineKey::Field)),
        _ => Err(ConfigError::UnknownKey(key.to_owned())),
    }
}

/// The paths whose values differ, in both directions.
fn diff(old: &BTreeMap<String, Value>, new: &BTreeMap<String, Value>) -> Vec<String> {
    let mut changed: Vec<String> = new
        .iter()
        .filter(|(path, value)| old.get(*path) != Some(value))
        .map(|(path, _)| path.clone())
        .collect();
    changed.extend(old.keys().filter(|path| !new.contains_key(*path)).cloned());
    changed.sort();
    changed
}

/// Removes a dotted key from a table, and the tables that become empty.
fn remove_key(table: &mut toml_edit::Table, key: &str) {
    match key.split_once('.') {
        None => {
            table.remove(key);
        }
        Some((head, rest)) => {
            if let Some(toml_edit::Item::Table(inner)) = table.get_mut(head) {
                remove_key(inner, rest);
                if inner.is_empty() {
                    table.remove(head);
                }
            }
        }
    }
}

/// Sets a dotted key in a table and makes the tables on the way.
fn insert_key(table: &mut toml_edit::Table, key: &str, value: &Value) -> Result<(), ConfigError> {
    match key.split_once('.') {
        None => {
            let item = toml_edit::Item::Value(to_toml(value)?);
            // Keep the comments around a value that exists.
            if let Some(existing) = table.get_mut(key).and_then(toml_edit::Item::as_value_mut) {
                let mut new = to_toml(value)?;
                *new.decor_mut() = existing.decor().clone();
                *existing = new;
            } else {
                table.insert(key, item);
            }
            Ok(())
        }
        Some((head, rest)) => {
            if !table.contains_key(head) {
                let mut inner = toml_edit::Table::new();
                inner.set_implicit(true);
                table.insert(head, toml_edit::Item::Table(inner));
            }
            match table.get_mut(head) {
                Some(toml_edit::Item::Table(inner)) => insert_key(inner, rest, value),
                _ => Err(ConfigError::Invalid(format!("{head} is not a table"))),
            }
        }
    }
}

fn to_toml(value: &Value) -> Result<toml_edit::Value, ConfigError> {
    Ok(match value {
        Value::Bool(b) => (*b).into(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.into(),
            None => n
                .as_f64()
                .ok_or_else(|| ConfigError::Invalid(format!("{n} is not a number")))?
                .into(),
        },
        Value::String(s) => s.as_str().into(),
        Value::Array(items) => {
            let mut array = toml_edit::Array::new();
            for item in items {
                array.push(to_toml(item)?);
            }
            toml_edit::Value::Array(array)
        }
        Value::Null | Value::Object(_) => {
            return Err(ConfigError::Invalid(
                "a setting is a boolean, a number, a text or a list".to_owned(),
            ));
        }
    })
}

/// Writes through a temporary file and a rename, so that a reader sees the old file or the new
/// one and never half of it.
fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    let temp = dir.join(format!(".{FILE_NAME}.tmp-{}", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        File::open(dir)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

enum Message {
    Event(crate::watch::Change),
    Stop,
}

/// Whether the watch is set up: the channel of the thread, and the answer once it came.
struct Readiness {
    channel: Receiver<Result<(), String>>,
    answer: Option<Result<(), String>>,
}

/// The thread that watches the file. Dropping it tells the thread to end, and the call does not
/// wait: the thread may still be setting up the watch.
pub struct ConfigWatcher {
    stop: Sender<Message>,
    ready: Mutex<Readiness>,
    thread: Option<JoinHandle<()>>,
}

impl ConfigWatcher {
    /// Waits until the watch is set up, or until it failed, or until `timeout` has passed.
    pub fn wait_ready(&self, timeout: Duration) -> Result<(), String> {
        let mut ready = lock(&self.ready);
        if ready.answer.is_none() {
            ready.answer = Some(match ready.channel.recv_timeout(timeout) {
                Ok(answer) => answer,
                Err(RecvTimeoutError::Timeout) => return Err("the watch is not ready".to_owned()),
                Err(RecvTimeoutError::Disconnected) => Err("the watcher thread ended".to_owned()),
            });
        }
        ready.answer.clone().unwrap_or(Ok(()))
    }

    /// Ends the thread and waits for it.
    pub fn stop(mut self) {
        let _ = self.stop.send(Message::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for ConfigWatcher {
    fn drop(&mut self) {
        // The thread ends by itself. Do not wait: it can be inside the setup of the watch.
        let _ = self.stop.send(Message::Stop);
    }
}

#[cfg(test)]
mod tests;
