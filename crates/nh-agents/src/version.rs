//! The version of an installed harness.
//!
//! The command of the manifest runs with the login environment of the user, with a limit of
//! [`TIMEOUT`], and only once for each executable and modification time: the answer is cached. The
//! regular expression of the manifest takes the version out of what the command prints. The
//! version then says which capability ranges apply, and whether it is below the minimum.

use std::{
    collections::HashMap,
    io::Read,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex, PoisonError,
        mpsc::{self, Receiver, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

use regex::Regex;
use rustix::process::{self, Pid, Signal};
use semver::{Version, VersionReq};

use crate::manifest::{
    Manifest,
    schema::{CapabilityRange, VersionRule},
    validate::parse_version,
};

/// How long the version command may run. The first start of a new build of a harness can take
/// seconds while macOS checks the binary, and a probe that times out leaves the run without its
/// capabilities. The probe runs on a thread of its own, so a long limit stops nothing else.
pub const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum VersionError {
    #[error("{0} is not in the PATH")]
    NotFound(String),
    #[error("the version command does not name a program")]
    NoCommand,
    #[error("cannot run {program}: {reason}")]
    Run { program: String, reason: String },
    #[error("{program} did not print its version within {millis} ms")]
    Timeout { program: String, millis: u128 },
    #[error("the version regular expression is not right: {0}")]
    Regex(String),
    #[error("the output {0:?} has no version")]
    NoVersion(String),
}

/// What a probe found: the program, its version, and what it printed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Probe {
    pub exe: PathBuf,
    pub version: Version,
    pub output: String,
}

/// The versions that probes found, by executable and modification time. A program that is
/// replaced has another time, and gets a new probe.
#[derive(Debug, Default)]
pub struct VersionCache {
    found: Mutex<HashMap<(PathBuf, SystemTime), Probe>>,
}

impl VersionCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many probes the cache holds.
    pub fn len(&self) -> usize {
        self.found
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The version in a text, by the group `v` of the regular expression. A version with fewer than
/// three parts counts as the same version with zeros.
pub fn extract(regex: &Regex, text: &str) -> Option<Version> {
    let found = regex.captures(text)?.name("v")?.as_str();
    parse_version(found).ok()
}

fn is_executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// The executable that `name` means, looked for in the directories of `path`. A name with a slash
/// is the path itself.
pub fn find_exe(name: &str, path: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let file = PathBuf::from(name);
        return is_executable(&file).then_some(file);
    }
    path.split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| Path::new(dir).join(name))
        .find(|file| is_executable(file))
}

/// What a program printed, and how it ended.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Output {
    /// The exit status. `None` when a signal ended the program.
    pub exit: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// How long the read waits for the end of the outputs after the program ended. A program that
/// the harness started in the background (a wrapper script, the Node launcher of a native binary)
/// can keep a pipe open. After this time its process group is killed.
const DRAIN: Duration = Duration::from_millis(300);

/// How long the read waits for the end of the outputs after a kill of the process group. A
/// program that left the group can keep a pipe open: the result then has what came.
const AFTER_KILL: Duration = Duration::from_millis(200);

/// The two outputs of a program, which grow on their threads, so that a program that fills a pipe
/// does not wait for us, and the part that came is there when the end of a pipe does not come.
struct Pipes {
    stdout: Arc<Mutex<Vec<u8>>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    done: Receiver<()>,
    /// The pipes that did not end yet.
    open: usize,
}

impl Pipes {
    fn take(child: &mut Child) -> Self {
        let (tx, done) = mpsc::channel();
        let mut open = 0;
        let mut drain = |stream: Option<Box<dyn Read + Send>>| {
            let bytes = Arc::new(Mutex::new(Vec::new()));
            if let Some(mut stream) = stream {
                open += 1;
                let (bytes, tx) = (Arc::clone(&bytes), tx.clone());
                thread::spawn(move || {
                    let mut chunk = [0_u8; 8192];
                    while let Ok(n) = stream.read(&mut chunk) {
                        if n == 0 {
                            break;
                        }
                        bytes
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .extend_from_slice(&chunk[..n]);
                    }
                    let _ = tx.send(());
                });
            }
            bytes
        };
        let stdout = drain(
            child
                .stdout
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        );
        let stderr = drain(
            child
                .stderr
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        );
        Self {
            stdout,
            stderr,
            done,
            open,
        }
    }

    /// Waits until both pipes ended, or until `within` passed. Gives whether they ended.
    fn wait(&mut self, within: Duration) -> bool {
        let until = Instant::now() + within;
        while self.open > 0 {
            let left = until.saturating_duration_since(Instant::now());
            match self.done.recv_timeout(left) {
                Ok(()) => self.open -= 1,
                Err(RecvTimeoutError::Timeout) => return false,
                Err(RecvTimeoutError::Disconnected) => self.open = 0,
            }
        }
        true
    }

    fn text(bytes: &Mutex<Vec<u8>>) -> String {
        String::from_utf8_lossy(&bytes.lock().unwrap_or_else(PoisonError::into_inner)).into_owned()
    }
}

/// Kills the process group that the program leads, so that the programs that it started end with
/// it. The group outlives its leader while a member lives, so its id is not used again.
fn kill_group(group: Option<Pid>) {
    if let Some(group) = group {
        let _ = process::kill_process_group(group, Signal::KILL);
    }
}

/// Runs a program with no stdin and gives what it printed on both outputs. The program ends with a
/// kill when it takes longer than `limit`. An empty `env` means the environment of this process.
///
/// The program leads a new process group. The kill at the limit goes to the group, so that the
/// programs that it started end too. When the program ended and a program that it started keeps an
/// output open for longer than [`DRAIN`], that group is killed as well. So the call takes at most
/// `limit` and a fraction of a second, and leaves no program of the group behind.
pub(crate) fn run_output(
    program: &Path,
    args: &[String],
    env: &[(String, String)],
    limit: Duration,
) -> Result<Output, VersionError> {
    let name = program.display().to_string();
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    if !env.is_empty() {
        command.env_clear().envs(env.iter().map(|(k, v)| (k, v)));
    }
    let mut child = command.spawn().map_err(|e| VersionError::Run {
        program: name.clone(),
        reason: e.to_string(),
    })?;
    let group = i32::try_from(child.id()).ok().and_then(Pid::from_raw);
    let mut pipes = Pipes::take(&mut child);
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= limit => {
                kill_group(group);
                let _ = child.kill();
                let _ = child.wait();
                return Err(VersionError::Timeout {
                    program: name,
                    millis: limit.as_millis(),
                });
            }
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(error) => {
                kill_group(group);
                let _ = child.kill();
                let _ = child.wait();
                return Err(VersionError::Run {
                    program: name,
                    reason: error.to_string(),
                });
            }
        }
    };
    if !pipes.wait(DRAIN) {
        kill_group(group);
        pipes.wait(AFTER_KILL);
    }
    Ok(Output {
        exit: status.code(),
        stdout: Pipes::text(&pipes.stdout),
        stderr: Pipes::text(&pipes.stderr),
    })
}

/// Runs a program and gives what it printed on both outputs.
fn run(
    program: &Path,
    args: &[String],
    env: &[(String, String)],
    limit: Duration,
) -> Result<String, VersionError> {
    let output = run_output(program, args, env, limit)?;
    let mut text = output.stdout;
    text.push_str(&output.stderr);
    Ok(text)
}

/// The `PATH` of an environment. An empty `env` means the environment of this process.
pub fn path_of(env: &[(String, String)]) -> String {
    env.iter()
        .find(|(key, _)| key == "PATH")
        .map(|(_, value)| value.clone())
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default()
}

/// Finds the version of the harness that a manifest describes. `env` is the login environment:
/// the program is looked for in its `PATH`, and it runs with it. An empty `env` means the
/// environment of this process. A probe that worked is cached, and the next call for the same
/// executable and time gives it back without a run.
pub fn probe_with_limit(
    rule: &VersionRule,
    env: &[(String, String)],
    cache: &VersionCache,
    limit: Duration,
) -> Result<Probe, VersionError> {
    let program = rule.command.first().ok_or(VersionError::NoCommand)?;
    let regex = Regex::new(&rule.regex).map_err(|e| VersionError::Regex(e.to_string()))?;
    let exe =
        find_exe(program, &path_of(env)).ok_or_else(|| VersionError::NotFound(program.clone()))?;
    let modified = exe
        .metadata()
        .and_then(|meta| meta.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let key = (exe.clone(), modified);
    if let Some(hit) = cache
        .found
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
    {
        return Ok(hit.clone());
    }
    let output = run(&exe, &rule.command[1..], env, limit)?;
    let version = extract(&regex, &output)
        .ok_or_else(|| VersionError::NoVersion(output.trim().to_owned()))?;
    let probe = Probe {
        exe,
        version,
        output: output.trim().to_owned(),
    };
    cache
        .found
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key, probe.clone());
    Ok(probe)
}

/// [`probe_with_limit`] with the limit of [`TIMEOUT`].
pub fn probe(
    rule: &VersionRule,
    env: &[(String, String)],
    cache: &VersionCache,
) -> Result<Probe, VersionError> {
    probe_with_limit(rule, env, cache, TIMEOUT)
}

impl Manifest {
    /// The version below which the capabilities are reduced to `observe`.
    pub fn minimum(&self) -> Option<Version> {
        self.min_version
            .as_deref()
            .and_then(|v| parse_version(v).ok())
    }

    /// Whether a version meets the minimum. A manifest with no minimum accepts every version.
    pub fn meets_minimum(&self, version: &Version) -> bool {
        self.minimum().is_none_or(|minimum| *version >= minimum)
    }

    /// The capability ranges that cover `version` in `mode`. The golden recordings decide which
    /// ranges exist, so a version that no range covers is a version that no recording covers.
    pub fn capability_ranges(&self, version: &Version, mode: &str) -> Vec<&CapabilityRange> {
        self.capabilities
            .iter()
            .filter(|range| range.mode == mode)
            .filter(|range| {
                VersionReq::parse(&range.versions).is_ok_and(|req| req.matches(version))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, time::Duration};

    use super::*;

    /// A directory with programs for a test, removed at the end.
    struct Bin(PathBuf);

    impl Bin {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("nh-agents-bin-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("a directory");
            Self(path)
        }

        /// A shell script that is a program.
        fn program(&self, name: &str, body: &str) -> PathBuf {
            let file = self.0.join(name);
            fs::write(&file, format!("#!/bin/sh\n{body}\n")).expect("a script");
            let mut permissions = fs::metadata(&file).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&file, permissions).expect("permissions");
            file
        }

        fn path(&self) -> String {
            format!("{}:/usr/bin:/bin", self.0.display())
        }
    }

    impl Drop for Bin {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn rule(program: &str) -> VersionRule {
        VersionRule {
            command: vec![program.to_owned(), "--version".to_owned()],
            regex: r"^(?P<v>\d+\.\d+\.\d+)".to_owned(),
        }
    }

    fn env(bin: &Bin) -> Vec<(String, String)> {
        vec![("PATH".to_owned(), bin.path())]
    }

    #[test]
    fn a_version_comes_out_of_what_the_program_prints() {
        let bin = Bin::new("prints");
        bin.program("claude", "echo '2.1.292 (Claude Code)'");
        let probe = probe(&rule("claude"), &env(&bin), &VersionCache::new()).expect("a version");
        assert_eq!(probe.version.to_string(), "2.1.292");
        assert_eq!(probe.output, "2.1.292 (Claude Code)");
        assert!(probe.exe.ends_with("claude"));
    }

    #[test]
    fn a_version_on_the_error_output_counts() {
        let bin = Bin::new("stderr");
        bin.program("tool", "echo '0.156.1' >&2");
        let probe = probe(&rule("tool"), &env(&bin), &VersionCache::new()).expect("a version");
        assert_eq!(probe.version.to_string(), "0.156.1");
    }

    #[test]
    fn the_program_runs_once_for_an_executable_and_its_time() {
        let bin = Bin::new("cache");
        let counter = bin.0.join("count");
        let body = format!("echo run >> {}\necho 1.2.3", counter.display());
        let file = bin.program("tool", &body);
        let cache = VersionCache::new();
        for _ in 0..3 {
            probe(&rule("tool"), &env(&bin), &cache).expect("a version");
        }
        let runs = || {
            fs::read_to_string(&counter)
                .expect("a count")
                .lines()
                .count()
        };
        assert_eq!(runs(), 1);
        assert_eq!(cache.len(), 1);
        // A new file has another time, so it is probed again.
        let later = SystemTime::now() + Duration::from_secs(60);
        fs::File::options()
            .write(true)
            .open(&file)
            .expect("a file")
            .set_modified(later)
            .expect("a time");
        probe(&rule("tool"), &env(&bin), &cache).expect("a version");
        assert_eq!(runs(), 2);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn the_program_gets_the_environment_that_it_is_given() {
        let bin = Bin::new("env");
        bin.program("tool", "echo \"$NH_WHAT.0.0\"");
        let mut vars = env(&bin);
        vars.push(("NH_WHAT".to_owned(), "7".to_owned()));
        let found = probe(&rule("tool"), &vars, &VersionCache::new()).expect("a version");
        assert_eq!(found.version.to_string(), "7.0.0");
        // The environment of this process is not passed on when one is given.
        let bin = Bin::new("env-clear");
        bin.program("tool2", "echo \"${HOME:-0}.0.0\"");
        let found = probe(&rule("tool2"), &env(&bin), &VersionCache::new()).expect("a version");
        assert_eq!(found.version.to_string(), "0.0.0");
    }

    #[test]
    fn a_program_that_takes_too_long_is_stopped() {
        let bin = Bin::new("slow");
        bin.program("tool", "sleep 5\necho 1.0.0");
        let started = Instant::now();
        let error = probe_with_limit(
            &rule("tool"),
            &env(&bin),
            &VersionCache::new(),
            Duration::from_millis(200),
        )
        .expect_err("it is too slow");
        assert!(
            matches!(error, VersionError::Timeout { millis: 200, .. }),
            "{error:?}"
        );
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    /// The pid that a program wrote to a file.
    fn pid_in(file: &Path) -> i32 {
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(pid) = fs::read_to_string(file)
                .ok()
                .and_then(|text| text.trim().parse().ok())
            {
                return pid;
            }
            assert!(Instant::now() < until, "no pid in {}", file.display());
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Whether the process `pid` ends within a short time. A program that the kill of the group
    /// ended is an orphan, and it is a zombie until the system reaps it. A process that stays gets
    /// a kill here, so that the test leaves nothing behind.
    fn ends_soon(pid: i32) -> bool {
        let pid = Pid::from_raw(pid).expect("a pid");
        let until = Instant::now() + Duration::from_secs(3);
        while Instant::now() < until {
            if process::test_kill_process(pid).is_err() {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let _ = process::kill_process(pid, Signal::KILL);
        false
    }

    #[test]
    fn a_wrapper_whose_child_keeps_the_output_open_returns_and_leaves_no_child() {
        let bin = Bin::new("wrapper");
        let pid_file = bin.0.join("pid");
        // The npm `codex` and the shims of a shell start the real program and can end first.
        bin.program(
            "tool",
            &format!("sleep 60 &\necho $! > {}\necho 1.2.3", pid_file.display()),
        );
        let limit = Duration::from_secs(5);
        let started = Instant::now();
        let output = run_output(&bin.0.join("tool"), &[], &env(&bin), limit).expect("an output");
        let took = started.elapsed();
        assert_eq!(output.stdout, "1.2.3\n");
        assert_eq!(output.exit, Some(0));
        // It returns before the limit and does not wait for the child. The first start of a new
        // script can take a second while macOS checks it, so the bound is the limit itself.
        assert!(took < limit, "{took:?}");
        assert!(
            ends_soon(pid_in(&pid_file)),
            "the child of the wrapper stays"
        );
    }

    #[test]
    fn a_program_that_hangs_past_the_limit_ends_with_its_children() {
        let bin = Bin::new("hangs");
        let pid_file = bin.0.join("pid");
        bin.program(
            "tool",
            &format!(
                "sleep 60 &\necho $! > {}\necho started\nsleep 60",
                pid_file.display()
            ),
        );
        // The limit leaves the shell time to start its child on a busy machine.
        let limit = Duration::from_millis(1500);
        let started = Instant::now();
        let error = run_output(&bin.0.join("tool"), &[], &env(&bin), limit).expect_err("it hangs");
        let took = started.elapsed();
        assert!(
            matches!(error, VersionError::Timeout { millis: 1500, .. }),
            "{error:?}"
        );
        assert!(took < limit + Duration::from_secs(2), "{took:?}");
        assert!(
            ends_soon(pid_in(&pid_file)),
            "the child of the program stays"
        );
    }

    #[test]
    fn the_errors_say_what_is_wrong() {
        let bin = Bin::new("errors");
        bin.program("nothing", "echo 'no version here'");
        let cache = VersionCache::new();
        assert!(matches!(
            probe(&rule("absent"), &env(&bin), &cache),
            Err(VersionError::NotFound(name)) if name == "absent"
        ));
        assert!(matches!(
            probe(&rule("nothing"), &env(&bin), &cache),
            Err(VersionError::NoVersion(text)) if text == "no version here"
        ));
        let empty = VersionRule {
            command: vec![],
            regex: String::new(),
        };
        assert_eq!(probe(&empty, &[], &cache), Err(VersionError::NoCommand));
        let broken = VersionRule {
            command: vec!["nothing".to_owned()],
            regex: "(".to_owned(),
        };
        assert!(matches!(
            probe(&broken, &env(&bin), &cache),
            Err(VersionError::Regex(_))
        ));
        // A failure is not cached.
        assert!(cache.is_empty());
    }

    #[test]
    fn a_program_is_found_by_name_or_by_path() {
        let bin = Bin::new("find");
        let file = bin.program("tool", "true");
        assert_eq!(find_exe("tool", &bin.path()), Some(file.clone()));
        assert_eq!(find_exe(&file.display().to_string(), ""), Some(file));
        assert_eq!(find_exe("absent", &bin.path()), None);
        // A file without the execute bit is not a program.
        fs::write(bin.0.join("data"), "x").expect("a file");
        assert_eq!(find_exe("data", &bin.path()), None);
    }

    #[test]
    fn the_version_decides_the_minimum_and_the_capability_ranges() {
        let loaded = crate::builtin();
        let claude = loaded.get("claude").expect("claude");
        let v = |text: &str| Version::parse(text).expect("a version");
        assert!(claude.meets_minimum(&v("2.1.0")));
        assert!(claude.meets_minimum(&v("2.1.292")));
        assert!(!claude.meets_minimum(&v("2.0.9")));
        // The recording covers 2.1.x, so 2.1.292 has a range, and 2.2.0 has none.
        assert_eq!(claude.capability_ranges(&v("2.1.292"), "tui").len(), 1);
        assert!(claude.capability_ranges(&v("2.2.0"), "tui").is_empty());
        assert!(
            claude
                .capability_ranges(&v("2.1.292"), "headless")
                .is_empty()
        );
        let codex = loaded.get("codex").expect("codex");
        assert_eq!(codex.capability_ranges(&v("0.156.1"), "tui").len(), 1);
        // The installed Codex is newer than the recordings, so no range covers it (unvalidated).
        assert!(codex.capability_ranges(&v("0.162.0"), "tui").is_empty());
        assert!(codex.meets_minimum(&v("0.162.0")));
    }
}
