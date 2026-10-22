//! The tests use a fake `$SHELL`, so they do not depend on the shell of the person.

use std::{fs, os::unix::fs::PermissionsExt};

use super::*;

/// A directory with a fake shell, removed at the end.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("nh-login-env-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("a directory");
        Self(path)
    }

    /// A shell script that gets `-l -i -c <script>` and so has the script in `$4`.
    fn shell(&self, body: &str) -> PathBuf {
        let file = self.0.join("fake-shell");
        fs::write(&file, format!("#!/bin/sh\n{body}\n")).expect("a script");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).expect("permissions");
        file
    }

    fn base(&self) -> Vec<(String, String)> {
        vec![
            ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
            ("HOME".to_owned(), self.0.display().to_string()),
            ("FROM_APP".to_owned(), "kept".to_owned()),
        ]
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn get<'a>(env: &'a [(String, String)], name: &str) -> Option<&'a str> {
    env.iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

#[test]
fn the_environment_of_the_profile_comes_back_without_its_noise() {
    let dir = Dir::new("profile");
    let shell = dir.shell(
        "[ \"$1 $2 $3\" = '-l -i -c' ] || exit 9\n\
         echo 'Welcome! the profile prints this'\n\
         printf 'no newline'\n\
         FROM_PROFILE='two words=yes'; export FROM_PROFILE\n\
         PATH=/opt/fake/bin:$PATH; export PATH\n\
         exec /bin/sh -c \"$4\"",
    );
    let started = Instant::now();
    let env = capture_with(&shell, &dir.base(), TIMEOUT).expect("an environment");
    assert!(started.elapsed() < TIMEOUT);
    assert_eq!(get(&env, "FROM_PROFILE"), Some("two words=yes"));
    assert_eq!(get(&env, "FROM_APP"), Some("kept"));
    assert_eq!(get(&env, "PATH"), Some("/opt/fake/bin:/usr/bin:/bin"));
    // The variables of the shell itself are left out.
    for own in SHELL_OWN {
        assert_eq!(get(&env, own), None, "{own}");
    }
    assert!(!env.iter().any(|(key, _)| key.contains("Welcome")));
}

#[test]
fn the_shell_runs_in_the_home_directory_with_no_stdin() {
    let dir = Dir::new("home");
    let shell = dir.shell(
        "if read -r line; then exit 8; fi\n\
         HERE=$(pwd -P); export HERE\n\
         exec /bin/sh -c \"$4\"",
    );
    let env = capture_with(&shell, &dir.base(), TIMEOUT).expect("an environment");
    let home = dir.0.canonicalize().expect("the home directory");
    assert_eq!(get(&env, "HERE"), Some(home.display().to_string().as_str()));
}

#[test]
fn a_shell_that_hangs_is_stopped_at_the_limit() {
    let dir = Dir::new("hangs");
    let shell = dir.shell("sleep 20");
    let started = Instant::now();
    let error =
        capture_with(&shell, &dir.base(), Duration::from_millis(300)).expect_err("it is too slow");
    assert!(
        matches!(error, LoginEnvError::Timeout { millis: 300, .. }),
        "{error}"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_program_of_the_profile_that_keeps_the_output_open_does_not_hold_the_read() {
    let dir = Dir::new("background");
    let shell = dir.shell("sleep 5 &\nexec /bin/sh -c \"$4\"");
    let started = Instant::now();
    let env = capture_with(&shell, &dir.base(), TIMEOUT).expect("an environment");
    assert_eq!(get(&env, "FROM_APP"), Some("kept"));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_shell_that_fails_or_prints_nothing_is_an_error() {
    let dir = Dir::new("fails");
    let shell = dir.shell("echo 'command not found' ; exit 1");
    assert!(matches!(
        capture_with(&shell, &dir.base(), TIMEOUT),
        Err(LoginEnvError::NoEnvironment(_))
    ));
    assert!(matches!(
        capture_with(&dir.0.join("absent"), &dir.base(), TIMEOUT),
        Err(LoginEnvError::Start { .. })
    ));
}

#[test]
fn the_parser_takes_what_follows_the_sentinel_up_to_the_last_nul() {
    let sentinel = "__S__";
    let mut output = b"noise\n".to_vec();
    output.extend_from_slice(b"__S__PATH=/bin\0EMPTY=\0EQ=a=b\0=nameless\0noequals\0");
    output.extend_from_slice(&[b'B', b'=', 0xff, 0]);
    output.extend_from_slice(b"logout noise");
    let env = parse(&output, sentinel).expect("an environment");
    assert_eq!(
        env,
        [
            ("PATH".to_owned(), "/bin".to_owned()),
            ("EMPTY".to_owned(), String::new()),
            ("EQ".to_owned(), "a=b".to_owned()),
            ("B".to_owned(), "\u{fffd}".to_owned()),
        ]
    );
    // No sentinel, no NUL, or no PATH: no environment.
    assert_eq!(parse(b"PATH=/bin\0", sentinel), None);
    assert_eq!(parse(b"__S__PATH=/bin", sentinel), None);
    assert_eq!(parse(b"__S__HOME=/h\0", sentinel), None);
}

#[test]
fn the_sentinel_is_not_in_the_command_line() {
    let sentinel = sentinel();
    assert!(sentinel.starts_with("__NEOHAR_ENV_") && sentinel.len() > 30);
    assert!(!script(&sentinel).contains(&sentinel));
    assert_ne!(sentinel, super::sentinel());
}

#[test]
fn the_fallback_adds_the_usual_directories_to_the_path() {
    let base = vec![
        ("PATH".to_owned(), "/usr/bin:/usr/local/bin:/bin".to_owned()),
        ("HOME".to_owned(), "/Users/me".to_owned()),
    ];
    let env = fallback(&base);
    assert_eq!(
        get(&env, "PATH"),
        Some("/usr/bin:/usr/local/bin:/bin:/opt/homebrew/bin:/Users/me/.local/bin")
    );
    assert_eq!(get(&env, "HOME"), Some("/Users/me"));
    // Without a PATH and without a home.
    let env = fallback(&[]);
    assert_eq!(get(&env, "PATH"), Some("/opt/homebrew/bin:/usr/local/bin"));
}

#[test]
fn the_shell_is_the_one_of_the_environment() {
    let env = vec![("SHELL".to_owned(), "/opt/bin/fish".to_owned())];
    assert_eq!(shell_of(&env), PathBuf::from("/opt/bin/fish"));
    let relative = vec![("SHELL".to_owned(), "zsh".to_owned())];
    assert!(shell_of(&relative).is_absolute());
}

fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn an_app_that_the_bash_tool_of_claude_code_started_gives_its_variables_to_no_pane() {
    // The environment of `open` in the Bash tool of Claude Code 2.1.301, in a pane of a task.
    let env = without_inherited(vars(&[
        ("PATH", "/usr/bin:/bin"),
        ("HOME", "/Users/me"),
        ("AI_AGENT", "claude-code_2-1-301_agent"),
        ("CLAUDECODE", "1"),
        ("CLAUDE_CODE_CHILD_SESSION", "1"),
        ("CLAUDE_CODE_ENTRYPOINT", "cli"),
        (
            "CLAUDE_CODE_SESSION_ID",
            "4c7ae320-9a23-425a-91b6-f07df5203379",
        ),
        ("CLAUDE_PID", "2534"),
        ("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS", "1"),
        ("GIT_EDITOR", "true"),
        ("COREPACK_ENABLE_AUTO_PIN", "0"),
        ("NEOHAR", "1"),
        ("NEOHAR_PANE_ID", "w16:p4"),
        ("NEOHAR_PANE_TOKEN", "ac3d"),
        ("NEOHAR_TASK_ID", "k7"),
        ("NEOHAR_PORTS", "41000"),
        ("PORT", "41000"),
        ("TERM_PROGRAM", "neohar"),
    ]));
    // A setting of the person stays: Claude Code gives it from its `settings.json`.
    assert_eq!(
        env,
        vars(&[
            ("PATH", "/usr/bin:/bin"),
            ("HOME", "/Users/me"),
            ("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS", "1"),
            ("TERM_PROGRAM", "neohar"),
        ])
    );
}

#[test]
fn the_variables_that_a_person_can_set_stay_when_no_harness_and_no_task_started_the_app() {
    let base = vars(&[
        ("PATH", "/usr/bin:/bin"),
        ("GIT_EDITOR", "nvim"),
        ("COREPACK_ENABLE_AUTO_PIN", "0"),
        ("PORT", "3000"),
        ("CLAUDECODE", "0"),
    ]);
    let env = without_inherited(base.clone());
    assert_eq!(env, base[..4].to_vec());
    // The variables of a run of Codex go in all cases.
    let env = without_inherited(vars(&[
        ("GIT_EDITOR", "nvim"),
        ("CODEX_SANDBOX", "seatbelt"),
        ("CODEX_SANDBOX_NETWORK_DISABLED", "1"),
        ("CODEX_THREAD_ID", "019a"),
    ]));
    assert_eq!(env, vars(&[("GIT_EDITOR", "nvim")]));
}
