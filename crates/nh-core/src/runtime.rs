//! The runtime of the core and the handle that hosts use.

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nh_proto::{
    api::{
        calls::system::{ProjectionVersions, ProjectionVersionsResult},
        events::Event,
        ids::{PaneId, Ref, RefValue},
        methods::Call,
        projections::Projection,
        streams::{PeekParams, ScrollbackParams},
    },
    paths::{Env, validate_session_name},
};
use nh_store::{PaneLiveness, Store};
use serde_json::Value;
use tokio::{
    runtime::{Builder, Handle, Runtime},
    sync::broadcast,
};
use ulid::Ulid;

use crate::{
    ErrorCode,
    actor::{self, ActorConfig, ActorHandle, ActorThread, Origin, Scope, State},
    agent_calls,
    agents::{AdapterHosts, Agents, Runs},
    api::{ApiSocket, ApiState, SignalRecord},
    authorize::authorize,
    config::{self, ConfigStore, ConfigWatcher},
    error::{CoreError, RpcError},
    harness::{self, Harnesses},
    health::Health,
    host::Host,
    integration_calls,
    interaction::{Gates, Interactions},
    jobs::Jobs,
    layout::{
        launch::{RuntimeCmd, RuntimeLink, Starting},
        model::Layout,
    },
    pane::{
        Pane, PaneFactory, PaneSpec, RestoreReport, lock, make_private_dir,
        restore::{self, RestoreState},
    },
    pane_calls,
    persist::Persist,
    projection::{MessageSink, ProjectionHub, SubscriptionId},
    projections::Projections,
    render::{FrameSink, RenderParams, RenderServer},
    rich_calls,
    router::HostRouter,
    supervise::{Restart, Supervisor, Task},
    tokens::PaneTokens,
    window_calls,
};

/// The name of the database in the data directory of the session.
const DATABASE: &str = "state.db";

/// The settings of the pane runtime that a test or a measurement changes. The defaults are the
/// values of the product.
#[derive(Clone, Debug)]
pub struct Tuning {
    /// The size of the ring of a new holder.
    pub ring_bytes: u64,
    /// How long the output of a pane is quiet before its holder asks for a checkpoint.
    pub checkpoint_idle: Duration,
    /// The longest time between two checkpoints of a busy pane.
    pub checkpoint_interval: Duration,
    /// How long a query that the holder queued stays fresh.
    pub query_fresh: Duration,
    /// How long an input waits for the ack of the holder before it is reported as `not_written`.
    pub input_ack_timeout: Duration,
    /// The time between the two resizes of the nudge after a restore.
    pub nudge_gap: Duration,
    /// How long a quit waits for the next cut point of a pane.
    pub quit_wait: Duration,
    /// How long a new pane waits between the start of its holder and the first connection to it.
    /// It is 0 in the product. A test makes it long, so that the child writes before the core
    /// attaches, which a fast child does by chance.
    pub attach_delay: Duration,
    /// How long the core holds the hook of a harness for an answer (`gate_timeout`).
    /// The installer sets the hook timeouts to match.
    pub gate_timeout: Duration,
    /// How long a change of the focus of a pane must stand before it counts.
    pub focus_debounce: Duration,
    /// The time in which a run posts one notification.
    pub notify_window: Duration,
    /// How long the execution of a run stays `idle` before its turn counts as ended.
    pub turn_settle: Duration,
    /// Whether the pane service starts the holders of the panes that the layout makes. A test of
    /// the layout alone turns it off, so that its panes stay `created` and no process starts.
    pub start_holders: bool,
    /// How long the offer of a pairing waits for a phone.
    pub phone_offer_ttl: Duration,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            ring_bytes: 16 << 20,
            checkpoint_idle: Duration::from_secs(2),
            checkpoint_interval: Duration::from_secs(30),
            query_fresh: Duration::from_secs(5),
            input_ack_timeout: Duration::from_secs(30),
            nudge_gap: Duration::from_millis(50),
            quit_wait: Duration::from_secs(1),
            attach_delay: Duration::ZERO,
            gate_timeout: Duration::from_secs(30 * 60),
            focus_debounce: crate::focus::DEBOUNCE,
            notify_window: crate::native::COALESCE,
            turn_settle: crate::agents::turn::SETTLE,
            start_holders: true,
            phone_offer_ttl: crate::phones::OFFER_TTL,
        }
    }
}

/// What a host gives the core when it starts it.
#[derive(Clone, Debug)]
pub struct CoreConfig {
    /// The session name. Debug builds of the app use `dev`.
    pub session: String,
    /// The roots of the directories. Tests and development builds set the overrides.
    pub paths: Env,
    /// The `nh` binary that starts holders: the sidecar of the app. In development it
    /// is `target/debug/nh`.
    pub nh_path: PathBuf,
    /// The environment that panes start from. The host captures it.
    pub base_env: Vec<(String, String)>,
    /// The settings of the pane runtime. `Tuning::default()` is the product.
    pub tuning: Tuning,
    /// The host in the process of the core: the app that carries it, with its client
    /// device. `nh daemon` has none: its host requests go to the link clients only.
    /// The host router of the core sends each request to the hosts that its rules name.
    pub host: Option<crate::host::InProcessHost>,
    /// Pane tokens that exist before the API socket serves its first request: a test or a
    /// recording has a fake pane with a fixed token. The product leaves this empty, and the pane
    /// runtime issues the tokens of its panes.
    pub pane_tokens: Vec<(Ulid, String)>,
}

/// The state that the RPC methods and the render server share.
pub(crate) struct Inner {
    pub runtime: Handle,
    pub factory: Arc<PaneFactory>,
    pub panes: Arc<Mutex<HashMap<Ulid, Arc<Pane>>>>,
    pub render: Arc<RenderServer>,
    pub projections: Arc<Projections>,
    pub api: Arc<ApiState>,
    /// The state actor: the commands and the store.
    pub actor: ActorHandle,
    /// The configuration.
    pub config: Arc<ConfigStore>,
    /// The tokens of the panes. A request with one has pane scope.
    pub tokens: Arc<PaneTokens>,
    /// The long commands that run.
    pub jobs: Arc<Jobs>,
    /// What the user needs to know about the core itself.
    pub health: Arc<Health>,
    /// The host router: the host in the process and the link clients.
    pub host: Arc<HostRouter>,
    /// The events of the log, after each commit. The API serves them to subscribers.
    pub events: broadcast::Sender<Event>,
    /// The progress of the restart flow of this core.
    pub restore: Arc<RestoreState>,
    /// The panes of the layout whose holder starts now.
    pub starting: Arc<Starting>,
    /// The watchers that find the agents in the panes.
    pub agents: Arc<Agents>,
    /// What the core does to a run: keys, prompts, launches.
    pub control: Arc<crate::agents::Control>,
    /// The Changes tabs of the windows and their git work.
    pub changes: Arc<crate::changes::ChangesService>,
    /// The Git views of the windows and `git_repos`.
    pub git_view: Arc<crate::git_view::GitViewService>,
    /// The harnesses of this machine and their login state.
    pub discovery: Arc<crate::discovery::Discovery>,
    /// What the host does on `session.shutdown`. Empty: the host ends from its menu.
    pub shutdown: crate::session::ShutdownSlot,
    /// What the host does on `debug.drop_links`. Empty in a host without `link.sock`.
    pub drop_links: crate::session::DropLinksSlot,
    /// The links to the remote machines. Empty in a daemon and in a client.
    pub machines: crate::machines::MachineSlot,
    /// The panes whose run waits after a turn. An input to one of them tells the actor.
    pub turn_waits: Arc<crate::agents::turn::TurnWaits>,
    /// The status of the listener of the phone and the removals of phones.
    pub phones: Arc<crate::phones::PhoneSide>,
    /// The workflow files of the person.
    pub workflows: Arc<crate::workflows::Library>,
    /// The executions of the workflows.
    pub executor: Arc<crate::workflows::Executor>,
}

impl Inner {
    /// Finds a pane by its ULID.
    pub fn pane(&self, id: &str) -> Result<Arc<Pane>, RpcError> {
        let ulid = id
            .parse::<Ulid>()
            .map_err(|_| RpcError::invalid_params(format!("{id:?} is not a pane id")))?;
        lock(&self.panes)
            .get(&ulid)
            .cloned()
            .ok_or_else(|| RpcError::unknown_pane(id))
    }

    pub async fn spawn_pane(&self, spec: PaneSpec) -> Result<Arc<Pane>, CoreError> {
        let pane = self.factory.spawn(spec).await?;
        self.insert_pane(&pane);
        Ok(pane)
    }

    /// See [`CoreHandle::quit`].
    pub async fn quit(&self) -> QuitReport {
        let panes: Vec<Arc<Pane>> = lock(&self.panes).values().cloned().collect();
        let mut tasks = tokio::task::JoinSet::new();
        for pane in panes {
            tasks.spawn(async move { pane.quit().await });
        }
        let mut report = QuitReport::default();
        while let Some(result) = tasks.join_next().await {
            if result.unwrap_or(false) {
                report.checkpointed += 1;
            } else {
                report.without_checkpoint += 1;
            }
        }
        report
    }

    /// Adds a pane to the map. The restart flow uses it for the panes that come back.
    pub fn insert_pane(&self, pane: &Arc<Pane>) {
        let replaced = lock(&self.panes).insert(pane.id(), Arc::clone(pane));
        // A restart makes a new pane object for the same pane. The old one must not report again.
        if let Some(old) = replaced
            && !Arc::ptr_eq(&old, pane)
        {
            old.retire();
        }
        self.render.pane_changed(pane.id());
        self.agents.watch(pane);
    }

    /// Takes a pane out of the map: its window stops drawing it.
    pub fn remove_pane(&self, id: Ulid) -> Option<Arc<Pane>> {
        let removed = lock(&self.panes).remove(&id);
        self.agents.unwatch(id);
        self.render.pane_changed(id);
        self.render.close_peek(id);
        self.projections
            .hub()
            .forget(Projection::PaneText(PaneId::new(id)));
        removed
    }
}

/// The core with its runtime. The host owns it and drops it when the app ends. Panes and their
/// holders keep running after that.
pub struct Core {
    runtime: Option<Runtime>,
    handle: CoreHandle,
    /// The thread of the state actor. It ends with the core.
    actor: ActorThread,
    /// Watches `config.toml`. It ends with the core.
    _config_watcher: Option<ConfigWatcher>,
    /// Watches the directory of the harness manifests. It ends with the core.
    _harness_watcher: Option<harness::HarnessWatcher>,
    /// Watches the folder of the workflow files. It ends with the core.
    _workflow_watcher: Option<crate::watch::DirWatch>,
    /// The task that accepts connections on `api.sock`, and the signal that stops it.
    api_task: Option<tokio::task::JoinHandle<()>>,
    api_stop: Arc<tokio::sync::Notify>,
    /// Removes `api.sock` when the core ends.
    _api_socket: ApiSocket,
}

/// What the host of the adapters sees of the panes: the screen, for the strip of context on a
/// card.
struct PaneEnv {
    panes: Arc<Mutex<HashMap<Ulid, Arc<Pane>>>>,
    home: Option<std::path::PathBuf>,
}

impl crate::agents::host::RenderEnv for PaneEnv {
    fn peek(&self, pane: nh_proto::api::ids::PaneId) -> Vec<String> {
        /// The lines of the strip.
        const LINES: usize = 20;
        let Some(pane) = lock(&self.panes).get(&pane.ulid()).cloned() else {
            return Vec::new();
        };
        let text = pane.text();
        let start = text
            .iter()
            .rposition(|line| !line.trim().is_empty())
            .map_or(0, |last| (last + 1).saturating_sub(LINES));
        text[start..]
            .iter()
            .map(|line| line.trim_end().to_owned())
            .collect()
    }

    fn network_allow(&self) -> Vec<String> {
        Vec::new()
    }

    fn home(&self) -> Option<std::path::PathBuf> {
        self.home.clone()
    }
}

impl Core {
    /// Starts the runtime. Its threads are named `nh-core-N`, and it is separate from the runtime
    /// of the host (a panic in the host must not reach the core).
    pub fn start(config: CoreConfig) -> Result<Self, CoreError> {
        validate_session_name(&config.session)?;
        config.paths.ensure_runtime_root()?;
        // Bind before anything else starts, so that a second core for the session fails at once.
        let mut api_socket = ApiSocket::bind(&config.paths, &config.session)?;
        static THREADS: AtomicUsize = AtomicUsize::new(0);
        let runtime = Builder::new_multi_thread()
            .thread_name_fn(|| format!("nh-core-{}", THREADS.fetch_add(1, Ordering::Relaxed)))
            .enable_all()
            .build()
            .map_err(CoreError::Runtime)?;
        let started = Instant::now();
        let data_dir = config.paths.data_dir(&config.session)?;
        make_private_dir(&data_dir)?;
        let store = Store::open(&data_dir.join(DATABASE))?;
        // The panes of the last core, and the numbers that this core must stay above.
        let (live, max_render_id, max_token, stored_tokens) = {
            let live = store.panes(PaneLiveness::Running)?;
            let with_snapshots = live
                .into_iter()
                .map(|pane| store.snapshots(pane.id).map(|snapshots| (pane, snapshots)))
                .collect::<Result<Vec<_>, _>>()?;
            (
                with_snapshots,
                store.max_render_id()?,
                store.max_fencing_token()?,
                store.pane_tokens()?,
            )
        };
        // A database of an older version has render ids that the counter did not count. The counter
        // starts above them, so that no number comes out twice.
        let mut store = store;
        store.tx(crate::persist::now_ms(), |tx| {
            tx.raise_handle_counter("render", max_render_id)
        })?;
        let projections = Arc::new(Projections::default());
        let health = Health::new(Arc::clone(projections.hub()));
        let jobs = Jobs::new(Arc::clone(projections.hub()));
        let supervisor = Supervisor::new(runtime.handle().clone(), Arc::clone(&health));
        let (events, _) = broadcast::channel(4096);
        // The configuration: `config.toml`, merged over the defaults. A bad file means the
        // defaults and a warning.
        let config_store = Arc::new(ConfigStore::load(
            config.paths.config_dir()?.join(config::FILE_NAME),
        ));
        config::publish_all(&config_store, projections.hub(), &health);
        let config_watcher = {
            let (store, hub, health) = (
                Arc::clone(&config_store),
                Arc::clone(projections.hub()),
                Arc::clone(&health),
            );
            match config_store.watch(move |_| config::publish_all(&store, &hub, &health)) {
                Ok(watcher) => Some(watcher),
                Err(error) => {
                    tracing::warn!(%error, "config.toml is not watched; use config.reload");
                    None
                }
            }
        };
        // The harness manifests: the built-in ones, and the files in `<config>/harnesses/`.
        let harnesses = Harnesses::load(
            config.paths.config_dir()?.join(harness::DIR_NAME),
            config
                .paths
                .data_dir(&config.session)
                .ok()
                .map(|data| data.join("harnesses").join("builtin"))
                .as_deref(),
            Arc::clone(&health),
        );
        let harness_watcher = match harnesses.watch() {
            Ok(watcher) => Some(watcher),
            Err(error) => {
                tracing::warn!(%error, "the harness manifests are not watched; use config.reload");
                None
            }
        };
        // The layout that the last core left: the workspaces, tabs and panes (`tree`).
        let layout = Layout::from_rows(store.load_layout()?)
            .map_err(|e| CoreError::Runtime(std::io::Error::other(e.to_string())))?;
        crate::layout::project::publish(projections.hub(), &Layout::default(), &layout);
        crate::layout::project::publish_palette(projections.hub(), &layout, None);
        // The runs of agents that the last core left open. A run whose pane is gone ends now.
        let agent_runs = Runs::load(&mut store, &|pane| layout.panes.contains_key(&pane))?;
        // The interactions that the last core left open. One whose run is gone
        // ends now. A shim that waited for one resumes it, and its deadline counts again.
        let open_interactions = Interactions::load(
            &mut store,
            nh_proto::api::time::Timestamp(crate::persist::now_ms()),
            &|run| {
                agent_runs
                    .by_id
                    .get(&run)
                    .is_some_and(crate::agents::runs::Run::live)
            },
        )?;
        // The rules of the table. The session rules of the last core end with it.
        let policy = crate::policy::load(&mut store);
        // The paired phones, and the slot of the sender of their push.
        let phones = crate::phones::Phones::load(&store)?;
        let push_slot = Arc::new(crate::phones::PushSlot::default());
        // A test or a recording may shorten the deadline of a gate. Without it the setting counts.
        let gates = Gates::new(
            runtime.handle().clone(),
            (config.tuning.gate_timeout != Tuning::default().gate_timeout)
                .then_some(config.tuning.gate_timeout),
        );
        // The panes of the layout report to the layout. A pane that is still `created` (the last
        // core ended before its holder started) gets its holder now.
        let focus_clock =
            crate::focus::FocusClock::new(runtime.handle().clone(), config.tuning.focus_debounce);
        let native_clock = crate::native::NativeClock::new(runtime.handle().clone());
        let turn_waits = Arc::new(crate::agents::turn::TurnWaits::default());
        let (runtime_link, runtime_rx) = RuntimeLink::channel();
        let adapters = AdapterHosts::new(runtime.handle().clone());
        let bound: std::collections::HashSet<Ulid> =
            layout.panes.keys().map(|pane| pane.ulid()).collect();
        for pane in layout.panes.values() {
            if pane.runtime == crate::layout::model::Runtime::Created
                && let Some(launch) = layout.launch_of(pane)
            {
                runtime_link.send(RuntimeCmd::Spawn(Box::new(launch)));
            }
        }
        // The panes of the core, and the render server that draws the ones that the layout names.
        let panes: Arc<Mutex<HashMap<Ulid, Arc<Pane>>>> = Arc::default();
        let keystrokes =
            crate::keystrokes::Keystrokes::new(runtime.handle().clone(), Arc::clone(&panes));
        let control = crate::agents::Control::new(
            runtime.handle().clone(),
            Arc::clone(&jobs),
            Arc::clone(&panes),
        );
        // One router for the host in the process and the link clients.
        let router = Arc::new(HostRouter::new(config.host.clone(), config.nh_path.clone()));
        // The harnesses of this machine and their login state. The probes run on the
        // blocking pool, at the start and every 30 minutes.
        let discovery = crate::discovery::Discovery::new(
            runtime.handle().clone(),
            config.base_env.clone(),
            Arc::clone(&harnesses),
            Arc::clone(&health),
            Arc::clone(projections.hub()),
            Arc::clone(&router) as Arc<dyn Host>,
        );
        control.set_discovery(Arc::clone(&discovery));
        // The Changes tab and the Git view run git only in the repositories that the person trusts.
        let trusts = crate::changes::trust::Trusts::new(&config.base_env);
        let workflow_facts =
            crate::workflows::FactsReader::new(Arc::clone(&trusts), &config.base_env);
        let changes = crate::changes::ChangesService::new(
            runtime.handle().clone(),
            &config.base_env,
            Arc::clone(&trusts),
        );
        changes.bind_projections(projections.hub());
        // A fetch from a click asks its questions through `nh askpass` and this API socket.
        let git_view = crate::git_view::GitViewService::new(
            runtime.handle().clone(),
            &config.base_env,
            Arc::clone(&jobs),
            Arc::clone(&config_store),
            Some(crate::git_view::Askpass {
                nh: config.nh_path.clone(),
                socket: api_socket.path().to_owned(),
                session: config.session.clone(),
            }),
            trusts,
        );
        git_view.bind_projections(projections.hub());
        // The workflow files of the person, and the executor of their executions.
        let workflow_library = crate::workflows::Library::new(
            config.paths.config_dir()?.join(crate::workflows::DIR_NAME),
            Arc::clone(projections.hub()),
        );
        let workflow_watcher = match workflow_library.watch() {
            Ok(watcher) => Some(watcher),
            Err(error) => {
                tracing::warn!(%error, "the workflow files are not watched");
                None
            }
        };
        workflow_library.publish();
        let executor = crate::workflows::Executor::new(
            runtime.handle().clone(),
            Arc::clone(&control),
            Arc::clone(&jobs),
            Arc::clone(&config_store),
            Arc::clone(projections.hub()),
            Arc::clone(&workflow_library),
            workflow_facts,
            &data_dir,
        );
        let (workflow_signals, workflow_signals_rx) = tokio::sync::mpsc::unbounded_channel();
        let render = Arc::new(RenderServer::new(
            runtime.handle().clone(),
            Arc::clone(&panes),
        ));
        render.sync_layout(&layout);
        let actor = actor::spawn(ActorConfig {
            store,
            state: State::new(
                &config.session,
                crate::persist::now_ms(),
                Arc::clone(&config_store),
                Arc::clone(&jobs),
                Arc::clone(&health),
            )
            .with_layout(layout)
            .with_runs(agent_runs)
            .with_interactions(open_interactions)
            .with_policy(policy)
            .with_gates(Arc::clone(&gates))
            .with_focus_clock(Arc::clone(&focus_clock))
            .with_native_clock(Arc::clone(&native_clock), config.tuning.notify_window)
            .with_turns(Arc::clone(&turn_waits), config.tuning.turn_settle)
            .with_keystrokes(Arc::clone(&keystrokes))
            .with_control(Arc::clone(&control))
            .with_phones(phones)
            .with_workflows(workflow_signals)
            .with_push(Arc::clone(&push_slot))
            .with_adapters(Arc::clone(&adapters))
            .with_runtime(runtime_link.clone())
            .with_render(Arc::clone(&render))
            .with_router(Arc::clone(&router))
            .with_harnesses(Arc::clone(&harnesses)),
            handlers: actor::default_handlers(),
            projections: Arc::clone(projections.hub()),
            events: events.clone(),
            health: Arc::clone(&health),
        })
        .map_err(CoreError::Runtime)?;
        // The first load of the manifests is an event for each manifest.
        harnesses.attach(actor.handle());
        git_view.bind_actor(actor.handle());
        executor.start(actor.handle(), workflow_signals_rx);
        adapters.attach(actor.handle());
        // The runs and the interactions of the last core are in the projections before the first
        // client asks.
        actor.handle().update_detached(|mut parts| {
            // The rows of `attention` of the last core get no push.
            crate::native::push::adopt(parts.state);
            crate::interaction::project::publish_all(parts.projections, &parts.state.interactions);
            crate::transcripts::started(&mut parts);
            crate::agents::runs::publish_all(&mut parts);
            // The host in the process attaches, and the windows of absent clients have no
            // focus.
            crate::clients::started(&mut parts);
            crate::native::started(parts.state);
            crate::phones::publish_all(&mut parts);
            crate::agents::drift::started(&mut parts);
        });
        let persist = Persist::on(actor.handle());
        // Once a second the actor tries a write if the store is degraded. A panic in a
        // task below starts it again, and `health` hears if it gives up.
        {
            let handle = actor.handle();
            supervisor.spawn(
                Task::new("storage-probe").restart(Restart::Times(5)),
                move || {
                    let handle = handle.clone();
                    async move {
                        let mut interval = tokio::time::interval(Duration::from_secs(1));
                        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                        loop {
                            interval.tick().await;
                            handle.tick();
                        }
                    }
                },
            );
        }
        // The retention of the store: old events, and the oldest scrollback of a pane over its
        // cap, go at the start and then once an hour.
        {
            let (handle, config) = (actor.handle(), Arc::clone(&config_store));
            supervisor.spawn(
                Task::new("retention").restart(Restart::Times(5)),
                move || crate::retention::run(handle.clone(), Arc::clone(&config)),
            );
        }
        // The hub makes one patch for the changes of each 16 ms. The task ends with the
        // runtime.
        {
            let hub = Arc::clone(projections.hub());
            supervisor.spawn(
                Task::new("projection-ticker").restart(Restart::Times(5)),
                move || Arc::clone(&hub).run_ticker(),
            );
        }
        let tokens = PaneTokens::new();
        // The tokens of the last core stay valid, so that a shim that waited for this core finds
        // its token.
        for (pane, token) in stored_tokens.iter().chain(&config.pane_tokens) {
            tokens.register(*pane, token.as_str());
        }
        let api = Arc::new(ApiState::with_gates(
            Arc::clone(&gates),
            Arc::clone(&tokens),
        ));
        let api_stop = Arc::new(tokio::sync::Notify::new());
        let uid = config.paths.uid;
        // The start time in milliseconds, and above every token that the database holds, so that a
        // core that starts later always has the higher token, also when the clock went
        // back.
        let fencing_token = u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_millis()),
        )
        .unwrap_or(0)
        .max(max_token.saturating_add(1));
        let starting = runtime_link.starting();
        // The shim of Codex. A directory that cannot be written means no shim.
        let shims_dir = {
            let dir = crate::shims::dir(&data_dir);
            match crate::shims::ensure(&dir) {
                Ok(()) => Some(dir),
                Err(error) => {
                    tracing::warn!(%error, "the shims are not written, so panes get none");
                    None
                }
            }
        };
        let agents = Agents::new(
            actor.handle(),
            Arc::clone(&harnesses),
            Arc::clone(&health),
            Arc::clone(&adapters),
            config.base_env.clone(),
            runtime.handle().clone(),
        );
        // The gates need the host of the adapters to open a hook, and the host needs a view of the
        // panes to render the payload of an interaction.
        gates.bind(actor.handle(), Arc::clone(&adapters));
        focus_clock.bind(actor.handle());
        native_clock.bind(actor.handle());
        keystrokes.bind(actor.handle());
        control.bind(actor.handle());
        agents.set_discovery(Arc::clone(&discovery));
        discovery.spawn_schedule();
        adapters.set_probe(config.base_env.clone(), Arc::clone(&harnesses));
        adapters.set_env(Arc::new(PaneEnv {
            panes: Arc::clone(&panes),
            home: config
                .base_env
                .iter()
                .find(|(name, _)| name == "HOME")
                .map(|(_, home)| std::path::PathBuf::from(home)),
        }));
        let phone_side = Arc::new(crate::phones::PhoneSide::new(
            config.tuning.phone_offer_ttl,
            data_dir.join("phone"),
            push_slot,
        ));
        let factory = Arc::new(PaneFactory {
            session: config.session,
            paths: config.paths,
            nh_path: config.nh_path,
            base_env: config.base_env,
            shims_dir,
            fencing_token,
            projections: Arc::clone(&projections),
            runtime: runtime.handle().clone(),
            persist,
            tuning: config.tuning,
            health: Arc::clone(&health),
            host: Arc::clone(&router),
            actor: actor.handle(),
            bound: Mutex::new(bound),
        });
        let restore_state = RestoreState::new(started);
        let inner = Arc::new(Inner {
            runtime: runtime.handle().clone(),
            factory,
            panes,
            render,
            projections,
            api: Arc::clone(&api),
            actor: actor.handle(),
            config: config_store,
            tokens,
            jobs,
            health,
            host: router,
            events,
            restore: Arc::clone(&restore_state),
            starting,
            agents,
            control: Arc::clone(&control),
            changes,
            git_view,
            discovery,
            shutdown: crate::session::ShutdownSlot::default(),
            drop_links: crate::session::DropLinksSlot::default(),
            machines: crate::machines::MachineSlot::default(),
            turn_waits,
            phones: Arc::clone(&phone_side),
            workflows: workflow_library,
            executor,
        });
        // The API server starts before the panes of the last core come back, because an agent in
        // such a pane can send a hook at once. It runs the calls on the core, so it starts here.
        api.bind_core(Arc::downgrade(&inner));
        crate::machines::publish(&inner);
        // The listener of the phone is off until its host starts it.
        phone_side.set_status(phone_side.status(), inner.projections.hub());
        let api_task;
        {
            let listener = {
                let _guard = runtime.enter();
                api_socket.take_listener().map_err(|source| CoreError::Io {
                    path: api_socket.path().to_owned(),
                    source,
                })?
            };
            api_task = Some(supervisor.spawn_once(
                "api-server",
                crate::api::serve(listener, uid, Arc::clone(&api), Arc::clone(&api_stop)),
            ));
        }
        // The transcripts of the runs: usage, cost, limits and the timeline.
        supervisor.spawn_once("transcripts", crate::transcripts::run(actor.handle()));
        // The text mirror of the panes that someone follows (`pane_text:<pane>`).
        supervisor.spawn_once("pane-text", crate::pane_text::run(Arc::downgrade(&inner)));
        // The holders that the layout asks for.
        supervisor.spawn_once(
            "pane-service",
            crate::pane::service::run(Arc::downgrade(&inner), runtime_rx),
        );
        restore::start(&inner, &restore_state, live);
        // When the panes are back, the agents that ended with the machine are resumed or offered.
        {
            let (inner, restore) = (Arc::clone(&inner), Arc::clone(&restore_state));
            runtime.spawn(async move {
                let mut done = restore.subscribe();
                let _ = done.wait_for(|done| *done).await;
                let control = Arc::clone(&inner.control);
                inner.actor.update_detached(move |mut parts| {
                    crate::agents::reboot::offer(&mut parts, &control);
                });
            });
        }
        Ok(Self {
            runtime: Some(runtime),
            handle: CoreHandle { inner },
            actor,
            _config_watcher: config_watcher,
            _harness_watcher: harness_watcher,
            _workflow_watcher: workflow_watcher,
            api_task,
            api_stop,
            _api_socket: api_socket,
        })
    }

    /// A handle for the host. It can be cloned and sent to any thread.
    pub fn handle(&self) -> CoreHandle {
        self.handle.clone()
    }

    /// Stops the runtime, drops its tasks and waits at most `timeout` for the work on blocking
    /// threads, for example a write to the database. Panes and their holders keep running. This is
    /// what a crash of the host looks like to them, except that the database is whole.
    ///
    /// Call it from a thread that belongs to no runtime. [`Drop`] does not wait.
    pub fn shutdown(mut self, timeout: Duration) {
        if let Some(runtime) = self.runtime.take() {
            // Close the listener first. The tasks of the runtime end in no set order, and a shim
            // that lost its connection must not find a listener that accepts one more connection
            // and drops it. A crash of the process closes everything at once, and a shim then
            // finds nobody until the next core binds.
            if let Some(task) = self.api_task.take() {
                self.api_stop.notify_one();
                let _ = runtime.block_on(task);
            }
            runtime.shutdown_timeout(timeout);
        }
        // The database closes here, after the last command and the last job.
        self.actor.stop();
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            // The host may drop the core inside an async context. `shutdown_background` does not
            // wait for the tasks, and it does not panic there.
            runtime.shutdown_background();
        }
    }
}

/// The error of a method of the contract that has no handler in this core.
fn no_handler(method: &str) -> RpcError {
    RpcError::new(
        ErrorCode::Unsupported,
        format!("{method} has no handler yet"),
    )
}

/// Whether a holder process is still there. A pane that has no holder has the pid 0.
fn holder_alive(pid: i32) -> bool {
    rustix::process::Pid::from_raw(pid)
        .is_some_and(|pid| rustix::process::test_kill_process(pid).is_ok())
}

/// Polls `done` every 10 ms until it is true or `within` has passed.
async fn wait_until(within: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + within;
    while !done() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// What [`CoreHandle::quit`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QuitReport {
    /// The panes that stored a checkpoint at the end.
    pub checkpointed: usize,
    /// The panes that ended their lease without a new checkpoint.
    pub without_checkpoint: usize,
}

/// What [`CoreHandle::stop_everything`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StopReport {
    /// The panes whose child ended after the polite signals.
    pub ended: usize,
    /// The panes whose child had to be killed.
    pub killed: usize,
    /// The panes whose child still ran at the end. Their holders keep running.
    pub left: usize,
}

/// How a host and its webviews call the core. Cloning is cheap.
#[derive(Clone)]
pub struct CoreHandle {
    inner: Arc<Inner>,
}

impl CoreHandle {
    /// The host tells what the system lets the app do with notifications. A permission
    /// that is not granted is a line in the doctor, because an approval then waits in the tray
    /// and the cards without a sound.
    pub fn notification_permission(&self, permission: crate::host::NotificationPermission) {
        use crate::host::NotificationPermission::{Denied, Granted, Unavailable};
        let health = &self.inner.health;
        match permission {
            Granted => health.clear("notifications"),
            Denied => health.warn(
                "notifications",
                "Notifications are off for neohar, so an approval makes no sound and no banner.",
                Some("Allow them in System Settings, Notifications, neohar.".to_owned()),
            ),
            Unavailable => health.warn(
                "notifications",
                "This run of neohar cannot show notifications: it does not run from an app bundle.",
                None,
            ),
        }
    }

    /// Attaches a client of a link. Until the attachment is dropped, the host router
    /// sends to `host` the requests of the [`Host`] trait that the rules give to this client.
    ///
    /// At once the client gets the badge, the tray and the notifications of the open
    /// interactions, and it adopts the rows of `windows` that have no owner. When the attachment
    /// is dropped, the windows of the client have no focus and are not visible.
    pub fn attach_client(
        &self,
        info: crate::host::ClientInfo,
        host: Arc<dyn Host>,
    ) -> crate::host::ClientAttachment {
        let attachment = crate::clients::attach(&self.inner.host, &self.inner.actor, info, host);
        // A client that connects sees the login state of now (When).
        let discovery = Arc::clone(&self.inner.discovery);
        self.inner.runtime.spawn(async move {
            if let Err(error) = discovery.refresh(None).await {
                tracing::debug!(%error, "the probe of the harnesses after an attach failed");
            }
        });
        attachment
    }

    /// Lets a client end the host with `session.shutdown`. The core calls `hook` on a
    /// task of its runtime, after the call is accepted, so `hook` must only signal the thread of
    /// the host: that thread then calls [`CoreHandle::quit_blocking`] and [`Core::shutdown`].
    /// Without a hook the method answers `unsupported`: the app quits from its menu.
    pub fn accept_shutdown(&self, hook: impl Fn() + Send + Sync + 'static) {
        self.inner.shutdown.set(Arc::new(hook));
    }

    /// Sets what `debug.drop_links` does: the host closes every link of `link.sock`. A method of
    /// development builds, for the tests of the reconnect.
    pub fn accept_drop_links(&self, hook: impl Fn() + Send + Sync + 'static) {
        self.inner.drop_links.set(Arc::new(hook));
    }

    /// Gives the core the links to the remote machines. Only a host that
    /// connects over ssh calls it: the app when it hosts the core. The core hands the links the
    /// machines of `config.toml`, and publishes the projection `machines`. The second call is
    /// ignored.
    pub fn set_machine_links(&self, links: Arc<dyn crate::machines::MachineLinks>) {
        crate::machines::attach(&self.inner, links);
    }

    /// Keeps the machine out of idle sleep while the setting `daemon.prevent_sleep` wants it:
    /// by default while an agent run is `working` or a gate holds a hook. Only
    /// `nh daemon` calls it. The task ends with the core. A no-op on systems other than macOS.
    pub fn prevent_sleep(&self) {
        let inner = Arc::downgrade(&self.inner);
        self.inner.runtime.spawn(crate::power::run(
            inner,
            std::process::id(),
            crate::power::Holder::Daemon,
        ));
    }

    /// Keeps the machine out of idle sleep for a paired phone: while
    /// `phone.keep_awake` is on, a phone has a push subscription, and a run works, a gate holds a
    /// hook or a push went less than 2 h ago. The app calls it when it hosts the core. The task
    /// ends with the core. A no-op on systems other than macOS.
    pub fn keep_awake_for_phones(&self) {
        let inner = Arc::downgrade(&self.inner);
        self.inner.runtime.spawn(crate::power::run(
            inner,
            std::process::id(),
            crate::power::Holder::App,
        ));
    }

    /// Whether the core wants the machine awake now, by the rules of
    /// [`CoreHandle::prevent_sleep`]. The tests read it.
    pub async fn wants_awake(&self) -> bool {
        crate::power::wanted(&self.inner, crate::power::Holder::Daemon).await
    }

    /// The host in this process reports its screen: locked or not, and whether its
    /// display sleeps. While either holds, the windows of this host count as without the focus,
    /// and the first report of it pushes each open row of `attention` that had no push. A host
    /// that reports nothing uses the focus alone.
    pub fn report_screen(&self, locked: bool, display_asleep: bool) {
        let screen = crate::focus::Screen {
            locked,
            display_asleep,
        };
        self.inner
            .actor
            .update_detached(move |mut parts| crate::focus::screen(&mut parts, screen));
    }

    /// A notice of another machine that the host got over its link: the paired phones
    /// get a push, with the rules of `native::push` and the label of the machine in the title.
    pub fn push_machine_notice(&self, machine: &str, notice: crate::host::HostNotification) {
        let machine = machine.to_owned();
        self.inner.actor.update_detached(move |mut parts| {
            crate::native::push::machine_notice(&mut parts, &machine, &notice);
        });
    }

    /// The device of the host in the process of the core. `nh daemon` has none.
    pub fn local_device(&self) -> Option<Ulid> {
        self.inner.host.local_device()
    }

    /// The host router of the core.
    pub fn router(&self) -> &Arc<HostRouter> {
        &self.inner.host
    }

    /// Calls a method of the API as the UI of the host in the process: the origin has its device,
    /// if the core has such a host.
    ///
    /// The methods that only change state in memory run in the caller. The others run on the
    /// runtime of the core.
    pub async fn rpc(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let origin = match self.local_device() {
            Some(device) => Origin::ui().from_device(device),
            None => Origin::ui(),
        };
        self.call_as(origin, method, params).await
    }

    /// Calls a method as `origin`. This is the one door for the UI, the CLI and the panes. It
    /// refuses a caller that the method is not for ([`crate::authorize`]), then runs the handler
    /// of the method. A method of the contract that has no handler is `unsupported`.
    pub async fn call_as(
        &self,
        origin: Origin,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        let known = authorize(method, &origin)?;
        // A subscription of the UI is tied to a channel of the host, not to a connection.
        if method == "projection.unsubscribe" && origin.scope == Scope::Ui {
            let subscription = params
                .get("subscription")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("subscription is a string"))?;
            return if self.unsubscribe_tracked(subscription) {
                Ok(serde_json::json!({}))
            } else {
                Err(RpcError::new(
                    ErrorCode::NotFound,
                    format!("no subscription {subscription}"),
                ))
            };
        }
        // The versions of projections need no connection and no actor.
        if method == "projection.versions" {
            let params: ProjectionVersions = serde_json::from_value(params)
                .map_err(|error| RpcError::invalid_params(error.to_string()))?;
            let hub = self.inner.projections.hub();
            let versions: BTreeMap<String, u64> = params
                .names
                .iter()
                .filter_map(|name| {
                    hub.version(*name)
                        .map(|version| (name.to_string(), version))
                })
                .collect();
            return serde_json::to_value(ProjectionVersionsResult { versions })
                .map_err(|error| RpcError::internal(error.to_string()));
        }
        let direct = pane_calls::handles(method)
            || crate::changes::handles(method)
            || crate::git_view::handles(method)
            || crate::workflows::handles(method)
            || crate::files::handles(method)
            || crate::doctor::handles(method)
            || agent_calls::handles(method)
            || window_calls::handles(method)
            || integration_calls::handles(method)
            || rich_calls::handles(method)
            || crate::transcripts::calls::handles(method)
            || crate::search::handles(method)
            || crate::session::handles(method)
            || crate::askpass::handles(method)
            || crate::machines::handles(method)
            || crate::phones::handles(method);
        if direct {
            let call = Call::parse(method, params).map_err(|error| {
                let call_error = error.call_error();
                let mut mapped = RpcError::new(call_error.code, call_error.message);
                mapped.details = call_error.details;
                mapped
            })?;
            return if pane_calls::handles(method) {
                pane_calls::run(&self.inner, call, origin).await
            } else if crate::changes::handles(method) {
                crate::changes::run(&self.inner, call, origin).await
            } else if crate::git_view::handles(method) {
                crate::git_view::run(&self.inner, call, origin).await
            } else if crate::workflows::handles(method) {
                crate::workflows::run(&self.inner, call, origin).await
            } else if crate::files::handles(method) {
                crate::files::run(&self.inner, call, origin).await
            } else if crate::doctor::handles(method) {
                crate::doctor::run(&self.inner, call).await
            } else if agent_calls::handles(method) {
                agent_calls::run(&self.inner, call, origin).await
            } else if integration_calls::handles(method) {
                integration_calls::run(&self.inner, call).await
            } else if rich_calls::handles(method) {
                rich_calls::run(&self.inner, call).await
            } else if crate::transcripts::calls::handles(method) {
                crate::transcripts::calls::run(&self.inner, call).await
            } else if crate::search::handles(method) {
                crate::search::run(&self.inner, call).await
            } else if crate::session::handles(method) {
                crate::session::run(&self.inner, call).await
            } else if crate::askpass::handles(method) {
                crate::askpass::run(&self.inner, call).await
            } else if crate::machines::handles(method) {
                crate::machines::run(&self.inner, call).await
            } else if crate::phones::handles(method) {
                crate::phones::run(&self.inner, call, origin).await
            } else {
                window_calls::run(&self.inner, call, origin).await
            };
        }
        if self.inner.actor.handles(method) {
            return self.call(method, params, origin).await;
        }
        #[cfg(feature = "debug")]
        if method.starts_with("debug.") {
            let result = match crate::debug_calls::call_sync(&self.inner, method, &params) {
                Some(result) => result,
                None => {
                    let inner = Arc::clone(&self.inner);
                    let name = method.to_owned();
                    self.inner
                        .runtime
                        .spawn(async move {
                            crate::debug_calls::call_async(&inner, &name, params).await
                        })
                        .await
                        .map_err(|error| {
                            RpcError::internal(format!("the method task failed: {error}"))
                        })?
                }
            };
            return match result {
                Err(error) if error.code == ErrorCode::MethodNotFound && known.is_some() => {
                    Err(no_handler(method))
                }
                other => other,
            };
        }
        // A method of the contract that nothing implements yet.
        Err(match known {
            Some(_) => no_handler(method),
            None => RpcError::method_not_found(method),
        })
    }

    pub(crate) fn from_inner(inner: Arc<Inner>) -> Self {
        Self { inner }
    }

    /// The shared state of the core, for the modules of the crate that add methods to the handle.
    pub(crate) fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    /// The tokens of the panes. The pane runtime issues one when a pane starts.
    pub fn pane_tokens(&self) -> Arc<PaneTokens> {
        Arc::clone(&self.inner.tokens)
    }

    /// How many `adapter.signal` and `adapter.gate` requests the API socket got.
    pub fn api_totals(&self) -> (u64, u64) {
        self.inner.api.totals()
    }

    /// Opens a stream: `render`, `peek` or `scrollback`. The frames go to `sink` until it says
    /// that its receiver is gone.
    pub fn stream(
        &self,
        kind: &str,
        params: Value,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError> {
        match kind {
            "peek" => return self.open_peek(params, sink),
            "scrollback" => return self.open_scrollback(params, sink),
            "render" => {}
            _ => {
                return Err(RpcError::new(
                    ErrorCode::NotFound,
                    format!("no stream {kind:?}"),
                ));
            }
        }
        let params: RenderParams =
            serde_json::from_value(params).map_err(|e| RpcError::invalid_params(e.to_string()))?;
        self.inner.render.open_stream(&params, sink)
    }

    /// The pane that a stream names. A stream names it by id: the host knows the ids from the
    /// `panes` projection.
    fn stream_pane(&self, pane: &Ref<PaneId>) -> Result<Arc<Pane>, RpcError> {
        let RefValue::Id(id) = &pane.0 else {
            return Err(RpcError::invalid_params(
                "a stream names a pane by its id, not by a handle",
            ));
        };
        self.inner.pane(&id.to_string()).map_err(|_| {
            RpcError::new(
                ErrorCode::NotFound,
                format!("no pane {pane} with a process"),
            )
        })
    }

    fn open_peek(&self, params: Value, sink: Arc<dyn FrameSink>) -> Result<(), RpcError> {
        let params: PeekParams =
            serde_json::from_value(params).map_err(|e| RpcError::invalid_params(e.to_string()))?;
        let pane = self.stream_pane(&params.pane)?;
        self.inner.render.open_peek(
            pane,
            Some(params.max_hz),
            params.max_cols,
            params.max_rows,
            sink,
        );
        Ok(())
    }

    fn open_scrollback(&self, params: Value, sink: Arc<dyn FrameSink>) -> Result<(), RpcError> {
        let params: ScrollbackParams =
            serde_json::from_value(params).map_err(|e| RpcError::invalid_params(e.to_string()))?;
        let pane = self.stream_pane(&params.pane)?;
        self.inner
            .render
            .open_scrollback(pane, params.from_line, u64::from(params.count), sink);
        Ok(())
    }

    /// Subscribes to a projection. `since` is a version the client has.
    /// The sink gets the patches since that version, or `resync` when the core no longer has them.
    pub fn subscribe(
        &self,
        name: &str,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Result<(), RpcError> {
        self.inner
            .projections
            .subscribe(name, since, sink)
            .map(|_| ())
    }

    /// Like [`CoreHandle::subscribe`], and it returns the id of the subscription: the name of the
    /// projection, `#`, and a number. A host with no connection to tie the subscription to (the
    /// Tauri host, the dev WebSocket) hands the id to the page, and the page ends the
    /// subscription with `projection.unsubscribe {subscription}`.
    pub fn subscribe_tracked(
        &self,
        name: &str,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Result<String, RpcError> {
        let (id, _) = self.inner.projections.subscribe(name, since, sink)?;
        Ok(format!("{name}#{}", id.0))
    }

    /// Ends a subscription that [`CoreHandle::subscribe_tracked`] made. Returns whether it
    /// existed.
    pub fn unsubscribe_tracked(&self, subscription: &str) -> bool {
        let Some((name, number)) = subscription.rsplit_once('#') else {
            return false;
        };
        let (Ok(projection), Ok(number)) = (
            name.parse::<nh_proto::api::projections::Projection>(),
            number.parse::<u64>(),
        ) else {
            return false;
        };
        self.inner
            .projections
            .hub()
            .unsubscribe(projection, SubscriptionId(number))
    }

    /// Calls a method of the state actor with the origin of the caller.
    pub async fn call(
        &self,
        method: &str,
        params: Value,
        origin: Origin,
    ) -> Result<Value, RpcError> {
        let call = Call::parse(method, params).map_err(|error| {
            let call_error = error.call_error();
            let mut mapped = RpcError::new(call_error.code, call_error.message);
            mapped.details = call_error.details;
            mapped
        })?;
        self.inner.actor.call(call, origin).await
    }

    /// A blob of the store: its bytes and its type (`nh://blob/<hash>`). `None` when the
    /// store has no blob with this hash. A name that is not a hash is `None` too.
    pub async fn blob(&self, hash: &str) -> Result<Option<(Vec<u8>, String)>, RpcError> {
        let hash = hash.to_owned();
        let found = self
            .inner
            .factory
            .persist
            .run(move |store| {
                let Some(info) = store.blob_info(&hash)? else {
                    return Ok(None);
                };
                Ok(store.get_blob(&hash)?.map(|bytes| (bytes, info.mime)))
            })
            .await;
        match found {
            Ok(found) => Ok(found),
            // A name that is not a hash cannot name a blob.
            Err(CoreError::Store(nh_store::StoreError::BadValue(_))) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Keeps the bytes of an uploaded file as a blob and returns its hash and its type. The core cleans the
    /// name and takes the type from its extension. A file over
    /// [`crate::UPLOAD_MAX_BYTES`] is `too_large`.
    pub async fn put_upload(
        &self,
        name: &str,
        bytes: Vec<u8>,
    ) -> Result<(String, String), RpcError> {
        if bytes.len() as u64 > crate::UPLOAD_MAX_BYTES {
            return Err(RpcError::new(
                ErrorCode::TooLarge,
                format!("a file has at most {} bytes", crate::UPLOAD_MAX_BYTES),
            ));
        }
        let mime = crate::uploads::mime_of(&crate::uploads::clean_name(name));
        let hash = self
            .inner
            .factory
            .persist
            .run(move |store| store.put_blob(&bytes, mime, crate::persist::now_ms()))
            .await
            .map_err(RpcError::from)?;
        Ok((hash, mime.to_owned()))
    }

    /// The projections of the core, for a host that reads them without a subscription.
    pub fn projections(&self) -> Arc<ProjectionHub> {
        Arc::clone(self.inner.projections.hub())
    }

    /// The configuration in force, and the settings with their sources.
    pub fn config(&self) -> Arc<ConfigStore> {
        Arc::clone(&self.inner.config)
    }

    /// The registry of the jobs: a command that takes long starts one here.
    pub fn jobs(&self) -> Arc<Jobs> {
        Arc::clone(&self.inner.jobs)
    }

    /// The health of the core: parts of the host report facts here (the renderer in use).
    pub fn health(&self) -> Arc<Health> {
        Arc::clone(&self.inner.health)
    }

    /// The events of the log, after each commit. A subscriber that falls behind by more than 4,096
    /// events gets a `Lagged` error and takes a fresh snapshot.
    pub fn events(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }

    /// Starts a pane that is not in the layout, as `debug.spawn_panes` does. No window draws it. A
    /// test calls it directly.
    pub async fn spawn_pane(&self, spec: PaneSpec) -> Result<Arc<Pane>, CoreError> {
        let inner = Arc::clone(&self.inner);
        self.inner
            .runtime
            .spawn(async move { inner.spawn_pane(spec).await })
            .await
            .map_err(|error| CoreError::Runtime(std::io::Error::other(error)))?
    }

    /// The last signals that hook shims sent to `api.sock`, oldest first. At most 64.
    pub fn recent_signals(&self) -> Vec<SignalRecord> {
        self.inner.api.recent_signals()
    }

    /// The login environment of the person: the environment of the programs that the core
    /// starts, and the `PATH` that it looks for them in.
    pub fn login_env(&self) -> Vec<(String, String)> {
        self.inner.factory.base_env.clone()
    }

    /// The runtime of the core. The server of `link.sock` runs its tasks here, so
    /// that it lives and ends with the core.
    pub fn runtime(&self) -> Handle {
        self.inner.runtime.clone()
    }

    /// Waits until every pane of the previous core is back, or lost ("Launch"). It
    /// returns at once when the database held no running pane.
    pub async fn wait_restored(&self) {
        let mut done = self.inner.restore.subscribe();
        // The sender lives as long as the core, so the wait ends with `true`.
        let _ = done.wait_for(|done| *done).await;
    }

    /// What the restart of this core did, pane by pane.
    pub fn restore_report(&self) -> RestoreReport {
        self.inner.restore.report()
    }

    /// Ends the lease on every pane after a checkpoint of each ("Quit"). The children
    /// keep running. A pane that gets no cut point within [`Tuning::quit_wait`] ends its lease
    /// without a new checkpoint, and the next core replays more of the journal. Returns how many
    /// panes stored a checkpoint and how many did not.
    ///
    /// Drop the core afterwards. Call [`CoreHandle::quit_blocking`] from a thread that is no
    /// task of the core.
    pub async fn quit(&self) -> QuitReport {
        self.inner.quit().await
    }

    /// Like [`CoreHandle::quit`], for a host that quits from a thread that is no task. It waits
    /// at most `timeout`.
    pub fn quit_blocking(&self, timeout: Duration) -> QuitReport {
        let (tx, rx) = std::sync::mpsc::channel();
        let this = self.clone();
        self.inner.runtime.spawn(async move {
            let _ = tx.send(this.quit().await);
        });
        rx.recv_timeout(timeout).unwrap_or_default()
    }

    /// "Quit and stop everything": ends every pane, then the holders leave. Each child
    /// gets the polite signals first: `Term` to the foreground group, and `Hup` to the child, which
    /// is what closing a terminal does and which a shell obeys. After `grace` (5 s in the product)
    /// the children that run get `Kill`. The pane tasks persist each exit and send `ExitAck`, so
    /// each holder leaves. The agents get `AGENT_GRACE` before this to end by themselves.
    ///
    /// Drop the core afterwards. Call [`CoreHandle::stop_everything_blocking`] from a thread that
    /// is no task of the core.
    pub async fn stop_everything(&self, grace: Duration) -> StopReport {
        self.stop_everything_with(grace, Some(crate::session::AGENT_GRACE))
            .await
    }

    /// Like [`CoreHandle::stop_everything`]. `agents` is how long the agents get to end by
    /// themselves first. `None` skips that step: `session.stop_everything {force: true}`.
    pub async fn stop_everything_with(
        &self,
        grace: Duration,
        agents: Option<Duration>,
    ) -> StopReport {
        // A pane whose holder starts has no child yet. Wait until it is in the map.
        wait_until(Duration::from_secs(10), || self.inner.starting.is_empty()).await;
        // The agents get their own way out first.
        if let Some(agents) = agents {
            self.inner.control.quit_agents(agents).await;
        }
        let panes = self.panes();
        let running = |panes: &[Arc<Pane>]| {
            panes
                .iter()
                .filter(|pane| pane.info().liveness == crate::pane::Liveness::Running)
                .cloned()
                .collect::<Vec<_>>()
        };
        for pane in &panes {
            pane.terminate();
        }
        wait_until(grace, || running(&panes).is_empty()).await;
        let stragglers = running(&panes);
        for pane in &stragglers {
            pane.kill();
        }
        let ended = panes.len() - stragglers.len();
        wait_until(Duration::from_secs(3), || running(&stragglers).is_empty()).await;
        let left = running(&stragglers).len();
        // A holder leaves when its pane task has stored the exit and sent the acknowledgement. A
        // host that drops the core right after this call must not leave one behind: it would keep
        // its terminal for ever, and a few hundred of them use all the terminals of the machine.
        wait_until(Duration::from_secs(3), || {
            !panes.iter().any(|pane| holder_alive(pane.holder_pid()))
        })
        .await;
        StopReport {
            ended,
            killed: stragglers.len() - left,
            left,
        }
    }

    /// Like [`CoreHandle::stop_everything`], for a host that quits from a thread that is no task.
    /// It waits at most `timeout`; a core that takes longer gives an empty report.
    pub fn stop_everything_blocking(&self, grace: Duration, timeout: Duration) -> StopReport {
        let (tx, rx) = std::sync::mpsc::channel();
        let this = self.clone();
        self.inner.runtime.spawn(async move {
            let _ = tx.send(this.stop_everything(grace).await);
        });
        rx.recv_timeout(timeout).unwrap_or_default()
    }

    /// The pane with this id.
    pub fn pane(&self, id: Ulid) -> Option<Arc<Pane>> {
        lock(&self.inner.panes).get(&id).cloned()
    }

    /// Every pane that this core started, in no particular order.
    pub fn panes(&self) -> Vec<Arc<Pane>> {
        lock(&self.inner.panes).values().cloned().collect()
    }

    /// Kills the children of all panes and waits until each pane has seen its child exit and its
    /// holder has left, but not longer than `timeout`. It blocks the calling thread, so a host
    /// calls it from its exit handler, not from a task. A pane whose holder is gone counts as
    /// stopped.
    pub fn stop_panes_blocking(&self, timeout: std::time::Duration) {
        self.stop_panes();
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            let busy = lock(&self.inner.panes).values().any(|pane| {
                pane.info().liveness == crate::pane::Liveness::Running
                    || holder_alive(pane.holder_pid())
            });
            if !busy {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// Kills the child of every pane and the processes that it started. The holders then
    /// exit. This is not the app's quit: a quit leaves the panes running. Tests and
    /// "quit and stop everything" use it.
    pub fn stop_panes(&self) {
        // A pane whose holder starts has no child to kill yet. Wait until it is in the map, but not
        // for ever.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !self.inner.starting.is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        for pane in lock(&self.inner.panes).values() {
            pane.kill_all();
        }
    }
}
