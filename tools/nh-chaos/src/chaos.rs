//! The chaos run (`nh-chaos run`).
//!
//! The run starts a host (the core without a window) with 20 live panes, loads them with random
//! input for 0.5 to 5 s, kills the host with `SIGKILL`, and starts it again. It does this many
//! times. After each restart it waits until every pane is restored and the resize nudge is over,
//! and takes the screen of every pane with its journal offset (`debug.screen`).
//!
//! The check comes after the next kill. The host is dead then, so the run can read the journal of
//! each holder from the offset where it stopped last time, and feed it to a reference engine that
//! never restarted. The screen of the restored pane at its journal offset must equal the screen of
//! the reference at the same offset. A cell that differs is a failure.
//!
//! Inputs get the same treatment. Two panes run `echo-seq`, which logs every byte it reads. The
//! run sends them inputs with ids, and learns the acks. After a kill, an input without a known ack
//! is in doubt: the run sends most of them again with the same id, and gives up the others (the
//! client says `server_restarted`). At the end every acked or resent input is in the log exactly
//! once, and no input is in the log twice.

use std::{
    collections::{BTreeMap, HashMap},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use nh_store::{PaneLiveness, PaneRecord, Store};
use nh_term::ScreenDump;
use rustix::process::{Pid, Signal, kill_process};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::process::{Child, Command};
use ulid::Ulid;

use crate::{
    journal::{self, Reference},
    panes::{self, Kind, PaneDef, Tools},
    rng::Rng,
    ws::{Control, HostInfo},
};

/// The session name of the host.
pub const SESSION: &str = "chaos";

/// The window that resizes the panes. No window with this id is open, so no frames are built. The
/// first window that sends `pane.resize` sets the size of the pane, open or not.
const WINDOW: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

/// The time that applications get to redraw after the nudge, before the run takes the screens.
/// The nudge ends when the engine has applied its second resize; the redraw of the application
/// follows. This is a wait for other processes, which show no event.
const REDRAW: Duration = Duration::from_millis(500);

/// What the user asks of a run.
pub struct Options {
    pub cycles: u32,
    pub seed: u64,
    pub dir: PathBuf,
    pub nh: PathBuf,
    pub fixture: Option<PathBuf>,
    pub ring_bytes: u64,
    pub checkpoint_idle_ms: u64,
    pub checkpoint_interval_ms: u64,
    pub load_ms: (u64, u64),
    pub keep: bool,
}

/// The state of an input to an `echo-seq` pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Fate {
    /// Sent, and no ack seen.
    Sent,
    /// The holder acknowledged it to a host that the run saw.
    Acked,
    /// Sent again after a restart, and no ack seen yet.
    Resent,
    /// Sent again after a restart, and acknowledged.
    ResentAcked,
    /// The client gave it up after a restart (`input_unconfirmed`, reason `server_restarted`).
    Abandoned,
}

struct Input {
    pane: usize,
    payload: String,
    fate: Fate,
}

/// A pane of the run.
struct PaneState {
    id: Ulid,
    def: PaneDef,
    reference: Reference,
    /// The journal offset up to which the reference has read the holder.
    have_end: u64,
    /// The cursor for `debug.pane_inputs`. A new host starts at 0.
    ack_cursor: u64,
    /// The resizes that the holder recorded, as `(offset, cols, rows)`. Mismatch files show them.
    resizes: Vec<(u64, u16, u16)>,
}

/// A screen that the run took after a restart.
struct Capture {
    offset: u64,
    dump: ScreenDump,
    /// The screen at the end of the replay, before the resize nudge, with its journal offset.
    /// After the nudge, applications repaint and hide a fault of the replay, so the run checks
    /// this screen as well.
    restored: Option<(u64, ScreenDump)>,
}

/// What one cycle found.
#[derive(Debug, Default, Serialize)]
pub struct CycleResult {
    pub cycle: u32,
    /// From the start of the host process until every pane was restored.
    pub restore_wall_ms: Option<u64>,
    /// The same, as the core measures it from the start of its runtime.
    pub restore_core_ms: Option<u64>,
    /// The time until the resize nudge ended in every pane.
    pub settle_ms: Option<u64>,
    pub recoveries: BTreeMap<String, u32>,
    pub replay_bytes: u64,
    pub killed_during_restore: bool,
    pub load_ms: u64,
    pub resized_panes: u32,
    pub captured_panes: u32,
    /// How many screens the check compared: up to two for each pane.
    pub screens_checked: u32,
    pub panes_with_differences: Vec<PaneDifference>,
    pub inputs_resent: u32,
    pub inputs_abandoned: u32,
}

/// A pane whose restored screen differs from the reference.
#[derive(Debug, Serialize)]
pub struct PaneDifference {
    pub pane: usize,
    pub kind: &'static str,
    /// `replay_end` is the screen before the nudge, and `settled` the screen after it.
    pub stage: &'static str,
    /// The differences of this pane stay out of the verdict.
    pub informational: bool,
    pub recovery: String,
    pub cells: usize,
    /// How many of the differences are blank cells that differ in style only.
    pub blank_style_only: usize,
    pub other: usize,
    pub samples: Vec<String>,
    pub file: String,
}

/// The result of the whole run.
#[derive(Debug, Default, Serialize)]
pub struct Summary {
    pub seed: u64,
    pub cycles: u32,
    pub restores: u32,
    pub kills_during_restore: u32,
    pub panes: usize,
    pub captured_screens: u32,
    pub screens_checked: u32,
    pub screens_with_differences: u32,
    pub cell_differences: usize,
    pub blank_style_only: usize,
    /// How many of those cells are in the screens at the end of the replay, before the nudge.
    pub cell_differences_before_nudge: usize,
    /// The same numbers for the pane that does not redraw. They stay out of the verdict.
    pub informational_screens_with_differences: u32,
    pub informational_cell_differences: usize,
    pub restore_wall_ms: Stats,
    pub restore_core_ms: Stats,
    pub settle_ms: Stats,
    pub recoveries: BTreeMap<String, u32>,
    pub replay_bytes_total: u64,
    pub inputs_sent: usize,
    pub inputs_acked_before_kill: usize,
    pub inputs_resent: usize,
    pub inputs_abandoned: usize,
    pub inputs_missing: Vec<u64>,
    pub inputs_applied_twice: Vec<u64>,
    pub inputs_unknown_bytes: usize,
    pub results: Vec<CycleResult>,
}

impl Summary {
    /// Whether the gate numbers hold: no cell differs, no input is lost or applied twice.
    pub fn passed(&self) -> bool {
        self.cell_differences == 0
            && self.inputs_missing.is_empty()
            && self.inputs_applied_twice.is_empty()
            && self.inputs_unknown_bytes == 0
    }
}

/// Min, median, 95th percentile and max of a list of milliseconds.
#[derive(Debug, Default, Serialize)]
pub struct Stats {
    pub n: usize,
    pub min: u64,
    pub median: u64,
    pub p95: u64,
    pub max: u64,
}

impl Stats {
    pub fn of(mut values: Vec<u64>) -> Self {
        if values.is_empty() {
            return Self::default();
        }
        values.sort_unstable();
        // The nearest rank.
        let rank = |p: usize| values[((values.len() * p).div_ceil(100)).clamp(1, values.len()) - 1];
        Self {
            n: values.len(),
            min: values[0],
            median: rank(50),
            p95: rank(95),
            max: values[values.len() - 1],
        }
    }
}

/// A running host.
struct Host {
    child: Child,
    pid: i32,
    control: Control,
    spawned_at: Instant,
}

impl Host {
    async fn kill(mut self) -> Result<()> {
        if let Some(pid) = Pid::from_raw(self.pid) {
            let _ = kill_process(pid, Signal::KILL);
        }
        self.child.wait().await?;
        Ok(())
    }
}

struct World {
    dir: PathBuf,
    nh: PathBuf,
    ring_bytes: u64,
    checkpoint: (u64, u64),
    panes: Vec<PaneState>,
    inputs: BTreeMap<u64, Input>,
    next_input_id: u64,
    hosts_started: u32,
}

/// Runs the chaos test and returns what it found.
pub async fn run(options: Options) -> Result<Summary> {
    let dir = options.dir.clone();
    let _ = std::fs::remove_dir_all(&dir);
    for sub in ["config", "data", "run", "home", "files", "mismatch"] {
        std::fs::create_dir_all(dir.join(sub))?;
    }
    // The core refuses a runtime root that is not private.
    std::fs::set_permissions(dir.join("run"), std::fs::Permissions::from_mode(0o700))?;
    let tools = Tools::find(options.fixture.clone())?;
    panes::write_files(&dir.join("files"))?;
    let mut rng = Rng::new(options.seed);
    let defs = panes::plan(&tools, &dir.join("files"), &mut rng);
    let mut world = World {
        dir: dir.clone(),
        nh: options.nh.clone(),
        ring_bytes: options.ring_bytes,
        checkpoint: (options.checkpoint_idle_ms, options.checkpoint_interval_ms),
        panes: Vec::new(),
        inputs: BTreeMap::new(),
        next_input_id: 1,
        hosts_started: 0,
    };
    let outcome = world.run_cycles(&options, defs, &mut rng).await;
    if !options.keep {
        world.clean();
    }
    outcome
}

impl World {
    async fn run_cycles(
        &mut self,
        options: &Options,
        defs: Vec<PaneDef>,
        rng: &mut Rng,
    ) -> Result<Summary> {
        let mut results = Vec::new();
        // One more cycle than asked: it has no load and no kill of its own, but it restores, takes
        // the screens, sends the in-doubt inputs again and is checked after the last kill.
        for cycle in 0..=options.cycles {
            let closing = cycle == options.cycles;
            let result = self
                .cycle(cycle, closing, &defs, options.load_ms, rng)
                .await
                .with_context(|| format!("cycle {cycle}"))?;
            eprintln!("{}", describe(&result));
            results.push(result);
        }
        self.summarize(options, results)
    }

    fn env(&self) -> Vec<(&'static str, String)> {
        let path = |name: &str| self.dir.join(name).display().to_string();
        vec![
            ("NEOHAR_CONFIG_DIR", path("config")),
            ("NEOHAR_DATA_DIR", path("data")),
            ("NEOHAR_RUNTIME_DIR", path("run")),
            ("HOME", path("home")),
            ("SHELL", "/bin/sh".to_owned()),
            ("LANG", "en_US.UTF-8".to_owned()),
            (
                "PATH",
                std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_owned()),
            ),
        ]
    }

    async fn start_host(&mut self) -> Result<Host> {
        self.hosts_started += 1;
        let info_file = self.dir.join("host.json");
        let _ = std::fs::remove_file(&info_file);
        let log = std::fs::File::create(self.dir.join(format!("host-{}.log", self.hosts_started)))?;
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg("host")
            .arg("--info")
            .arg(&info_file)
            .arg("--nh")
            .arg(&self.nh)
            .arg("--session")
            .arg(SESSION)
            .arg("--ring-bytes")
            .arg(self.ring_bytes.to_string())
            .arg("--checkpoint-idle-ms")
            .arg(self.checkpoint.0.to_string())
            .arg("--checkpoint-interval-ms")
            .arg(self.checkpoint.1.to_string())
            .env_clear()
            .envs(self.env())
            .stdin(std::process::Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        let spawned_at = Instant::now();
        let mut child = command.spawn().context("cannot start the host")?;
        let pid = i32::try_from(child.id().context("the host has no pid")?)?;
        let deadline = Instant::now() + Duration::from_secs(20);
        let info = loop {
            if let Ok(text) = std::fs::read_to_string(&info_file)
                && let Ok(info) = serde_json::from_str::<HostInfo>(&text)
            {
                break info;
            }
            if let Some(status) = child.try_wait()? {
                bail!("the host ended before it was ready: {status}");
            }
            if Instant::now() > deadline {
                bail!("the host was not ready in 20 s");
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        };
        let control = Control::connect(&info).await?;
        Ok(Host {
            child,
            pid,
            control,
            spawned_at,
        })
    }

    /// The records of the panes in the database of the host.
    fn records(&self) -> Result<HashMap<Ulid, PaneRecord>> {
        let path = self.dir.join("data").join(SESSION).join("state.db");
        let store = Store::open(&path).context("cannot open state.db")?;
        let mut all = HashMap::new();
        for liveness in [
            PaneLiveness::Running,
            PaneLiveness::Exited,
            PaneLiveness::Lost,
        ] {
            for pane in store.panes(liveness)? {
                all.insert(pane.id, pane);
            }
        }
        Ok(all)
    }

    async fn spawn_panes(&mut self, host: &mut Host, defs: &[PaneDef]) -> Result<()> {
        for def in defs {
            let result = host
                .control
                .call(
                    "debug.spawn_panes",
                    json!({
                        "count": 1, "cols": def.cols, "rows": def.rows,
                        "argv": def.argv, "isolated_home": true,
                    }),
                )
                .await?;
            let id: Ulid = result["panes"][0]["pane"]
                .as_str()
                .context("no pane id in the answer")?
                .parse()
                .map_err(|e| anyhow!("the pane id is no ULID: {e}"))?;
            self.panes.push(PaneState {
                id,
                def: def.clone(),
                reference: Reference::new(def.cols, def.rows),
                have_end: 0,
                ack_cursor: 0,
                resizes: Vec::new(),
            });
        }
        Ok(())
    }

    /// Waits until the core says that every pane is back. Returns the report.
    async fn wait_restored(&self, host: &mut Host) -> Result<Value> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let report = host.control.call("debug.restore_report", json!({})).await?;
            if report["done"] == true {
                return Ok(report);
            }
            if Instant::now() > deadline {
                bail!("the panes were not restored in 30 s: {report}");
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    /// Waits until every pane has read its journal and ended its nudge.
    async fn wait_settled(&self, host: &mut Host) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let mut waiting = 0;
            for pane in &self.panes {
                let screen = host
                    .control
                    .call(
                        "debug.screen",
                        json!({ "pane": pane.id.to_string(), "dump": false }),
                    )
                    .await?;
                if screen["restoring"] == true || screen["settled"] != true {
                    waiting += 1;
                }
            }
            if waiting == 0 {
                return Ok(());
            }
            if Instant::now() > deadline {
                bail!("{waiting} panes did not settle in 30 s");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn capture(&self, host: &mut Host) -> Result<Vec<Capture>> {
        let mut out = Vec::new();
        for pane in &self.panes {
            let screen = host
                .control
                .call("debug.screen", json!({ "pane": pane.id.to_string() }))
                .await?;
            let offset = screen["fed_offset"].as_u64().context("no offset")?;
            let dump: ScreenDump = serde_json::from_value(screen["dump"].clone())
                .context("the dump does not parse")?;
            let restored = match screen.get("restored") {
                Some(restored) => Some((
                    restored["fed_offset"].as_u64().context("no offset")?,
                    serde_json::from_value(restored["dump"].clone())
                        .context("the restored dump does not parse")?,
                )),
                None => None,
            };
            out.push(Capture {
                offset,
                dump,
                restored,
            });
        }
        Ok(out)
    }

    /// Sends an input to a pane. The id is the client's.
    async fn send(&self, host: &mut Host, pane: usize, id: u64, event: Value) -> Result<()> {
        host.control
            .call(
                "pane.input",
                json!({ "pane": self.panes[pane].id.to_string(), "input_id": id, "event": event }),
            )
            .await
            .map(|_| ())
    }

    fn echo_panes(&self) -> Vec<usize> {
        self.panes
            .iter()
            .enumerate()
            .filter(|(_, p)| p.def.kind == Kind::EchoSeq)
            .map(|(i, _)| i)
            .collect()
    }

    /// Learns the acks of the echo panes from the host.
    async fn poll_acks(&mut self, host: &mut Host) -> Result<()> {
        for pane in self.echo_panes() {
            let answer = host
                .control
                .call(
                    "debug.pane_inputs",
                    json!({ "pane": self.panes[pane].id.to_string(), "since": self.panes[pane].ack_cursor }),
                )
                .await?;
            self.panes[pane].ack_cursor = answer["next"].as_u64().unwrap_or(0);
            for id in answer["acked"].as_array().into_iter().flatten() {
                let Some(id) = id.as_u64() else { continue };
                if let Some(input) = self.inputs.get_mut(&id) {
                    input.fate = match input.fate {
                        Fate::Sent => Fate::Acked,
                        Fate::Resent => Fate::ResentAcked,
                        other => other,
                    };
                }
            }
        }
        Ok(())
    }

    /// The inputs that no host acknowledged to the run, in the order of their ids.
    fn in_doubt(&self) -> Vec<u64> {
        self.inputs
            .iter()
            .filter(|(_, i)| i.fate == Fate::Sent)
            .map(|(id, _)| *id)
            .collect()
    }

    /// After a restart: sends most inputs in doubt again with their ids, and gives up the others.
    async fn settle_inputs(
        &mut self,
        host: &mut Host,
        rng: &mut Rng,
        result: &mut CycleResult,
    ) -> Result<()> {
        for id in self.in_doubt() {
            if rng.chance(70) {
                let (pane, payload) = {
                    let input = &self.inputs[&id];
                    (input.pane, input.payload.clone())
                };
                self.send(host, pane, id, panes::text(&payload)).await?;
                self.inputs.get_mut(&id).context("the input is known")?.fate = Fate::Resent;
                result.inputs_resent += 1;
            } else {
                self.inputs.get_mut(&id).context("the input is known")?.fate = Fate::Abandoned;
                result.inputs_abandoned += 1;
            }
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            self.poll_acks(host).await?;
            if !self.inputs.values().any(|i| i.fate == Fate::Resent) {
                return Ok(());
            }
            if Instant::now() > deadline {
                let waiting: Vec<_> = self
                    .inputs
                    .iter()
                    .filter(|(_, i)| i.fate == Fate::Resent)
                    .map(|(id, _)| *id)
                    .collect();
                bail!("no ack for the inputs sent again: {waiting:?}");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Random input and, in some cycles, random resizes, until `duration` has passed.
    async fn load(
        &mut self,
        host: &mut Host,
        rng: &mut Rng,
        duration: Duration,
        resizes: bool,
        result: &mut CycleResult,
    ) -> Result<()> {
        let end = Instant::now() + duration;
        let mut next_poll = Instant::now();
        let echo = self.echo_panes();
        // The scroller takes no input: nothing in it reads the terminal.
        let targets: Vec<usize> = (0..self.panes.len())
            .filter(|&i| !self.panes[i].def.kind.informational())
            .collect();
        while Instant::now() < end {
            let pane = *rng.pick(&targets);
            let id = self.next_input_id;
            self.next_input_id += 1;
            if self.panes[pane].def.kind == Kind::EchoSeq {
                let payload = panes::payload(id);
                self.inputs.insert(
                    id,
                    Input {
                        pane,
                        payload: payload.clone(),
                        fate: Fate::Sent,
                    },
                );
                self.send(host, pane, id, panes::text(&payload)).await?;
            } else {
                let event = panes::random_input(self.panes[pane].def.kind, rng);
                self.send(host, pane, id, event).await?;
            }
            // The echo panes get an extra input now and then, so that they have many in flight.
            if !echo.is_empty() && rng.chance(40) {
                let pane = *rng.pick(&echo);
                let id = self.next_input_id;
                self.next_input_id += 1;
                let payload = panes::payload(id);
                self.inputs.insert(
                    id,
                    Input {
                        pane,
                        payload: payload.clone(),
                        fate: Fate::Sent,
                    },
                );
                self.send(host, pane, id, panes::text(&payload)).await?;
            }
            if resizes && rng.chance(2) {
                let pane = usize::try_from(rng.below(self.panes.len() as u64))?;
                let (cols, rows) = (rng.range(60, 140), rng.range(18, 45));
                host.control
                    .call(
                        "pane.resize",
                        json!({
                            "pane": self.panes[pane].id.to_string(), "cols": cols, "rows": rows,
                            "window": WINDOW,
                        }),
                    )
                    .await?;
                result.resized_panes += 1;
            }
            if Instant::now() >= next_poll {
                self.poll_acks(host).await?;
                next_poll = Instant::now() + Duration::from_millis(50);
            }
            tokio::time::sleep(Duration::from_millis(rng.range(2, 25))).await;
        }
        Ok(())
    }

    /// Sends a burst of inputs to the echo panes, one after the other with no pause and no look at
    /// the acks, so that the kill that follows finds inputs in every state: written and
    /// acknowledged, written and not acknowledged to the run, and not written.
    async fn burst(&mut self, host: &mut Host, rng: &mut Rng) -> Result<()> {
        let echo = self.echo_panes();
        for _ in 0..rng.range(5, 60) {
            let pane = *rng.pick(&echo);
            let id = self.next_input_id;
            self.next_input_id += 1;
            let payload = panes::payload(id);
            self.inputs.insert(
                id,
                Input {
                    pane,
                    payload: payload.clone(),
                    fate: Fate::Sent,
                },
            );
            self.send(host, pane, id, panes::text(&payload)).await?;
        }
        Ok(())
    }

    /// Reads the journals of all panes from where the run stopped, while the host is dead.
    async fn read_journals(&mut self) -> Result<()> {
        let records = self.records()?;
        for pane in &mut self.panes {
            let record = records
                .get(&pane.id)
                .with_context(|| format!("pane {} is not in the database", pane.id))?;
            let (items, end) = journal::read_from(record, pane.have_end).await?;
            for item in &items {
                if let journal::Item::Resize { offset, cols, rows } = item {
                    pane.resizes.push((*offset, *cols, *rows));
                }
            }
            pane.reference.extend(items);
            pane.have_end = end;
        }
        Ok(())
    }

    /// Compares the screens that the run took with the reference. The host is dead.
    fn verify(
        &mut self,
        cycle: u32,
        captures: &[Capture],
        recoveries: &[String],
        result: &mut CycleResult,
    ) -> Result<()> {
        for (index, (pane, capture)) in self.panes.iter_mut().zip(captures).enumerate() {
            // The screen at the end of the replay comes first: it has the lower offset.
            let stages = capture
                .restored
                .iter()
                .map(|(offset, dump)| ("replay_end", *offset, dump))
                .chain([("settled", capture.offset, &capture.dump)]);
            for (stage, offset, dump) in stages {
                let expected = pane
                    .reference
                    .dump_at(offset, (dump.cols, dump.rows))
                    .with_context(|| format!("pane {index} ({})", pane.def.kind.name()))?;
                let diff = dump.differences(&expected, 6);
                result.screens_checked += 1;
                if diff.is_empty() {
                    continue;
                }
                let (blank, other) = classify(dump, &expected);
                let file = self.dir.join("mismatch").join(format!(
                    "cycle-{cycle}-pane-{index}-{}-{stage}.json",
                    pane.def.kind.name()
                ));
                std::fs::write(
                    &file,
                    serde_json::to_vec_pretty(&json!({
                        "cycle": cycle, "pane": pane.id.to_string(), "stage": stage,
                        "offset": offset, "recovery": recoveries.get(index), "diff": diff,
                        "kind": pane.def.kind.name(), "resizes": pane.resizes,
                        "restored": dump, "reference": expected,
                    }))?,
                )?;
                result.panes_with_differences.push(PaneDifference {
                    pane: index,
                    kind: pane.def.kind.name(),
                    stage,
                    informational: pane.def.kind.informational(),
                    recovery: recoveries.get(index).cloned().unwrap_or_default(),
                    cells: diff.count,
                    blank_style_only: blank,
                    other,
                    samples: diff.samples,
                    file: file.display().to_string(),
                });
            }
        }
        Ok(())
    }

    /// One cycle: start the host, restore, take the screens, load, kill, read the journals and
    /// check the screens.
    async fn cycle(
        &mut self,
        cycle: u32,
        closing: bool,
        defs: &[PaneDef],
        load_ms: (u64, u64),
        rng: &mut Rng,
    ) -> Result<CycleResult> {
        let mut result = CycleResult {
            cycle,
            ..CycleResult::default()
        };
        let mut host = self.start_host().await?;
        // The acks of a new core are numbered from 0.
        for pane in &mut self.panes {
            pane.ack_cursor = 0;
        }
        let mut captures = Vec::new();
        let mut recoveries = Vec::new();

        if cycle == 0 {
            self.spawn_panes(&mut host, defs).await?;
            self.wait_settled(&mut host).await?;
        } else if rng.chance(10) && !closing {
            // A kill in the middle of the restart. There is no screen to check this time. The
            // next restart must still come back exact.
            tokio::time::sleep(Duration::from_millis(rng.range(0, 250))).await;
            result.killed_during_restore = true;
            host.kill().await?;
            self.read_journals().await?;
            return Ok(result);
        } else {
            let report = self.wait_restored(&mut host).await?;
            result.restore_wall_ms = Some(millis(host.spawned_at.elapsed()));
            result.restore_core_ms = report["finished_ms"].as_u64();
            recoveries = self.read_report(&report, &mut result)?;
            self.wait_settled(&mut host).await?;
            result.settle_ms = Some(millis(host.spawned_at.elapsed()));
        }
        tokio::time::sleep(REDRAW).await;
        captures.extend(self.capture(&mut host).await?);
        result.captured_panes = u32::try_from(captures.len())?;

        self.settle_inputs(&mut host, rng, &mut result).await?;
        if !closing {
            let duration = Duration::from_millis(rng.range(load_ms.0, load_ms.1));
            result.load_ms = millis(duration);
            let resizes = rng.chance(25);
            self.load(&mut host, rng, duration, resizes, &mut result)
                .await?;
            self.burst(&mut host, rng).await?;
        }
        host.kill().await?;
        self.read_journals().await?;
        self.verify(cycle, &captures, &recoveries, &mut result)?;
        Ok(result)
    }

    /// Checks the restore report of the core and returns the recovery method of each pane.
    fn read_report(&self, report: &Value, result: &mut CycleResult) -> Result<Vec<String>> {
        let entries = report["panes"]
            .as_array()
            .context("no panes in the report")?;
        if entries.len() != self.panes.len() {
            bail!(
                "{} panes were running and the report has {}",
                self.panes.len(),
                entries.len()
            );
        }
        let mut by_pane: HashMap<String, &Value> = HashMap::new();
        for entry in entries {
            let pane = entry["pane"].as_str().context("no pane id")?.to_owned();
            if entry["outcome"] != "restored" {
                bail!("a pane was not restored: {entry}");
            }
            by_pane.insert(pane, entry);
        }
        let mut methods = Vec::new();
        for pane in &self.panes {
            let entry = by_pane
                .get(&pane.id.to_string())
                .with_context(|| format!("pane {} is not in the report", pane.id))?;
            let method = entry["recovery"].as_str().unwrap_or("?").to_owned();
            *result.recoveries.entry(method.clone()).or_default() += 1;
            result.replay_bytes += entry["replay_bytes"].as_u64().unwrap_or(0);
            methods.push(method);
        }
        Ok(methods)
    }

    /// The final checks of the inputs, and the summary of the run.
    fn summarize(&self, options: &Options, results: Vec<CycleResult>) -> Result<Summary> {
        let mut summary = Summary {
            seed: options.seed,
            cycles: options.cycles,
            panes: self.panes.len(),
            ..Summary::default()
        };
        // The text that each echo pane logged, in the order of the numbers.
        let mut logs: HashMap<usize, Vec<u8>> = HashMap::new();
        for pane in self.echo_panes() {
            let Some(path) = self.panes[pane].def.argv.last() else {
                continue;
            };
            let text = std::fs::read_to_string(path).unwrap_or_default();
            logs.insert(
                pane,
                nh_fixture::echo_seq::read_log(&text)
                    .map_err(|e| anyhow!("the log {path} is bad: {e}"))?,
            );
        }
        let mut payload_bytes: HashMap<usize, usize> = HashMap::new();
        for (id, input) in &self.inputs {
            summary.inputs_sent += 1;
            match input.fate {
                Fate::Acked => summary.inputs_acked_before_kill += 1,
                Fate::ResentAcked => summary.inputs_resent += 1,
                Fate::Abandoned => summary.inputs_abandoned += 1,
                Fate::Sent | Fate::Resent => summary.inputs_missing.push(*id),
            }
            let log = logs.get(&input.pane).map_or(&[][..], Vec::as_slice);
            let seen = count(log, input.payload.as_bytes());
            *payload_bytes.entry(input.pane).or_default() += seen * input.payload.len();
            let required_once = matches!(input.fate, Fate::Acked | Fate::ResentAcked);
            if seen > 1 {
                summary.inputs_applied_twice.push(*id);
            } else if required_once && seen == 0 {
                summary.inputs_missing.push(*id);
            }
        }
        // Bytes in a log that belong to no input.
        for (pane, log) in &logs {
            let known = payload_bytes.get(pane).copied().unwrap_or(0);
            summary.inputs_unknown_bytes += log.len().saturating_sub(known);
        }
        summary.restores = u32::try_from(
            results
                .iter()
                .filter(|r| r.restore_wall_ms.is_some())
                .count(),
        )?;
        summary.kills_during_restore =
            u32::try_from(results.iter().filter(|r| r.killed_during_restore).count())?;
        let numbers = |f: fn(&CycleResult) -> Option<u64>| results.iter().filter_map(f).collect();
        summary.restore_wall_ms = Stats::of(numbers(|r| r.restore_wall_ms));
        summary.restore_core_ms = Stats::of(numbers(|r| r.restore_core_ms));
        summary.settle_ms = Stats::of(numbers(|r| r.settle_ms));
        for result in &results {
            summary.captured_screens += result.captured_panes;
            summary.screens_checked += result.screens_checked;
            summary.replay_bytes_total += result.replay_bytes;
            for (method, n) in &result.recoveries {
                *summary.recoveries.entry(method.clone()).or_default() += n;
            }
            for pane in &result.panes_with_differences {
                if pane.informational {
                    summary.informational_screens_with_differences += 1;
                    summary.informational_cell_differences += pane.cells;
                } else {
                    summary.screens_with_differences += 1;
                    summary.cell_differences += pane.cells;
                    summary.blank_style_only += pane.blank_style_only;
                    if pane.stage == "replay_end" {
                        summary.cell_differences_before_nudge += pane.cells;
                    }
                }
            }
        }
        summary.results = results;
        Ok(summary)
    }

    /// Kills the holders of the run. They outlive their host by design.
    fn clean(&self) {
        kill_holders(&self.dir);
    }
}

/// Kills every holder that the database of a run in `dir` knows.
pub fn kill_holders(dir: &Path) {
    kill_holders_of(&dir.join("data").join(SESSION));
}

/// Kills every holder that `<session_dir>/state.db` knows. The children of the holders end with
/// their terminals. Nothing happens when the database does not exist.
pub fn kill_holders_of(session_dir: &Path) {
    let path = session_dir.join("state.db");
    if !path.is_file() {
        return;
    }
    let Ok(store) = Store::open(&path) else {
        return;
    };
    for liveness in [
        PaneLiveness::Running,
        PaneLiveness::Exited,
        PaneLiveness::Lost,
    ] {
        for pane in store.panes(liveness).unwrap_or_default() {
            if let Some(pid) = Pid::from_raw(pane.holder_pid) {
                let _ = kill_process(pid, Signal::KILL);
            }
        }
    }
}

/// The number of non-overlapping places where `needle` stands in `haystack`.
fn count(haystack: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() || haystack.len() < needle.len() {
        return 0;
    }
    haystack
        .windows(needle.len())
        .filter(|w| *w == needle)
        .count()
}

/// Splits the cell differences of two dumps of the same size into blank cells that differ in style
/// only, and the others.
fn classify(a: &ScreenDump, b: &ScreenDump) -> (usize, usize) {
    if (a.cols, a.rows) != (b.cols, b.rows) {
        return (0, 1);
    }
    let (mut blank, mut other) = (0, 0);
    for y in 0..a.rows {
        for x in 0..a.cols {
            let (ca, cb) = (a.cell(x, y), b.cell(x, y));
            if ca == cb {
                continue;
            }
            match (ca, cb) {
                (Some(ca), Some(cb))
                    if ca.ch == ' '
                        && cb.ch == ' '
                        && ca.zerowidth.is_empty()
                        && cb.zerowidth.is_empty() =>
                {
                    blank += 1;
                }
                _ => other += 1,
            }
        }
    }
    // Cursor, modes, title and palette differences count as others.
    let header = a.cursor != b.cursor
        || a.modes != b.modes
        || a.title != b.title
        || a.palette_overrides != b.palette_overrides;
    (blank, other + usize::from(header))
}

fn millis(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

fn describe(r: &CycleResult) -> String {
    let mut line = format!("cycle {:>3}:", r.cycle);
    if r.killed_during_restore {
        line.push_str(" killed during the restore");
    } else {
        if let Some(ms) = r.restore_wall_ms {
            line.push_str(&format!(
                " restore {ms} ms (core {} ms)",
                r.restore_core_ms.unwrap_or(0)
            ));
        }
        line.push_str(&format!(" captured {}", r.captured_panes));
        if !r.recoveries.is_empty() {
            line.push_str(&format!(" {:?}", r.recoveries));
        }
        line.push_str(&format!(
            " load {} ms resent {} abandoned {}",
            r.load_ms, r.inputs_resent, r.inputs_abandoned
        ));
    }
    let (gate, other): (Vec<_>, Vec<_>) = r
        .panes_with_differences
        .iter()
        .partition(|p| !p.informational);
    if !gate.is_empty() {
        let cells: usize = gate.iter().map(|p| p.cells).sum();
        line.push_str(&format!(
            "  DIFFERENCES: {} screens, {cells} cells",
            gate.len()
        ));
    }
    if !other.is_empty() {
        line.push_str(&format!("  (+{} informational)", other.len()));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_statistics_use_the_nearest_rank() {
        let stats = Stats::of((1..=100).collect());
        assert_eq!(
            (stats.min, stats.median, stats.p95, stats.max),
            (1, 50, 95, 100)
        );
        let one = Stats::of(vec![7]);
        assert_eq!(
            (one.min, one.median, one.p95, one.max, one.n),
            (7, 7, 7, 7, 1)
        );
        assert_eq!(Stats::of(vec![]).n, 0);
    }

    #[test]
    fn a_payload_is_counted_where_it_stands_and_not_twice_in_overlap() {
        assert_eq!(count(b"<00000001><00000002>", b"<00000001>"), 1);
        assert_eq!(count(b"<00000001><00000001>", b"<00000001>"), 2);
        assert_eq!(count(b"", b"<00000001>"), 0);
    }

    #[test]
    fn a_summary_passes_only_without_differences_and_input_faults() {
        let mut summary = Summary::default();
        assert!(summary.passed());
        summary.cell_differences = 1;
        assert!(!summary.passed());
        summary.cell_differences = 0;
        summary.inputs_applied_twice.push(3);
        assert!(!summary.passed());
    }
}
