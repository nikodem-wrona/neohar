//! The recorder: starts the mock model, the API socket and the harness, and runs the steps.
//!
//! The harness runs under a real holder, as in a pane. The recorder is the only client of the
//! holder. It feeds every output event to a VT engine of `nh-term` and writes the replies that the
//! engine makes to terminal queries back as input, as the core does. `wait_screen` looks at the
//! rows of that engine, so a step waits for what a person would see.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use mock_llm::LogSink;
use nh_proto::{
    frame::Palette,
    holder::{ExitStatus, HolderEvent, HolderRequest, Marker, StreamId},
};
use nh_record::holder::{Connection, HolderProcess, HolderSpec, Scratch};
use nh_term::{
    AlacrittyEngine, Effect, MIN_HISTORY, Recorder, ScreenDump, VtEngine, xterm_palette,
};
use regex::Regex;

use crate::{
    GoldenError,
    apisock::ApiSocket,
    cast::CastWriter,
    corectl::CoreCtl,
    keys::key_bytes,
    meta::{Meta, blake3_hex, describe_exit, os_description},
    procs,
    recipe::{self, Harness, Prepared, Setup},
    sandbox,
    scenario::{Action, CoreOp, Mode, Scenario, Step},
};

mod pipe;

use pipe::run_pipe;

/// How long the output must be quiet for a `snapshot` step and for the end of a recording.
const QUIET: Duration = Duration::from_millis(400);
/// The longest wait for that quiet at the end of a recording.
const SETTLE_MAX: Duration = Duration::from_secs(5);
/// How often a wait looks again when no output comes.
const POLL: Duration = Duration::from_millis(25);
/// How long the API socket may lag behind the tee file at the end of a recording.
const API_GRACE: Duration = Duration::from_secs(2);

/// What to record.
pub struct Request<'a> {
    pub harness: Harness,
    pub scenario: &'a Scenario,
    /// The bytes of the scenario file, for the hash and for the copy in the recording.
    pub scenario_text: &'a str,
    pub scenario_name: &'a str,
    /// The `nh` binary: it runs the holder and it is the hook shim.
    pub nh: &'a Path,
    /// The binary of the harness.
    pub binary: &'a Path,
    /// The scratch tree. The working directory of the harness is inside it and shows on its
    /// screen, so a fixed root makes recordings that repeat. Only one recording per root runs.
    pub root: &'a Path,
}

/// A recording, in memory. [`Recording::write`] saves it.
pub struct Recording {
    pub meta: Meta,
    /// The bytes of the session file: `session.cast` in a terminal, `session.jsonl` with pipes.
    pub session: Vec<u8>,
    pub session_file: &'static str,
    /// The lines of `signals.jsonl`, as the shim wrote them.
    pub signals: String,
    /// The lines of `llm-requests.jsonl`, as the mock model wrote them. [`Recording::write`]
    /// keeps only what [`crate::scrub::requests`] keeps.
    pub llm_requests: String,
    /// The transcript files that the harness wrote (`transcripts/transcript-<n>.jsonl`), in the
    /// order of their names. They are fixtures for the parsers of the adapters.
    /// [`Recording::write`] keeps only what [`crate::scrub::transcript`] keeps.
    pub transcripts: Vec<String>,
    /// The screens, by name, in the order of the scenario. The last is `final`.
    pub screens: Vec<(String, String)>,
    /// The process lists of the `processes` steps, by name.
    pub processes: Vec<(String, String)>,
    /// More files, by path in the recording (`core/events.jsonl`, `core/interactions.json`).
    pub extras: Vec<(String, String)>,
    pub scenario_text: String,
}

impl Recording {
    /// Writes the files of the recording into `out`. It replaces the files of an older recording
    /// there and leaves other files alone.
    pub fn write(&self, out: &Path) -> Result<(), GoldenError> {
        let put = |name: &str, text: &[u8]| {
            let path = out.join(name);
            fs::write(&path, text).map_err(GoldenError::io(format!("write {}", path.display())))
        };
        let screens = out.join("screens");
        let _ = fs::remove_dir_all(&screens);
        fs::create_dir_all(&screens)
            .map_err(GoldenError::io(format!("create {}", screens.display())))?;
        put(self.session_file, &self.session)?;
        put("signals.jsonl", self.signals.as_bytes())?;
        put(
            "llm-requests.jsonl",
            crate::scrub::requests(&self.llm_requests).as_bytes(),
        )?;
        put("scenario.toml", self.scenario_text.as_bytes())?;
        put("meta.toml", self.meta.to_toml()?.as_bytes())?;
        if !self.transcripts.is_empty() {
            let dir = out.join("transcripts");
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir)
                .map_err(GoldenError::io(format!("create {}", dir.display())))?;
            for (index, text) in self.transcripts.iter().enumerate() {
                let path = dir.join(format!("transcript-{}.jsonl", index + 1));
                fs::write(&path, crate::scrub::transcript(text))
                    .map_err(GoldenError::io(format!("write {}", path.display())))?;
            }
        }
        for (path, text) in &self.extras {
            let target = out.join(path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(GoldenError::io(format!("create {}", parent.display())))?;
            }
            fs::write(&target, text)
                .map_err(GoldenError::io(format!("write {}", target.display())))?;
        }
        let processes = out.join("processes");
        let _ = fs::remove_dir_all(&processes);
        if !self.processes.is_empty() {
            fs::create_dir_all(&processes)
                .map_err(GoldenError::io(format!("create {}", processes.display())))?;
            for (name, text) in &self.processes {
                let path = processes.join(format!("{name}.txt"));
                fs::write(&path, format!("{text}\n"))
                    .map_err(GoldenError::io(format!("write {}", path.display())))?;
            }
        }
        for (name, text) in &self.screens {
            let path = screens.join(format!("{name}.txt"));
            fs::write(&path, format!("{text}\n"))
                .map_err(GoldenError::io(format!("write {}", path.display())))?;
        }
        Ok(())
    }
}

/// A claim on a scratch root, so that two recordings never share one. It is an advisory lock on a
/// file next to the root. The operating system drops the lock when the process ends, so a crash
/// leaves nothing to clean up. A second recording waits for the first for up to [`LOCK_WAIT`].
struct RootLock {
    /// The open file holds the lock. Closing it releases the lock.
    _file: fs::File,
}

/// How long a recording waits for another recording of the same harness.
const LOCK_WAIT: Duration = Duration::from_secs(180);

impl RootLock {
    fn acquire(root: &Path) -> Result<Self, GoldenError> {
        Self::acquire_within(root, LOCK_WAIT)
    }

    fn acquire_within(root: &Path, wait: Duration) -> Result<Self, GoldenError> {
        let path = PathBuf::from(format!("{}.lock", root.display()));
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .read(true)
            .open(&path)
            .map_err(GoldenError::io(format!("open {}", path.display())))?;
        let deadline = Instant::now() + wait;
        let mut told = false;
        loop {
            match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
                Ok(()) => break,
                Err(rustix::io::Errno::WOULDBLOCK) => {}
                Err(e) => {
                    return Err(GoldenError::io(format!("lock {}", path.display()))(
                        e.into(),
                    ));
                }
            }
            let owner = fs::read_to_string(&path).unwrap_or_default();
            if Instant::now() >= deadline {
                return Err(GoldenError::Setup(format!(
                    "another recording uses {} (process {}) and did not end in {wait:?}",
                    root.display(),
                    owner.trim()
                )));
            }
            if !told {
                eprintln!(
                    "nh-golden: another recording uses {}. Waiting for it to end",
                    root.display()
                );
                told = true;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        // The pid is for the message of a waiting recording.
        let _ = file.set_len(0);
        let _ = writeln!(file, "{}", std::process::id());
        Ok(Self { _file: file })
    }
}

/// Runs `argv` and returns its output, for `--version`.
fn run_for_output(
    argv: &[String],
    env: &[(String, String)],
    cwd: &Path,
) -> Result<String, GoldenError> {
    let (program, args) = argv
        .split_first()
        .ok_or_else(|| GoldenError::Setup("there is no program to run".into()))?;
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .output()
        .map_err(GoldenError::io(format!("run {program}")))?;
    if !output.status.success() {
        return Err(GoldenError::Setup(format!(
            "{} failed with {}:\n{}{}",
            argv.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// What a run of the harness needs, whatever the mode.
struct RunContext<'a> {
    request: &'a Request<'a>,
    /// The harness under the sandbox, with the arguments of the scenario.
    sandboxed: Vec<String>,
    /// The harness under the sandbox, without arguments. A peer adds its own.
    sandboxed_base: Vec<String>,
    env: Vec<(String, String)>,
    work: PathBuf,
    tee_dir: PathBuf,
    canonical_root: PathBuf,
    config_dir: PathBuf,
    harness_version: String,
    llm_log: PathBuf,
    real_home: PathBuf,
    started: Instant,
    /// The directories whose paths in a command line mark a process of this recording.
    proc_roots: Vec<String>,
    /// Replacements that make a command line the same on every machine.
    proc_replacements: Vec<(String, String)>,
}

/// Kills the processes that still have the scratch root in their command line when it goes out of
/// scope: a daemon that left the process group of the harness.
struct Leftovers {
    roots: Vec<String>,
}

impl Drop for Leftovers {
    fn drop(&mut self) {
        procs::kill_with_root(&self.roots);
    }
}

/// The services that run beside the harness. The end of a recording stops them.
struct Services {
    /// The small socket of the recorder. `None` when the scenario has a core.
    api: Option<ApiSocket>,
    /// The core of a delivery scenario.
    core: Option<CoreCtl>,
    server: mock_llm::Server,
    runtime: tokio::runtime::Runtime,
}

/// What a run of the harness left, before the files are made.
struct Outcome {
    /// The bytes of `session.cast` or `session.jsonl`.
    session: Vec<u8>,
    session_file: &'static str,
    screens: Vec<(String, String)>,
    /// The process lists of the `processes` steps, by name.
    processes: Vec<(String, String)>,
    exit: Option<ExitStatus>,
    replies: usize,
}

/// Whether one result of a race is `Ok` and the other is the error `already_answered`.
fn code_is_already_answered(
    a: &Result<serde_json::Value, String>,
    b: &Result<serde_json::Value, String>,
) -> bool {
    match (a, b) {
        (Ok(_), Err(code)) | (Err(code), Ok(_)) => code == "already_answered",
        _ => false,
    }
}

/// Records the scenario.
pub fn record(request: &Request<'_>) -> Result<Recording, GoldenError> {
    record_chain(&[request])
}

/// How a run treats the scratch tree.
#[derive(Clone, Copy)]
struct Tree {
    /// The run makes a new tree (and removes an old one). Otherwise it takes over the tree that
    /// the run before it kept.
    fresh: bool,
    /// The tree stays when the run ends, for the run after it.
    keep: bool,
}

/// Records the last request of the chain. The requests before it run first, in the same scratch
/// tree, and nothing of them is recorded (`after` in a scenario file). All requests need the same
/// root and the same harness. The chain holds the lock on the root from the first run to the last.
pub fn record_chain(chain: &[&Request<'_>]) -> Result<Recording, GoldenError> {
    let Some(first) = chain.first() else {
        return Err(GoldenError::Setup("there is no scenario to record".into()));
    };
    if chain
        .iter()
        .any(|r| r.root != first.root || r.harness != first.harness)
    {
        return Err(GoldenError::Setup(
            "the scenarios of a chain need the same harness and the same root".into(),
        ));
    }
    if !sandbox::available() {
        return Err(GoldenError::Setup(
            "a recording needs macOS and /usr/bin/sandbox-exec: the sandbox is what keeps the \
             harness away from the real configuration of the user"
                .into(),
        ));
    }
    let _lock = RootLock::acquire(first.root)?;
    let mut last_recording = None;
    for (index, request) in chain.iter().enumerate() {
        let tree = Tree {
            fresh: index == 0,
            keep: index + 1 < chain.len(),
        };
        last_recording = Some(record_run(request, tree)?);
    }
    last_recording.ok_or_else(|| GoldenError::Setup("there is no scenario to record".into()))
}

/// One run of one scenario.
fn record_run(request: &Request<'_>, tree: Tree) -> Result<Recording, GoldenError> {
    if let Some(name) = &request.scenario.harness
        && name != request.harness.name()
    {
        return Err(GoldenError::Setup(format!(
            "the scenario is for {name}, not for {}",
            request.harness.name()
        )));
    }
    let started = Instant::now();
    let mut scratch = if tree.fresh {
        Scratch::create_at(request.root.to_owned())?
    } else {
        Scratch::reuse_at(request.root.to_owned())?
    };
    if tree.keep {
        scratch.keep_on_drop();
    }
    let tee_dir = scratch.root.join("tee");
    let api_path = scratch.run.join("api.sock");
    // A Unix socket path holds about 100 bytes.
    if api_path.display().to_string().len() > 90 {
        return Err(GoldenError::Setup(format!(
            "the root {} is too long for a socket path",
            request.root.display()
        )));
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(GoldenError::io("start the runtime of the mock model"))?;
    let llm_log = scratch.root.join("llm-requests.jsonl");
    let sink = LogSink::file(&llm_log).map_err(GoldenError::io("create the request log"))?;
    let server = runtime
        .block_on(mock_llm::serve(
            request.scenario.mock.clone(),
            sink,
            std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
        ))
        .map_err(GoldenError::io("start the mock model"))?;
    let (api, core, api_path) = match &request.scenario.core {
        Some(settings) => {
            let core = CoreCtl::start(&scratch, request.nh, runtime.handle().clone(), settings)?;
            let socket = core.socket()?;
            (None, Some(core), socket)
        }
        None => (
            Some(ApiSocket::start(&api_path).map_err(GoldenError::io("start the API socket"))?),
            None,
            api_path,
        ),
    };

    let mock_url = server.url();
    let Prepared {
        argv,
        env,
        config_dir,
    } = recipe::prepare(&Setup {
        harness: request.harness,
        binary: request.binary,
        nh: request.nh,
        mock_url: &mock_url,
        scratch: &scratch,
        tee_dir: &tee_dir,
        api_socket: &api_path,
        gate: request.scenario.core.is_some(),
        args: &request.scenario.args,
        files: &request.scenario.files,
        hook_marker: request.scenario.hook_marker.as_deref(),
        settings: &request.scenario.settings,
        state: &request.scenario.state,
        hook_replies: &request.scenario.hook_replies,
    })?;

    // The sandbox: the scratch tree is the only place to write, and the real directories of the
    // harnesses are closed.
    let real_home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| GoldenError::Setup("HOME is not set".into()))?;
    let canonical_root =
        fs::canonicalize(&scratch.root).map_err(GoldenError::io("resolve the root"))?;
    let extra_writable: Vec<PathBuf> = request
        .scenario
        .writable
        .iter()
        .map(|path| {
            PathBuf::from(path.replace("{uid}", &rustix::process::getuid().as_raw().to_string()))
        })
        .collect();
    let profile = sandbox::profile(
        &canonical_root,
        &extra_writable,
        &sandbox::real_harness_paths(&real_home),
    );
    let profile_path = scratch.root.join("sandbox.sb");
    fs::write(&profile_path, &profile).map_err(GoldenError::io("write the sandbox profile"))?;
    let sandboxed = sandbox::wrap(&profile_path, &argv);

    // The version, from `--version` without the arguments of the scenario.
    let mut version_argv = sandbox::wrap(&profile_path, &argv[..1]);
    version_argv.push("--version".into());
    let version_output = run_for_output(&version_argv, &env, &scratch.work)?;
    let harness_version = request
        .harness
        .parse_version(&version_output)
        .ok_or_else(|| {
            GoldenError::Setup(format!(
                "cannot read a {} version from {version_output:?}",
                request.harness.name()
            ))
        })?;

    let root_texts: Vec<String> = [request.root.to_owned(), canonical_root.clone()]
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    let mut proc_replacements: Vec<(String, String)> = root_texts
        .iter()
        .map(|r| (r.clone(), "<scratch>".to_owned()))
        .collect();
    proc_replacements.extend([
        (request.nh.display().to_string(), "<nh>".to_owned()),
        (
            recipe::golden_binary().display().to_string(),
            "<golden>".to_owned(),
        ),
        (real_home.display().to_string(), "<home>".to_owned()),
        (request.binary.display().to_string(), "<harness>".to_owned()),
    ]);
    // The longest path first, so that `/private/tmp/…` is not cut at `/tmp/…`.
    proc_replacements.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
    let _leftovers = Leftovers {
        roots: root_texts.clone(),
    };
    let sandboxed_base = sandbox::wrap(&profile_path, &argv[..1]);
    let context = RunContext {
        request,
        sandboxed,
        sandboxed_base,
        env,
        work: scratch.work.clone(),
        tee_dir,
        canonical_root,
        config_dir,
        harness_version,
        llm_log,
        real_home,
        started,
        proc_roots: root_texts,
        proc_replacements,
    };
    let services = Services {
        api,
        core,
        server,
        runtime,
    };
    match request.scenario.mode {
        Mode::Pty => run_pty(&context, scratch, services),
        Mode::Pipe => run_pipe(&context, scratch, services),
    }
}

/// Runs the harness in a terminal, under a holder.
fn run_pty(
    ctx: &RunContext<'_>,
    scratch: Scratch,
    mut services: Services,
) -> Result<Recording, GoldenError> {
    let request = ctx.request;
    let (cols, rows) = (request.scenario.cols, request.scenario.rows);
    let mut holder = HolderProcess::new(scratch)?;
    // Declared after the holder, so that it runs first when the recording fails: the tree is still
    // there to copy.
    let mut keeper = FailureKeeper::new(&ctx.canonical_root);
    holder.launch(&HolderSpec {
        nh: request.nh,
        argv: &ctx.sandboxed,
        env: &ctx.env,
        cols,
        rows,
    })?;
    let mut conn = holder.connect()?;
    conn.send(&HolderRequest::Attach { from_offset: 0 })?;

    let title = format!(
        "{} {}: {}",
        request.harness.name(),
        ctx.harness_version,
        request.scenario_name
    );
    let cast = CastWriter::new(Vec::new(), cols, rows, &title, "xterm-256color")
        .map_err(GoldenError::io("start the cast"))?;
    let theme = xterm_palette();
    let mut session = Session {
        conn,
        holder: &mut holder,
        engine: AlacrittyEngine::new(cols, rows, MIN_HISTORY, theme.clone()),
        theme,
        fx: Recorder::default(),
        cast,
        begin: Instant::now(),
        last_output: Instant::now(),
        exit: None,
        next_input_id: 1,
        replies: 0,
        screen_cache: None,
        snapshots: Vec::new(),
        tee_file: ctx.tee_dir.join("signals.jsonl"),
        signal_cursor: 0,
        proc_roots: ctx.proc_roots.clone(),
        proc_replacements: ctx.proc_replacements.clone(),
        processes: Vec::new(),
        core: services.core.take(),
    };
    let outcome = request
        .scenario
        .steps
        .iter()
        .enumerate()
        .try_for_each(|(index, step)| session.run_step(index + 1, step))
        .and_then(|()| session.settle());
    if let Err(error) = outcome {
        // The files of a failed recording include what the harness printed until the failure.
        let _ = fs::write(
            ctx.canonical_root.join("failed-session.cast"),
            session.cast.get_ref(),
        );
        let screen = session.screen_text();
        let _ = fs::write(
            ctx.canonical_root.join("failed-screen.txt"),
            format!("{screen}\n"),
        );
        if let Some(core) = &session.core {
            let _ = fs::write(
                ctx.canonical_root.join("failed-core-events.jsonl"),
                core.events_text(),
            );
            let _ = fs::write(
                ctx.canonical_root.join("failed-core-interactions.json"),
                core.interactions_text(),
            );
        }
        return Err(error);
    }
    let final_screen = session.screen_text();
    session.snapshots.push(("final".to_owned(), final_screen));
    if session.exit.is_some() {
        // The holder may exit now. The guard kills it if it does not.
        let _ = session.conn.send(&HolderRequest::ExitAck);
    }
    let exit = session.exit;
    let replies = session.replies;
    let snapshots = std::mem::take(&mut session.snapshots);
    let processes = std::mem::take(&mut session.processes);
    services.core = session.core.take();
    let cast = session
        .cast
        .finish()
        .map_err(GoldenError::io("finish the cast"))?;
    let recording = finish(
        ctx,
        services,
        Outcome {
            session: cast,
            session_file: "session.cast",
            screens: snapshots,
            processes,
            exit,
            replies,
        },
    )?;
    keeper.succeeded();
    Ok(recording)
}

/// Makes the files of a recording from what the run left. It runs before the scratch tree goes,
/// because the hook tee, the request log and the transcripts are in it.
fn finish(
    ctx: &RunContext<'_>,
    services: Services,
    outcome: Outcome,
) -> Result<Recording, GoldenError> {
    let request = ctx.request;
    let Services {
        api,
        mut core,
        server,
        runtime,
    } = services;
    // The hooks that run late are in the tee file. Give the API socket (or the core) time to see
    // the same ones.
    let signals = fs::read_to_string(ctx.tee_dir.join("signals.jsonl")).unwrap_or_default();
    let tee_calls = signals.lines().filter(|l| !l.trim().is_empty()).count();
    let deadline = Instant::now() + API_GRACE;
    let seen = |api: &Option<ApiSocket>, core: &Option<CoreCtl>| match (api, core) {
        (Some(api), _) => api.calls().len(),
        (None, Some(core)) => {
            let (signals, gates) = core.totals();
            usize::try_from(signals + gates).unwrap_or(usize::MAX)
        }
        (None, None) => 0,
    };
    while seen(&api, &core) < tee_calls && Instant::now() < deadline {
        std::thread::sleep(POLL);
    }
    let seen_calls = seen(&api, &core);
    let api_calls = api.map(ApiSocket::stop).unwrap_or_default();
    // The hook gets the environment of the harness. The token proves that the variables of the
    // recipe (`NEOHAR_*`) reached the hook process through the harness.
    if let Some(call) = api_calls
        .iter()
        .find(|c| c.token != recipe::PANE_TOKEN || c.harness != request.harness.name())
    {
        return Err(GoldenError::Step(format!(
            "a hook reached the API socket with the token {:?} for {:?}. It should have {:?} for {:?}",
            call.token,
            call.harness,
            recipe::PANE_TOKEN,
            request.harness.name()
        )));
    }
    let mut extras = Vec::new();
    if let Some(core) = &mut core {
        // The core takes a hook only with the token of a pane, so an interaction
        // proves that the variables of the recipe reached the hook through the harness.
        extras.push(("core/events.jsonl".to_owned(), core.events_text()));
        extras.push((
            "core/interactions.json".to_owned(),
            core.interactions_text(),
        ));
    }

    let report = server.report();
    runtime.block_on(server.stop());
    let llm_requests = fs::read_to_string(&ctx.llm_log).unwrap_or_default();
    let llm_main_requests = llm_requests
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["main"] == true)
        .count() as u64;

    let (cols, rows) = match request.scenario.mode {
        Mode::Pty => (request.scenario.cols, request.scenario.rows),
        Mode::Pipe => (0, 0),
    };
    let meta = Meta {
        harness: request.harness.name().to_owned(),
        harness_version: ctx.harness_version.clone(),
        scenario: request.scenario_name.to_owned(),
        scenario_blake3: blake3_hex(request.scenario_text.as_bytes()),
        mode: request.scenario.mode.name().to_owned(),
        os: os_description(),
        arch: std::env::consts::ARCH.to_owned(),
        cols,
        rows,
        duration_ms: u64::try_from(ctx.started.elapsed().as_millis()).unwrap_or(u64::MAX),
        exit: describe_exit(outcome.exit),
        llm_requests: report.requests,
        llm_main_requests,
        turns_used: report.turns_used,
        turns_total: report.turns_total,
        unmatched_main: report.unmatched_main,
        hook_calls: tee_calls,
        hook_calls_api: if core.is_some() {
            seen_calls
        } else {
            api_calls.len()
        },
        terminal_replies: outcome.replies,
        screens: outcome
            .screens
            .iter()
            .map(|(name, _)| name.clone())
            .collect(),
        processes: outcome
            .processes
            .iter()
            .map(|(name, _)| name.clone())
            .collect(),
    };
    // A recording that ends its core on purpose can lose a hook that comes while no core runs.
    let may_lose = request
        .scenario
        .steps
        .iter()
        .any(|s| matches!(&s.action, Action::Core(op) if op.op == "kill"));
    let consistent = if may_lose {
        meta.hook_calls_api <= meta.hook_calls
    } else {
        meta.hook_calls == meta.hook_calls_api
    };
    if !consistent {
        return Err(GoldenError::Step(format!(
            "{} hooks wrote to signals.jsonl and {} reached the API socket. The shim lost a call, \
             or the socket did",
            meta.hook_calls, meta.hook_calls_api
        )));
    }
    let transcripts = collect_transcripts(&ctx.config_dir);
    // The paths of the user stay out of the files. The hook command in a transcript holds the path
    // of `nh`, and a build of the repository is under the home directory.
    let golden = recipe::golden_binary().display().to_string();
    let redactions = [
        (request.nh.display().to_string(), "<nh>".to_owned()),
        (golden, "<golden>".to_owned()),
        (ctx.real_home.display().to_string(), "<home>".to_owned()),
    ];
    let redact = |text: &str| -> String {
        redactions
            .iter()
            .filter(|(from, _)| !from.is_empty())
            .fold(text.to_owned(), |text, (from, to)| text.replace(from, to))
    };
    Ok(Recording {
        meta,
        session: redact(&String::from_utf8_lossy(&outcome.session)).into_bytes(),
        session_file: outcome.session_file,
        signals: redact(&signals),
        llm_requests: redact(&llm_requests),
        transcripts: transcripts.iter().map(|t| redact(t)).collect(),
        screens: outcome
            .screens
            .into_iter()
            .map(|(name, text)| (name, redact(&text)))
            .collect(),
        processes: outcome
            .processes
            .into_iter()
            .map(|(name, text)| (name, redact(&text)))
            .collect(),
        extras: extras
            .into_iter()
            .map(|(path, text)| (path, redact(&text)))
            .collect(),
        scenario_text: request.scenario_text.to_owned(),
    })
}

/// The transcripts of the session: every `.jsonl` file under `projects/` (Claude Code) or
/// `sessions/` (Codex) in the configuration directory, sorted by path.
fn collect_transcripts(config_dir: &Path) -> Vec<String> {
    fn walk(dir: &Path, into: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => walk(&path, into),
                Ok(kind) if kind.is_file() && path.extension().is_some_and(|e| e == "jsonl") => {
                    into.push(path);
                }
                _ => {}
            }
        }
    }
    let mut files = Vec::new();
    for sub in ["projects", "sessions"] {
        walk(&config_dir.join(sub), &mut files);
    }
    files.sort();
    files
        .iter()
        .filter_map(|file| fs::read_to_string(file).ok())
        .collect()
}

/// Copies the scratch tree of a recording that failed, so that its files can be read: the
/// configuration of the harness, the hook tee, the request log and what the harness wrote. It runs
/// when the value is dropped without [`FailureKeeper::succeeded`]. The copy is next to the tree,
/// with `-failed` after the name, and the next failure replaces it.
struct FailureKeeper {
    root: PathBuf,
    dest: PathBuf,
    ok: bool,
}

impl FailureKeeper {
    fn new(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
            dest: PathBuf::from(format!("{}-failed", root.display())),
            ok: false,
        }
    }

    fn succeeded(&mut self) {
        self.ok = true;
    }
}

/// Copies regular files up to 8 MiB and the directories around them. Sockets and links are left.
fn copy_tree(from: &Path, to: &Path) {
    let Ok(entries) = fs::read_dir(from) else {
        return;
    };
    let _ = fs::create_dir_all(to);
    for entry in entries.filter_map(Result::ok) {
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => copy_tree(&source, &target),
            Ok(kind) if kind.is_file() && entry.metadata().is_ok_and(|m| m.len() <= 8 << 20) => {
                let _ = fs::copy(&source, &target);
            }
            _ => {}
        }
    }
}

impl Drop for FailureKeeper {
    fn drop(&mut self) {
        if self.ok {
            return;
        }
        let _ = fs::remove_dir_all(&self.dest);
        copy_tree(&self.root, &self.dest);
        eprintln!(
            "nh-golden: the files of the failed recording are in {}",
            self.dest.display()
        );
    }
}

struct Session<'a> {
    conn: Connection,
    holder: &'a mut HolderProcess,
    engine: AlacrittyEngine,
    theme: Palette,
    fx: Recorder,
    cast: CastWriter<Vec<u8>>,
    /// The start of the recording: the zero of the cast.
    begin: Instant,
    last_output: Instant,
    exit: Option<ExitStatus>,
    next_input_id: u64,
    replies: usize,
    /// The rows of the screen, until the next output.
    screen_cache: Option<Vec<String>>,
    snapshots: Vec<(String, String)>,
    tee_file: PathBuf,
    /// How many signals earlier `wait_signal` and `expect_signal` steps took.
    signal_cursor: usize,
    proc_roots: Vec<String>,
    proc_replacements: Vec<(String, String)>,
    processes: Vec<(String, String)>,
    /// The core of a delivery scenario. A core step takes it out for the time of the step.
    core: Option<CoreCtl>,
}

impl Session<'_> {
    fn since_begin(&self) -> Duration {
        self.begin.elapsed()
    }

    /// The rows of the screen, without the blanks at the end of a row.
    fn rows(&mut self) -> &[String] {
        if self.screen_cache.is_none() {
            let dump = ScreenDump::of(&self.engine, &self.theme);
            self.screen_cache = Some(dump.text());
        }
        self.screen_cache.as_deref().unwrap_or_default()
    }

    /// The screen as text: rows joined with newlines, without the empty rows at the end.
    fn screen_text(&mut self) -> String {
        let mut rows = self.rows().to_vec();
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        rows.join("\n")
    }

    fn send(&mut self, bytes: &[u8], from_step: bool) -> Result<(), GoldenError> {
        let input_id = self.next_input_id;
        self.next_input_id += 1;
        if from_step {
            self.cast
                .input(self.since_begin(), bytes)
                .map_err(GoldenError::io("write the cast"))?;
        }
        self.conn.send(&HolderRequest::Input {
            input_id,
            bytes: bytes.to_vec(),
        })?;
        Ok(())
    }

    /// Reads at most one event from the holder, waiting at most `within`.
    fn pump(&mut self, within: Duration) -> Result<(), GoldenError> {
        let Some(event) = self.conn.recv(within)? else {
            return Ok(());
        };
        match event {
            HolderEvent::Output {
                stream: StreamId::Pty,
                bytes,
                ..
            } => {
                let at = self.since_begin();
                self.last_output = Instant::now();
                self.screen_cache = None;
                self.cast
                    .output(at, &bytes)
                    .map_err(GoldenError::io("write the cast"))?;
                self.engine.advance(&bytes, &mut self.fx);
                // The engine answers the queries of the program, as the core does.
                for effect in std::mem::take(&mut self.fx.0) {
                    if let Effect::PtyWrite(reply) = effect {
                        self.replies += 1;
                        self.send(&reply, false)?;
                    }
                }
            }
            HolderEvent::Marker {
                marker: Marker::Resize { cols, rows },
                ..
            } => {
                self.screen_cache = None;
                self.engine.resize(cols, rows);
                self.cast
                    .resize(self.since_begin(), cols, rows)
                    .map_err(GoldenError::io("write the cast"))?;
            }
            HolderEvent::ChildExited { status } => {
                self.exit = Some(status);
                self.holder.child_exited = true;
            }
            HolderEvent::Truncated { requested, start } => {
                return Err(GoldenError::Step(format!(
                    "the holder's ring dropped output that the recorder had not read (asked for \
                     {requested}, the ring starts at {start})"
                )));
            }
            HolderEvent::LeaseLost { by_fencing_token } => {
                return Err(GoldenError::Step(format!(
                    "another client took the holder (fencing token {by_fencing_token})"
                )));
            }
            HolderEvent::Refused { reason } => {
                return Err(GoldenError::Step(format!("the holder refused: {reason:?}")));
            }
            _ => {}
        }
        Ok(())
    }

    /// Reads until `for_` has passed.
    fn pump_for(&mut self, for_: Duration) -> Result<(), GoldenError> {
        let deadline = Instant::now() + for_;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            self.pump(left.min(POLL))?;
        }
        Ok(())
    }

    /// Reads until the output has been quiet for [`QUIET`], for at most `max`.
    fn quiet(&mut self, max: Duration) -> bool {
        let deadline = Instant::now() + max;
        loop {
            if self.exit.is_some() || self.last_output.elapsed() >= QUIET {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            if self.pump(POLL).is_err() {
                return false;
            }
        }
    }

    fn settle(&mut self) -> Result<(), GoldenError> {
        // Output that is still coming after the last step belongs in the recording.
        self.pump_for(Duration::from_millis(50))?;
        self.quiet(SETTLE_MAX);
        Ok(())
    }

    /// An error for a step, with what the screen shows.
    fn failure(&mut self, number: usize, action: &str, why: &str) -> GoldenError {
        let screen = self.screen_text();
        GoldenError::Step(format!(
            "step {number} ({action}) failed: {why}.\nThe screen now:\n{screen}"
        ))
    }

    fn signals(&self) -> Vec<serde_json::Value> {
        fs::read_to_string(&self.tee_file)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    /// The position of the first signal of `event` after the cursor.
    fn find_signal(&self, event: &str) -> Option<usize> {
        let signals = self.signals();
        signals
            .iter()
            .enumerate()
            .skip(self.signal_cursor)
            .find(|(_, s)| s["event"] == event)
            .map(|(at, _)| at)
    }

    fn run_step(&mut self, number: usize, step: &Step) -> Result<(), GoldenError> {
        match &step.action {
            Action::Send(text) => {
                self.require_running(number, "send")?;
                self.send(text.as_bytes(), true)
            }
            Action::Key(name) => {
                self.require_running(number, "key")?;
                let bytes = key_bytes(name).unwrap_or_default();
                self.send(&bytes, true)
            }
            Action::Sleep(time) => self.pump_for(*time),
            Action::WaitScreen(pattern) => {
                let regex = Regex::new(pattern).map_err(|e| GoldenError::Step(e.to_string()))?;
                let deadline = Instant::now() + step.timeout;
                loop {
                    if self.rows().iter().any(|row| regex.is_match(row)) {
                        return Ok(());
                    }
                    if self.exit.is_some() {
                        return Err(self.failure(
                            number,
                            &format!("wait_screen {pattern:?}"),
                            "the harness exited first",
                        ));
                    }
                    if Instant::now() >= deadline {
                        return Err(self.failure(
                            number,
                            &format!("wait_screen {pattern:?}"),
                            &format!("no row matched within {:?}", step.timeout),
                        ));
                    }
                    self.pump(POLL)?;
                }
            }
            Action::ExpectScreen(pattern) => {
                // Read what has arrived, then look once.
                self.pump_for(Duration::from_millis(50))?;
                let regex = Regex::new(pattern).map_err(|e| GoldenError::Step(e.to_string()))?;
                if self.rows().iter().any(|row| regex.is_match(row)) {
                    Ok(())
                } else {
                    Err(self.failure(
                        number,
                        &format!("expect_screen {pattern:?}"),
                        "no row matches",
                    ))
                }
            }
            Action::WaitSignal(event) => {
                let deadline = Instant::now() + step.timeout;
                loop {
                    if let Some(at) = self.find_signal(event) {
                        self.signal_cursor = at + 1;
                        return Ok(());
                    }
                    if Instant::now() >= deadline {
                        return Err(self.failure(
                            number,
                            &format!("wait_signal {event}"),
                            &format!("no such hook within {:?}", step.timeout),
                        ));
                    }
                    self.pump(POLL)?;
                }
            }
            Action::ExpectSignal(event) => {
                self.pump_for(Duration::from_millis(50))?;
                if let Some(at) = self.find_signal(event) {
                    self.signal_cursor = at + 1;
                    Ok(())
                } else {
                    Err(self.failure(
                        number,
                        &format!("expect_signal {event}"),
                        "no such hook yet",
                    ))
                }
            }
            Action::ExpectExit => {
                let deadline = Instant::now() + step.timeout;
                while self.exit.is_none() {
                    if Instant::now() >= deadline {
                        return Err(self.failure(
                            number,
                            "expect_exit",
                            &format!("the harness is still running after {:?}", step.timeout),
                        ));
                    }
                    self.pump(POLL)?;
                }
                // Output that came with the exit is read.
                self.pump_for(Duration::from_millis(100))
            }
            other @ (Action::SendJson(_)
            | Action::WaitLine(_)
            | Action::Respond(_)
            | Action::CloseStdin
            | Action::StartPeer(_)
            | Action::AnswerRequest { .. }) => Err(GoldenError::Step(format!(
                "step {number}: the action {other:?} needs mode = \"pipe\""
            ))),
            Action::Core(op) => self.run_core(number, op, step.timeout),
            Action::Processes(name) => {
                self.pump_for(Duration::from_millis(100))?;
                let root = self.holder.child_pid().unwrap_or(0);
                let text = procs::tree_text(
                    root,
                    &self.proc_roots,
                    &self.proc_replacements,
                    &procs::list(),
                );
                self.processes.push((name.clone(), text));
                Ok(())
            }
            Action::Capture(name) => {
                self.pump_for(Duration::from_millis(100))?;
                let text = self.screen_text();
                self.cast
                    .marker(self.since_begin(), name)
                    .map_err(GoldenError::io("write the cast"))?;
                self.snapshots.push((name.clone(), text));
                Ok(())
            }
            Action::Snapshot(name) => {
                if !self.quiet(step.timeout) {
                    return Err(self.failure(
                        number,
                        &format!("snapshot {name}"),
                        &format!("the output did not stop within {:?}", step.timeout),
                    ));
                }
                let text = self.screen_text();
                self.cast
                    .marker(self.since_begin(), name)
                    .map_err(GoldenError::io("write the cast"))?;
                self.snapshots.push((name.clone(), text));
                Ok(())
            }
        }
    }

    /// One step on the core. The core is taken out of the session for the step, because the step
    /// reads the output of the harness while it waits.
    fn run_core(
        &mut self,
        number: usize,
        op: &CoreOp,
        timeout: Duration,
    ) -> Result<(), GoldenError> {
        let mut core = self.core.take().ok_or_else(|| {
            GoldenError::Step(format!("step {number}: the recording has no core"))
        })?;
        let result = self.run_core_with(&mut core, number, op, timeout);
        self.core = Some(core);
        result
    }

    fn run_core_with(
        &mut self,
        core: &mut CoreCtl,
        number: usize,
        op: &CoreOp,
        timeout: Duration,
    ) -> Result<(), GoldenError> {
        let describe =
            |core: &CoreCtl| serde_json::to_string_pretty(&core.interactions()).unwrap_or_default();
        match op.op.as_str() {
            "wait" => {
                let deadline = Instant::now() + timeout;
                loop {
                    if let Some(found) = core.find(op) {
                        core.note(
                            "wait",
                            serde_json::json!({"state": found["state"], "mode": found["mode"]}),
                        );
                        return Ok(());
                    }
                    if Instant::now() >= deadline {
                        return Err(self.failure(
                            number,
                            "core wait",
                            &format!("no interaction fits {op:?} within {timeout:?}. The interactions:\n{}", describe(core)),
                        ));
                    }
                    self.pump(POLL)?;
                }
            }
            "expect" => {
                self.pump_for(Duration::from_millis(100))?;
                match core.find(op) {
                    Some(found) => {
                        core.note(
                            "expect",
                            serde_json::json!({"state": found["state"], "mode": found["mode"]}),
                        );
                        Ok(())
                    }
                    None => Err(self.failure(
                        number,
                        "core expect",
                        &format!(
                            "no interaction fits {op:?}. The interactions:\n{}",
                            describe(core)
                        ),
                    )),
                }
            }
            "answer" => core.answer(op).map(|_| ()),
            "race" => {
                let (a, b) = core.race(op)?;
                if code_is_already_answered(&a, &b) {
                    Ok(())
                } else {
                    Err(self.failure(
                        number,
                        "core race",
                        "one answer must win and the other must get already_answered",
                    ))
                }
            }
            "deliver" => core.deliver(op),
            "release" => core.release(op),
            "focus" => core.focus(op),
            "kill" => core.kill(),
            "start" => core.restart(),
            "kill_shim" => core.kill_shim(op),
            other => Err(GoldenError::Step(format!(
                "step {number}: the core op {other:?} is not known"
            ))),
        }
    }

    fn require_running(&mut self, number: usize, action: &str) -> Result<(), GoldenError> {
        if let Some(status) = self.exit {
            return Err(self.failure(
                number,
                action,
                &format!("the harness already exited ({status:?})"),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_recording_waits_for_the_root_and_then_fails_or_goes_on() {
        let root = std::env::temp_dir().join(format!("nhg-lock-{}", std::process::id()));
        let first = RootLock::acquire_within(&root, Duration::from_secs(1)).expect("the first");
        let error = RootLock::acquire_within(&root, Duration::from_millis(300))
            .err()
            .expect("the root is taken");
        assert!(
            error.to_string().contains("another recording uses"),
            "{error}"
        );
        drop(first);
        RootLock::acquire_within(&root, Duration::from_secs(1)).expect("the root is free again");
        let _ = fs::remove_file(format!("{}.lock", root.display()));
    }
}
