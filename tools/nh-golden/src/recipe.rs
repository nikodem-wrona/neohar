//! The isolation recipe of each harness: the files, the environment and the command line that make
//! a real binary run offline, in a scratch tree, against the mock model, with the neohar hooks.
//!
//! The recipe is the answer to the question "what is the least that makes the harness start to a
//! usable prompt?"

use std::{
    fs,
    path::{Path, PathBuf},
};

use nh_record::holder::Scratch;
use serde_json::{Value, json};

use crate::{
    GoldenError,
    scenario::{FileSpec, HookReply},
};

/// A harness that v1 supports (`AGENTS.md`: Claude Code and Codex, nothing else).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Harness {
    Claude,
    Codex,
}

/// The hook events that neohar installs for Claude Code.
pub const CLAUDE_HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PermissionDenied",
    "PostToolUse",
    "PostToolUseFailure",
    "Notification",
    "Stop",
    "StopFailure",
    "SubagentStart",
    "SubagentStop",
    "PreCompact",
    "PostCompact",
    "CwdChanged",
    "Elicitation",
    "SessionEnd",
];

/// The hook events that neohar installs for Codex.
pub const CODEX_HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "SubagentStart",
    "SubagentStop",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "PreCompact",
    "PostCompact",
    "UserPromptSubmit",
    "Stop",
    "Interrupt",
];

/// The token that the hooks send to the API socket.
pub const PANE_TOKEN: &str = "nh-golden";

impl Harness {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }

    /// The name in the scenario files, the hook command and the output path.
    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    /// The hook events that the recipe installs.
    pub fn hook_events(self) -> &'static [&'static str] {
        match self {
            Self::Claude => CLAUDE_HOOK_EVENTS,
            Self::Codex => CODEX_HOOK_EVENTS,
        }
    }

    /// The version in the output of `--version`: `2.1.292 (Claude Code)` or `codex-cli 0.156.1`.
    pub fn parse_version(self, output: &str) -> Option<String> {
        let line = output.lines().next()?.trim();
        let token = match self {
            Self::Claude => line.split_whitespace().next()?,
            Self::Codex => line.strip_prefix("codex-cli")?.split_whitespace().next()?,
        };
        let valid = !token.is_empty()
            && token.starts_with(|c: char| c.is_ascii_digit())
            && token
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '+');
        valid.then(|| token.to_owned())
    }
}

/// Finds the binary of a harness on `PATH`.
pub fn find_binary(harness: Harness, path_var: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    std::env::split_paths(path_var?)
        .map(|dir| dir.join(harness.name()))
        .find(|candidate| fs::metadata(candidate).is_ok_and(|m| m.is_file()))
}

/// What a recipe needs to know.
pub struct Setup<'a> {
    pub harness: Harness,
    /// The binary of the harness, with an absolute path.
    pub binary: &'a Path,
    /// The `nh` binary that the hooks call.
    pub nh: &'a Path,
    /// The base URL of the mock model, such as `http://127.0.0.1:50000`.
    pub mock_url: &'a str,
    pub scratch: &'a Scratch,
    /// The directory where the shim writes `signals.jsonl`.
    pub tee_dir: &'a Path,
    /// The API socket of the recording.
    pub api_socket: &'a Path,
    /// The recording has a core that holds `PermissionRequest` hooks. The hook then
    /// waits as long as the gate, as the installer sets it.
    pub gate: bool,
    /// Arguments for the harness, with the placeholders of the scenario.
    pub args: &'a [String],
    /// Files to write before the harness starts.
    pub files: &'a [FileSpec],
    /// A key that is added, with the value `true`, to every hook object.
    pub hook_marker: Option<&'a str>,
    /// Settings of the scenario, merged into the settings of the recipe.
    pub settings: &'a toml::Table,
    /// State that is merged into `.claude.json` (Claude Code only).
    pub state: &'a toml::Table,
    /// What the hooks print.
    pub hook_replies: &'a [HookReply],
}

/// What a recipe makes.
#[derive(Clone, Debug)]
pub struct Prepared {
    /// The harness and its arguments. The sandbox wraps them.
    pub argv: Vec<String>,
    /// The whole environment of the harness.
    pub env: Vec<(String, String)>,
    /// The configuration directory that the harness uses (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`).
    pub config_dir: PathBuf,
}

fn write(path: &Path, text: &str) -> Result<(), GoldenError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(GoldenError::io(format!("create {}", parent.display())))?;
    }
    fs::write(path, text).map_err(GoldenError::io(format!("write {}", path.display())))
}

/// A string for a shell: in single quotes.
fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// The `hooks` object that both harnesses read. One entry per event, each calling the shim. When
/// the scenario gives a reply for an event, the command prints the content of a file after the
/// shim ran (the shim reads the standard input and prints nothing).
fn hooks_json(
    harness: Harness,
    nh: &Path,
    replies: &[(String, PathBuf)],
    marker: Option<&str>,
    gate: bool,
) -> Value {
    let mut hooks = serde_json::Map::new();
    for event in harness.hook_events() {
        let mut command = format!(
            "{} hook {} {event}",
            shell_quote(&nh.display().to_string()),
            harness.name()
        );
        if let Some((_, file)) = replies.iter().find(|(e, _)| e == event) {
            command.push_str(&format!(
                "; /bin/cat {}",
                shell_quote(&file.display().to_string())
            ));
        }
        // Codex cuts the timeout of these two events to 3 s and warns about it at every start.
        let timeout = if matches!(*event, "SessionEnd" | "Interrupt") {
            3
        } else if gate && *event == "PermissionRequest" {
            // The installer sets this to the gate timeout of the core.
            1800
        } else {
            10
        };
        let mut hook = json!({ "type": "command", "command": command, "timeout": timeout });
        if let (Some(key), Some(object)) = (marker, hook.as_object_mut()) {
            object.insert(key.to_owned(), Value::Bool(true));
        }
        hooks.insert((*event).to_owned(), json!([{ "hooks": [hook] }]));
    }
    json!({ "hooks": hooks })
}

/// Merges `from` into `into`. Objects merge key by key. Any other value replaces the old one.
fn merge_json(into: &mut Value, from: &Value) {
    match (into, from) {
        (Value::Object(into), Value::Object(from)) => {
            for (key, value) in from {
                merge_json(into.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
        (into, from) => *into = from.clone(),
    }
}

/// Merges `from` into `into`, like [`merge_json`].
fn merge_toml(into: &mut toml::Table, from: &toml::Table) {
    for (key, value) in from {
        match (into.get_mut(key), value) {
            (Some(toml::Value::Table(old)), toml::Value::Table(new)) => merge_toml(old, new),
            _ => {
                into.insert(key.clone(), value.clone());
            }
        }
    }
}

/// The `nh-golden` binary: this executable, or the one next to it when a test runs. A test
/// executable is in `target/debug/deps/`, so the directory above counts as "next to" it too.
pub fn golden_binary() -> PathBuf {
    let Ok(exe) = std::env::current_exe() else {
        return PathBuf::from("nh-golden");
    };
    if exe.file_name().is_some_and(|name| name == "nh-golden") {
        return exe;
    }
    let mut dirs: Vec<PathBuf> = exe.parent().map(Path::to_owned).into_iter().collect();
    if let Some(up) = dirs.first().and_then(|dir| dir.parent()) {
        dirs.push(up.to_owned());
    }
    dirs.into_iter()
        .map(|dir| dir.join("nh-golden"))
        .find(|candidate| candidate.is_file())
        .unwrap_or(exe)
}

/// Expands the placeholders of the scratch tree, `{golden}` and `{uid}`.
pub fn expand(scratch: &Scratch, text: &str) -> String {
    scratch
        .expand(text)
        .replace("{golden}", &golden_binary().display().to_string())
        .replace("{uid}", &rustix::process::getuid().as_raw().to_string())
}

/// Writes the files of the scenario. A file must be inside the scratch tree.
fn write_files(scratch: &Scratch, files: &[FileSpec]) -> Result<(), GoldenError> {
    for file in files {
        let path = PathBuf::from(expand(scratch, &file.path));
        if !path.starts_with(&scratch.root) || path.components().any(|c| c.as_os_str() == "..") {
            return Err(GoldenError::Setup(format!(
                "the file {} is not inside the scratch tree",
                path.display()
            )));
        }
        write(&path, &expand(scratch, &file.content))?;
    }
    Ok(())
}

/// Writes the reply files of the scenario and returns the event and the file of each.
fn write_replies(
    scratch: &Scratch,
    replies: &[HookReply],
) -> Result<Vec<(String, PathBuf)>, GoldenError> {
    let dir = scratch.root.join("replies");
    let mut files = Vec::new();
    for reply in replies {
        let file = dir.join(format!("{}.json", reply.event));
        write(&file, &reply.reply.to_string())?;
        files.push((reply.event.clone(), file));
    }
    Ok(files)
}

/// The real path of the working directory. The harnesses key their trust lists by it, and on macOS
/// `/tmp` is a link to `/private/tmp`.
pub fn real_work_dir(scratch: &Scratch) -> Result<PathBuf, GoldenError> {
    fs::canonicalize(&scratch.work).map_err(GoldenError::io(format!(
        "resolve {}",
        scratch.work.display()
    )))
}

/// Makes the configuration files and returns how to start the harness.
pub fn prepare(setup: &Setup<'_>) -> Result<Prepared, GoldenError> {
    let scratch = setup.scratch;
    // A hook runs in the working directory of the harness, so a relative path would not find `nh`.
    // The harness shows no error for that: Claude Code logs it to its debug file only.
    for (what, path) in [("nh", setup.nh), ("the harness binary", setup.binary)] {
        if !path.is_absolute() {
            return Err(GoldenError::Setup(format!(
                "the path of {what} must be absolute, not {}",
                path.display()
            )));
        }
    }
    let work = real_work_dir(scratch)?;
    let reply_files = write_replies(scratch, setup.hook_replies)?;
    write_files(scratch, setup.files)?;
    let binary = setup.binary.display().to_string();
    let mut extra: Vec<(String, String)> = vec![
        // The harness is found by its path. `PATH` also has its directory, for helpers.
        (
            "PATH".into(),
            format!(
                "{}:{}",
                nh_record::holder::DEFAULT_PATH,
                setup
                    .binary
                    .parent()
                    .map_or(String::new(), |d| d.display().to_string())
            ),
        ),
        // The shim does nothing without these.
        ("NEOHAR".into(), "1".into()),
        (
            "NEOHAR_SOCKET".into(),
            setup.api_socket.display().to_string(),
        ),
        ("NEOHAR_PANE_TOKEN".into(), PANE_TOKEN.into()),
        (
            "NEOHAR_HOOK_TEE".into(),
            setup.tee_dir.display().to_string(),
        ),
    ];
    let config_dir = match setup.harness {
        Harness::Claude => {
            let dir = scratch.home.join("claude");
            // The least that reaches the prompt. Without `hasCompletedOnboarding` Claude Code
            // shows its welcome and the theme picker. Without the approved key it asks whether to
            // use the key of the environment (`approved` holds the end of the key). Without
            // `hasTrustDialogAccepted` it asks to trust the folder, and it loads no hook until
            // the answer is yes.
            let mut seed = json!({
                "hasCompletedOnboarding": true,
                "customApiKeyResponses": { "approved": ["mock"], "rejected": [] },
                "projects": { work.display().to_string(): { "hasTrustDialogAccepted": true } },
            });
            let state =
                serde_json::to_value(setup.state).map_err(|e| GoldenError::Setup(e.to_string()))?;
            merge_json(&mut seed, &state);
            write(&dir.join(".claude.json"), &seed.to_string())?;
            let mut settings = hooks_json(
                Harness::Claude,
                setup.nh,
                &reply_files,
                setup.hook_marker,
                setup.gate,
            );
            let from_scenario = serde_json::to_value(setup.settings)
                .map_err(|e| GoldenError::Setup(e.to_string()))?;
            merge_json(&mut settings, &from_scenario);
            write(
                &dir.join("settings.json"),
                &serde_json::to_string_pretty(&settings)
                    .map_err(|e| GoldenError::Setup(e.to_string()))?,
            )?;
            extra.extend([
                ("CLAUDE_CONFIG_DIR".into(), dir.display().to_string()),
                ("ANTHROPIC_BASE_URL".into(), setup.mock_url.into()),
                ("ANTHROPIC_API_KEY".into(), "mock".into()),
                (
                    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".into(),
                    "1".into(),
                ),
                // The Bash tool writes its task files to `/tmp/claude-<uid>` unless told
                // otherwise. The sandbox does not allow that place, and every other Claude Code
                // of the user shares it.
                (
                    "CLAUDE_CODE_TMPDIR".into(),
                    scratch.tmp.display().to_string(),
                ),
            ]);
            dir
        }
        Harness::Codex => {
            if !setup.state.is_empty() {
                return Err(GoldenError::Setup(
                    "[state] is for Claude Code. Codex has no such file".into(),
                ));
            }
            let dir = scratch.home.join("codex");
            let mut config = toml::Table::new();
            let mut set = |key: &str, value: toml::Value| {
                config.insert(key.to_owned(), value);
            };
            // An unknown name makes Codex use fallback metadata and show one warning. A name that
            // Codex knows is worse: Codex moves it to its newest model, and that changes the
            // screen and asks questions. The mock answers to any name.
            set("model", "mock-model".into());
            set("model_provider", "mock".into());
            // The recording must not wait for an answer to an approval, and Codex must not start
            // a sandbox of its own inside ours (a sandbox cannot start another one).
            set("approval_policy", "never".into());
            set("sandbox_mode", "danger-full-access".into());
            set("check_for_update_on_startup", false.into());
            let table = |pairs: Vec<(&str, toml::Value)>| {
                toml::Value::Table(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
            };
            set("analytics", table(vec![("enabled", false.into())]));
            // These features talk to the network at start (plugin sync, the list of apps, update
            // checks). The sandbox blocks the network, and each failure adds a warning to the
            // status line. Off, the screen shows only what the scenario does.
            set(
                "features",
                table(vec![
                    ("plugins", false.into()),
                    ("remote_plugin", false.into()),
                    ("apps", false.into()),
                    ("in_app_updates", false.into()),
                ]),
            );
            set("tui", table(vec![("show_tooltips", false.into())]));
            set(
                "model_providers",
                table(vec![(
                    "mock",
                    table(vec![
                        ("name", "mock".into()),
                        ("base_url", format!("{}/v1", setup.mock_url).into()),
                        ("wire_api", "responses".into()),
                        ("env_key", "MOCK_API_KEY".into()),
                    ]),
                )]),
            );
            set(
                "projects",
                table(vec![(
                    &work.display().to_string(),
                    table(vec![("trust_level", "trusted".into())]),
                )]),
            );
            merge_toml(&mut config, setup.settings);
            write(
                &dir.join("config.toml"),
                &toml::to_string(&config).map_err(|e| GoldenError::Setup(e.to_string()))?,
            )?;
            write(
                &dir.join("hooks.json"),
                &serde_json::to_string_pretty(&hooks_json(
                    Harness::Codex,
                    setup.nh,
                    &reply_files,
                    setup.hook_marker,
                    setup.gate,
                ))
                .map_err(|e| GoldenError::Setup(e.to_string()))?,
            )?;
            extra.extend([
                ("CODEX_HOME".into(), dir.display().to_string()),
                ("MOCK_API_KEY".into(), "mock".into()),
            ]);
            dir
        }
    };
    let mut argv = vec![binary];
    argv.extend(setup.args.iter().map(|arg| expand(scratch, arg)));
    Ok(Prepared {
        argv,
        env: scratch.child_env(&extra),
        config_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_read_from_the_first_line() {
        assert_eq!(
            Harness::Claude
                .parse_version("2.1.292 (Claude Code)\n")
                .as_deref(),
            Some("2.1.292")
        );
        assert_eq!(
            Harness::Codex
                .parse_version("codex-cli 0.156.1\n")
                .as_deref(),
            Some("0.156.1")
        );
        assert_eq!(Harness::Codex.parse_version("2.1.292 (Claude Code)"), None);
        assert_eq!(Harness::Claude.parse_version(""), None);
        assert_eq!(Harness::Claude.parse_version("not-a-version"), None);
        // A version becomes a directory name, so it holds no path separator.
        assert_eq!(
            Harness::Claude.parse_version("1.0/../x (Claude Code)"),
            None
        );
    }

    #[test]
    fn a_gate_recording_holds_the_permission_hook_as_long_as_the_gate() {
        let json = hooks_json(Harness::Claude, Path::new("/opt/nh"), &[], None, true);
        let timeout = |event: &str| json["hooks"][event][0]["hooks"][0]["timeout"].as_i64();
        assert_eq!(timeout("PermissionRequest"), Some(1800));
        assert_eq!(timeout("Stop"), Some(10));
        let json = hooks_json(Harness::Claude, Path::new("/opt/nh"), &[], None, false);
        assert_eq!(
            json["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"],
            10
        );
    }

    #[test]
    fn a_reply_file_is_printed_after_the_shim() {
        let replies = [("Stop".to_owned(), PathBuf::from("/tmp/r/Stop.json"))];
        let json = hooks_json(
            Harness::Claude,
            Path::new("/opt/nh"),
            &replies,
            Some("_neohar"),
            false,
        );
        let command = |event: &str| {
            json["hooks"][event][0]["hooks"][0]["command"]
                .as_str()
                .expect("a command")
                .to_owned()
        };
        assert_eq!(
            command("Stop"),
            "'/opt/nh' hook claude Stop; /bin/cat '/tmp/r/Stop.json'"
        );
        assert_eq!(
            command("SessionStart"),
            "'/opt/nh' hook claude SessionStart"
        );
        assert_eq!(json["hooks"]["Stop"][0]["hooks"][0]["_neohar"], true);
    }

    #[test]
    fn settings_of_the_scenario_merge_into_the_files_of_the_recipe() {
        let mut json = json!({ "hooks": { "Stop": 1 }, "permissions": { "allow": ["a"] } });
        merge_json(
            &mut json,
            &json!({ "permissions": { "defaultMode": "default", "allow": ["b"] }, "model": "m" }),
        );
        assert_eq!(
            json,
            json!({
                "hooks": { "Stop": 1 },
                "permissions": { "defaultMode": "default", "allow": ["b"] },
                "model": "m"
            })
        );
        let mut table: toml::Table = toml::from_str("a = 1\n[t]\nx = 1\ny = 2").expect("toml");
        let extra: toml::Table = toml::from_str("a = 2\n[t]\ny = 3\nz = 4").expect("toml");
        merge_toml(&mut table, &extra);
        assert_eq!(
            table,
            toml::from_str("a = 2\n[t]\nx = 1\ny = 3\nz = 4").expect("toml")
        );
    }

    #[test]
    fn scenario_settings_and_state_reach_the_files_of_claude_code() {
        let scratch = Scratch::create().expect("scratch");
        let settings: toml::Table = toml::from_str(
            "skipAutoPermissionPrompt = true\n[permissions]\ndefaultMode = \"default\"",
        )
        .expect("toml");
        let state: toml::Table = toml::from_str("extra = 1").expect("toml");
        let replies = [HookReply {
            event: "Stop".into(),
            reply: json!({ "continue": true }),
        }];
        let prepared = prepare(&Setup {
            harness: Harness::Claude,
            binary: Path::new("/usr/local/bin/claude"),
            nh: Path::new("/usr/local/bin/nh"),
            mock_url: "http://127.0.0.1:1",
            scratch: &scratch,
            tee_dir: &scratch.root.join("tee"),
            api_socket: &scratch.run.join("a.sock"),
            gate: false,
            args: &[],
            hook_marker: None,
            files: &[],
            settings: &settings,
            state: &state,
            hook_replies: &replies,
        })
        .expect("prepared");
        let read = |name: &str| -> Value {
            serde_json::from_str(
                &fs::read_to_string(prepared.config_dir.join(name)).expect("a file"),
            )
            .expect("JSON")
        };
        let written = read("settings.json");
        assert_eq!(written["skipAutoPermissionPrompt"], true, "{written}");
        assert_eq!(written["permissions"]["defaultMode"], "default");
        assert!(
            written["hooks"]["Stop"][0]["hooks"][0]["command"]
                .as_str()
                .expect("a command")
                .ends_with("/replies/Stop.json'")
        );
        assert_eq!(read(".claude.json")["extra"], 1);
        assert_eq!(read(".claude.json")["hasCompletedOnboarding"], true);
        let reply =
            fs::read_to_string(scratch.root.join("replies/Stop.json")).expect("a reply file");
        assert_eq!(reply, "{\"continue\":true}");
    }

    #[test]
    fn a_relative_path_to_nh_is_refused() {
        let scratch = Scratch::create().expect("scratch");
        let error = prepare(&Setup {
            harness: Harness::Claude,
            binary: Path::new("/usr/local/bin/tool"),
            nh: Path::new("target/debug/nh"),
            mock_url: "http://127.0.0.1:1",
            scratch: &scratch,
            tee_dir: &scratch.root.join("tee"),
            api_socket: &scratch.run.join("a.sock"),
            gate: false,
            args: &[],
            hook_marker: None,
            files: &[],
            settings: &toml::Table::new(),
            state: &toml::Table::new(),
            hook_replies: &[],
        })
        .expect_err("a relative path");
        assert!(error.to_string().contains("must be absolute"), "{error}");
    }

    #[test]
    fn the_hook_command_calls_the_shim_with_the_harness_and_the_event() {
        let json = hooks_json(Harness::Codex, Path::new("/opt/it's/nh"), &[], None, false);
        let command = json["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .expect("a command");
        assert_eq!(command, "'/opt/it'\\''s/nh' hook codex Stop");
        assert_eq!(
            json["hooks"].as_object().expect("an object").len(),
            CODEX_HOOK_EVENTS.len()
        );
    }

    #[test]
    fn the_recipes_write_their_files_inside_the_scratch_tree() {
        let scratch = Scratch::create().expect("scratch");
        let tee = scratch.root.join("tee");
        let sock = scratch.run.join("api.sock");
        for harness in [Harness::Claude, Harness::Codex] {
            let prepared = prepare(&Setup {
                harness,
                binary: Path::new("/usr/local/bin/tool"),
                nh: Path::new("/usr/local/bin/nh"),
                mock_url: "http://127.0.0.1:1234",
                scratch: &scratch,
                tee_dir: &tee,
                api_socket: &sock,
                gate: false,
                args: &["--flag".to_owned(), "{work}/x".to_owned()],
                hook_marker: None,
                files: &[],
                settings: &toml::Table::new(),
                state: &toml::Table::new(),
                hook_replies: &[],
            })
            .expect("prepared");
            assert!(prepared.config_dir.starts_with(&scratch.home));
            assert_eq!(
                prepared.argv,
                [
                    "/usr/local/bin/tool".to_owned(),
                    "--flag".to_owned(),
                    format!("{}/x", scratch.work.display())
                ]
            );
            let get = |key: &str| {
                prepared
                    .env
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.as_str())
            };
            assert_eq!(get("NEOHAR"), Some("1"));
            assert_eq!(get("NEOHAR_HOOK_TEE"), Some(tee.to_string_lossy().as_ref()));
            // The environment of the user does not get in.
            for key in ["USER", "SHELL", "SSH_AUTH_SOCK", "ANTHROPIC_API_KEY_REAL"] {
                assert_eq!(get(key), None, "{key}");
            }
            match harness {
                Harness::Claude => {
                    assert_eq!(get("ANTHROPIC_BASE_URL"), Some("http://127.0.0.1:1234"));
                    assert!(prepared.config_dir.join(".claude.json").is_file());
                    assert!(prepared.config_dir.join("settings.json").is_file());
                }
                Harness::Codex => {
                    let config = fs::read_to_string(prepared.config_dir.join("config.toml"))
                        .expect("config.toml");
                    assert!(
                        config.contains("base_url = \"http://127.0.0.1:1234/v1\""),
                        "{config}"
                    );
                    assert!(config.contains("wire_api = \"responses\""));
                    assert!(prepared.config_dir.join("hooks.json").is_file());
                    // The recipe never writes the hook trust. The scenario trusts the hooks in
                    // the interface of Codex, as a user must.
                    assert!(!config.contains("hooks.state"));
                }
            }
        }
    }
}
