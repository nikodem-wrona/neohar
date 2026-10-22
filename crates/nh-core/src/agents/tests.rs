//! The runs in the state of the actor: a run starts as a suggestion, learns its version,
//! shows in the projections, ends with its process, and comes back after a restart.

use std::sync::Arc;

use nh_agents::adapter::{Adapter, AdapterCaps, AdapterCtx, AdapterSignal, SignalReply};
use nh_agents::{
    arbiter::{Input, Observation},
    matcher::Detection,
};
use nh_proto::api::{
    calls::workspace::WorkspaceCreate,
    common::{Harness, Source},
    events::Event,
    ids::{PaneId, RunId, WindowId},
    model::{AdapterHealth, Execution, Liveness},
    projections::Projection,
    time::Timestamp,
};
use nh_store::Store;
use semver::Version;
use serde_json::Value;
use tokio::sync::broadcast;

use super::{AdapterHosts, runs, runs::Runs};
use crate::{
    actor::{self, ActorConfig, ActorThread, State},
    focus::FocusClock,
    harness::Harnesses,
    health::Health,
    jobs::Jobs,
    layout::{model::Layout, testing::Counters},
    persist::now_ms,
    projection::ProjectionHub,
};

/// What a test changes in the fixture.
#[derive(Default)]
pub(crate) struct Options {
    pub(crate) gate_timeout: Option<std::time::Duration>,
    /// The text of `config.toml`.
    pub(crate) config: Option<&'static str>,
    /// How long a change of the focus stands before it counts. `None`: it counts at once.
    pub(crate) focus_debounce: Option<std::time::Duration>,
    /// The time in which a run posts one notification. `None`: ten seconds.
    pub(crate) notify_window: Option<std::time::Duration>,
    /// The notifications of the host have buttons.
    pub(crate) actions: bool,
    /// How long the execution stays `idle` before a turn counts as ended. `None`: three seconds.
    pub(crate) turn_settle: Option<std::time::Duration>,
    /// The sender of the push to a phone. `None`: the core sends no push.
    pub(crate) push: Option<Arc<dyn crate::phones::PushOut>>,
}

static NEXT_CONFIG: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub(crate) struct Fixture {
    pub(crate) hosts: Arc<AdapterHosts>,
    pub(crate) thread: ActorThread,
    pub(crate) hub: Arc<ProjectionHub>,
    pub(crate) events: broadcast::Receiver<Event>,
    pub(crate) health: Arc<Health>,
    pub(crate) pane: PaneId,
    pub(crate) gates: Arc<crate::interaction::Gates>,
    /// The host of the core: it writes down what the core asks of it.
    pub(crate) host: Arc<crate::host::RecordingHost>,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        Self::with_hosts(AdapterHosts::new(tokio::runtime::Handle::current()))
    }

    pub(crate) fn with_hosts(hosts: Arc<AdapterHosts>) -> Self {
        Self::with_gate_timeout(hosts, None)
    }

    /// A fixture whose gates time out after `timeout`, for the tests of the deadline.
    pub(crate) fn with_gate_timeout(
        hosts: Arc<AdapterHosts>,
        timeout: Option<std::time::Duration>,
    ) -> Self {
        Self::build(
            hosts,
            Options {
                gate_timeout: timeout,
                ..Options::default()
            },
        )
    }

    /// A fixture with a file `config.toml` and a debounce of the focus, for the tests of the
    /// gate modes. Without a debounce the focus counts at once.
    pub(crate) fn with_options(options: Options) -> Self {
        Self::build(
            AdapterHosts::new(tokio::runtime::Handle::current()),
            options,
        )
    }

    fn build(hosts: Arc<AdapterHosts>, options: Options) -> Self {
        let gates =
            crate::interaction::Gates::new(tokio::runtime::Handle::current(), options.gate_timeout);
        let hub = ProjectionHub::new();
        let (events_tx, events) = broadcast::channel(256);
        let health = Health::new(Arc::clone(&hub));
        let jobs = Jobs::new(Arc::clone(&hub));
        let mut layout = Layout::default();
        let created = layout
            .create_workspace(
                WorkspaceCreate {
                    name: Some("Work".to_owned()),
                    root: None,
                    group: None,
                    machine: None,
                    command: None,
                },
                &mut Counters::new(),
            )
            .expect("a workspace");
        let mut store = Store::open_in_memory().expect("a store");
        // The panes are rows of the database, and the rows of the runs point to them.
        store
            .tx(1, |tx| {
                crate::layout::persist::persist(tx, &Layout::default(), &layout)?;
                // The numbers of the handles that the layout used are not used again.
                tx.raise_handle_counter("workspace", 1)?;
                tx.raise_handle_counter("render", 1)?;
                for workspace in &layout.workspaces {
                    tx.raise_handle_counter(&format!("pane:{}", workspace.id), 1)?;
                    tx.raise_handle_counter(&format!("tab:{}", workspace.id), 1)?;
                }
                Ok(())
            })
            .expect("a layout");
        let harnesses = Harnesses::load(
            std::env::temp_dir().join(format!("nh-agents-none-{}", std::process::id())),
            None,
            Arc::clone(&health),
        );
        let config_path = std::env::temp_dir().join(format!(
            "nh-agents-test-{}-{}.toml",
            std::process::id(),
            NEXT_CONFIG.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        if let Some(text) = options.config {
            std::fs::write(&config_path, text).expect("a config file");
        }
        let clock = options
            .focus_debounce
            .map(|debounce| FocusClock::new(tokio::runtime::Handle::current(), debounce));
        let host = Arc::new(crate::host::RecordingHost::new(
            std::path::PathBuf::from("/bin/nh"),
            options.actions,
        ));
        let native_clock = crate::native::NativeClock::new(tokio::runtime::Handle::current());
        let mut state = State::new(
            "test",
            1_000,
            Arc::new(crate::config::ConfigStore::load(config_path)),
            jobs,
            Arc::clone(&health),
        )
        .with_layout(layout)
        .with_harnesses(harnesses)
        .with_gates(Arc::clone(&gates))
        .with_host(Arc::clone(&host) as Arc<dyn crate::host::Host>)
        .with_native_clock(
            Arc::clone(&native_clock),
            options.notify_window.unwrap_or(crate::native::COALESCE),
        )
        .with_turns(
            Arc::default(),
            options.turn_settle.unwrap_or(super::turn::SETTLE),
        )
        .with_adapters(Arc::clone(&hosts));
        if let Some(clock) = &clock {
            state = state.with_focus_clock(Arc::clone(clock));
        }
        if let Some(out) = options.push {
            let slot = Arc::new(crate::phones::PushSlot::default());
            slot.set(out);
            state = state.with_push(slot);
        }
        let thread = actor::spawn(ActorConfig {
            store,
            state,
            handlers: actor::default_handlers(),
            projections: Arc::clone(&hub),
            events: events_tx,
            health: Arc::clone(&health),
        })
        .expect("the actor starts");
        hosts.attach(thread.handle());
        gates.bind(thread.handle(), Arc::clone(&hosts));
        native_clock.bind(thread.handle());
        if let Some(clock) = &clock {
            clock.bind(thread.handle());
        }
        Self {
            hosts,
            thread,
            hub,
            events,
            health,
            pane: created.value.pane,
            gates,
            host,
        }
    }

    pub(crate) async fn call(&self, method: &str, params: Value) -> Value {
        let call = nh_proto::api::methods::Call::parse(method, params).expect("a call");
        self.thread
            .handle()
            .call(call, crate::actor::Origin::ui())
            .await
            .expect("the call works")
    }

    pub(crate) async fn open_window(&self) -> WindowId {
        let opened = self.call("window.open", serde_json::json!({})).await;
        opened["window"]
            .as_str()
            .expect("an id")
            .parse()
            .expect("a window id")
    }

    /// The window has the focus and the pane is its active element.
    pub(crate) async fn look(&self, window: WindowId) {
        let pane = self.pane;
        self.thread
            .handle()
            .update(move |parts| {
                let facts = crate::layout::model::WindowFacts {
                    focused: true,
                    visible: true,
                    active_pane: Some(pane),
                    ..Default::default()
                };
                crate::actor::layout::apply_window_report(parts, window, facts);
            })
            .await
            .expect("the actor answers");
    }

    pub(crate) async fn update<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut crate::actor::Parts<'_>) -> T + Send + 'static,
    ) -> T {
        self.thread
            .handle()
            .update(move |mut parts| f(&mut parts))
            .await
            .expect("the actor answers")
    }

    pub(crate) fn item(&self, projection: Projection, key: &str) -> Option<Value> {
        self.hub.flush();
        self.hub.snapshot(projection).items.get(key).cloned()
    }

    pub(crate) fn kinds(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            out.push(event.kind.clone());
        }
        out
    }
}

fn detection(manifest: &str, harness: Harness) -> Detection {
    Detection {
        manifest: manifest.to_owned(),
        harness,
        priority: 100,
        remote_loop: None,
        pid: 4242,
        start: 1_760_000_000_000,
        place: 0,
    }
}

async fn start_claude(f: &Fixture) -> RunId {
    let pane = f.pane;
    f.update(move |parts| {
        runs::start(parts, pane, &detection("claude", Harness::Claude), false).expect("a run")
    })
    .await
}

#[tokio::test]
async fn a_process_starts_a_run_that_is_suggested_and_cannot_be_answered() {
    let mut f = Fixture::new();
    let run = start_claude(&f).await;
    let value = f
        .item(Projection::Agents, &run.to_string())
        .expect("a row in agents");
    assert_eq!(value["harness"], "claude");
    assert_eq!(value["binding"]["state"], "suggested");
    assert_eq!(value["facets"]["liveness"]["value"]["state"], "alive");
    assert_eq!(value["facets"]["execution"]["value"]["state"], "starting");
    assert_eq!(
        value["facets"]["execution"]["provenance"]["source"],
        "process"
    );
    assert_eq!(value["pane"], f.pane.to_string());
    let caps = &value["capabilities"];
    assert_eq!(caps["observe"], true);
    assert_eq!(caps["gate"], false);
    assert_eq!(caps["answer_keystroke"], false);
    assert_eq!(caps["answer_native"], serde_json::json!([]));
    // The pane shows its run, and the event is in the log.
    let pane = f
        .item(Projection::Panes, &f.pane.to_string())
        .expect("a pane row");
    assert_eq!(pane["run"], run.to_string());
    assert!(f.kinds().contains(&"agent.detected".to_owned()));
    // The row is in the table.
    let id = run.ulid();
    let stored = f
        .thread
        .handle()
        .store(move |store| store.run(id))
        .await
        .expect("a read")
        .expect("a row");
    assert_eq!(stored.binding, "suggested");
    assert_eq!(stored.process, Some((4242, 1_760_000_000_000)));
    assert!(stored.ended_at.is_none());
    assert!(f.item(Projection::Attention, &run.to_string()).is_none());
}

#[tokio::test]
async fn a_run_in_a_pane_that_is_not_in_the_layout_does_not_start() {
    let f = Fixture::new();
    let stranger = PaneId::new(ulid::Ulid::from(99));
    let started = f
        .update(move |parts| {
            runs::start(
                parts,
                stranger,
                &detection("claude", Harness::Claude),
                false,
            )
        })
        .await;
    assert!(started.is_none());
}

#[tokio::test]
async fn a_recorded_version_gives_the_capabilities_of_its_range_and_a_new_version_gives_observe() {
    let mut f = Fixture::new();
    let run = start_claude(&f).await;
    f.update(move |parts| runs::set_version(parts, run, Ok(Version::new(2, 1, 292))))
        .await;
    let value = f.item(Projection::Agents, &run.to_string()).expect("a row");
    assert_eq!(value["harness_version"], "2.1.292");
    assert_eq!(
        value["facets"]["adapter_health"]["value"]["state"],
        "healthy"
    );
    // The run is only suggested, so the range gives it resume and survival, and no answer.
    assert_eq!(value["capabilities"]["resume"], true);
    assert_eq!(value["capabilities"]["survive_disconnect"], true);
    assert_eq!(value["capabilities"]["gate"], false);
    assert_eq!(value["capabilities"]["answer_keystroke"], false);
    assert!(f.health.value().unvalidated_harnesses.is_empty());
    f.kinds();

    // The same program after an update: the recordings do not cover the version.
    f.update(move |parts| runs::set_version(parts, run, Ok(Version::new(2, 2, 0))))
        .await;
    let value = f.item(Projection::Agents, &run.to_string()).expect("a row");
    assert_eq!(
        value["facets"]["adapter_health"]["value"]["state"],
        "unvalidated_version"
    );
    assert_eq!(value["capabilities"]["resume"], false);
    assert_eq!(value["capabilities"]["observe"], true);
    let kinds = f.kinds();
    assert!(kinds.contains(&"agent.harness_version_unvalidated".to_owned()));
    assert!(kinds.contains(&"agent.state_changed".to_owned()));
    let unvalidated = f.health.value().unvalidated_harnesses;
    assert_eq!(unvalidated.len(), 1);
    assert_eq!(unvalidated[0].harness, Harness::Claude);
    assert_eq!(unvalidated[0].version, "2.2.0");
}

#[tokio::test]
async fn a_version_that_cannot_be_found_is_unvalidated_and_makes_no_event_with_a_version() {
    let mut f = Fixture::new();
    let run = start_claude(&f).await;
    f.kinds();
    f.update(move |parts| runs::set_version(parts, run, Err("not in the PATH".to_owned())))
        .await;
    let value = f.item(Projection::Agents, &run.to_string()).expect("a row");
    assert_eq!(
        value["facets"]["adapter_health"]["value"]["state"],
        "unvalidated_version"
    );
    assert!(value.get("harness_version").is_none() || value["harness_version"].is_null());
    assert!(
        !f.kinds()
            .contains(&"agent.harness_version_unvalidated".to_owned())
    );
    assert!(
        f.health.value().unvalidated_harnesses.is_empty(),
        "no version to list"
    );
}

#[tokio::test]
async fn the_end_of_the_process_ends_the_run_and_a_new_run_takes_its_place_in_the_projection() {
    let mut f = Fixture::new();
    let run = start_claude(&f).await;
    f.kinds();
    f.update(move |parts| runs::end(parts, run, Source::Holder, Some(0)))
        .await;
    let value = f.item(Projection::Agents, &run.to_string()).expect("kept");
    assert_eq!(value["facets"]["liveness"]["value"]["state"], "exited");
    assert_eq!(value["facets"]["liveness"]["value"]["code"], 0);
    assert_eq!(value["facets"]["execution"]["value"]["state"], "exited");
    let pane = f
        .item(Projection::Panes, &f.pane.to_string())
        .expect("a pane");
    assert!(pane.get("run").is_none() || pane["run"].is_null());
    assert!(f.kinds().contains(&"agent.state_changed".to_owned()));
    // The table has the end.
    let id = run.ulid();
    let stored = f
        .thread
        .handle()
        .store(move |store| store.run(id))
        .await
        .expect("a read")
        .expect("a row");
    assert!(stored.ended_at.is_some());
    // Ending again changes nothing.
    f.update(move |parts| runs::end(parts, run, Source::Process, None))
        .await;
    assert!(f.kinds().is_empty());

    // Another harness in the same pane: the old run leaves the projections, the new one shows.
    let pane = f.pane;
    let second = f
        .update(move |parts| {
            runs::start(parts, pane, &detection("codex", Harness::Codex), true).expect("a run")
        })
        .await;
    assert!(f.item(Projection::Agents, &run.to_string()).is_none());
    let value = f
        .item(Projection::Agents, &second.to_string())
        .expect("a row");
    assert_eq!(value["harness"], "codex");
    assert_eq!(value["yolo"], true);
}

#[tokio::test]
async fn another_process_replaces_the_run_of_a_pane() {
    let f = Fixture::new();
    let first = start_claude(&f).await;
    let pane = f.pane;
    let second = f
        .update(move |parts| {
            runs::replace(parts, pane, &detection("codex", Harness::Codex), false).expect("a run")
        })
        .await;
    assert_ne!(first, second);
    // The old run ended in the table, and only the new one is in the projection.
    let id = first.ulid();
    let stored = f
        .thread
        .handle()
        .store(move |store| store.run(id))
        .await
        .expect("a read")
        .expect("a row");
    assert!(stored.ended_at.is_some());
    assert!(
        f.item(Projection::Agents, &first.to_string())
            .is_some_and(|v| { v["facets"]["liveness"]["value"]["state"] == "exited" })
            || f.item(Projection::Agents, &first.to_string()).is_none()
    );
    let pane_row = f
        .item(Projection::Panes, &f.pane.to_string())
        .expect("a pane");
    assert_eq!(pane_row["run"], second.to_string());
}

#[tokio::test]
async fn a_run_in_error_needs_attention_until_it_ends() {
    let f = Fixture::new();
    let run = start_claude(&f).await;
    let now = Timestamp(now_ms());
    f.update(move |parts| {
        runs::input(
            parts,
            run,
            Input::new(
                Source::Structured,
                now,
                Observation::Execution(Execution::Error),
            ),
        );
    })
    .await;
    let waiting = f
        .item(Projection::Attention, &run.to_string())
        .expect("a row in attention");
    assert_eq!(waiting["reasons"], serde_json::json!(["error"]));
    assert_eq!(waiting["urgency"], "normal");
    f.hub.flush();
    assert_eq!(
        f.hub.snapshot(Projection::Attention).order,
        vec![run.to_string()]
    );
    // The process ends: the run stops counting.
    f.update(move |parts| runs::end(parts, run, Source::Holder, Some(1)))
        .await;
    assert!(f.item(Projection::Attention, &run.to_string()).is_none());
}

#[tokio::test]
async fn attention_has_the_more_urgent_run_first() {
    let f = Fixture::new();
    let first = start_claude(&f).await;
    // A second pane for a second run.
    let other = f
        .update(|parts| {
            let mut counters = Counters::new();
            let before = parts.state.layout.clone();
            let created = parts
                .state
                .layout
                .create_workspace(
                    WorkspaceCreate {
                        name: Some("Other".to_owned()),
                        root: None,
                        group: None,
                        machine: None,
                        command: None,
                    },
                    &mut counters,
                )
                .expect("a workspace");
            let after = parts.state.layout.clone();
            parts
                .commit(|tx| crate::layout::persist::persist(tx, &before, &after))
                .expect("a layout");
            created.value.pane
        })
        .await;
    let second = f
        .update(move |parts| {
            runs::start(parts, other, &detection("codex", Harness::Codex), false).expect("a run")
        })
        .await;
    let now = Timestamp(now_ms());
    f.update(move |parts| {
        // The first run is rate limited (low), the second is in error (normal).
        runs::input(
            parts,
            first,
            Input::new(
                Source::Structured,
                now,
                Observation::Execution(Execution::RateLimited { resets_at: None }),
            ),
        );
        runs::input(
            parts,
            second,
            Input::new(
                Source::Structured,
                now,
                Observation::Execution(Execution::Error),
            ),
        );
    })
    .await;
    f.hub.flush();
    assert_eq!(
        f.hub.snapshot(Projection::Attention).order,
        vec![second.to_string(), first.to_string()]
    );
}

#[tokio::test]
async fn a_run_settles_to_unknown_when_nobody_speaks() {
    let f = Fixture::new();
    let run = start_claude(&f).await;
    // Too early: nothing changes.
    f.update(move |parts| runs::settle(parts, run)).await;
    let value = f.item(Projection::Agents, &run.to_string()).expect("a row");
    assert_eq!(value["facets"]["execution"]["value"]["state"], "starting");
    // Later.
    f.update(move |parts| {
        if let Some(run) = parts.state.runs.by_id.get_mut(&run) {
            run.started_at = Timestamp(now_ms() - runs::SETTLE_AFTER_MS - 1_000);
        }
        runs::settle(parts, run);
    })
    .await;
    let value = f.item(Projection::Agents, &run.to_string()).expect("a row");
    assert_eq!(value["facets"]["execution"]["value"]["state"], "unknown");
}

#[tokio::test]
async fn the_open_runs_of_the_last_core_continue_and_a_run_without_a_pane_ends() {
    let f = Fixture::new();
    let run = start_claude(&f).await;
    let facets = serde_json::to_value(
        nh_agents::arbiter::Arbiter::new(Timestamp(1), Source::Process)
            .facets(nh_proto::api::model::ReadState::Seen),
    )
    .expect("facets");
    let row = |n: u128, facets: Value| nh_store::RunRow {
        id: ulid::Ulid::from(n),
        pane: None,
        harness: "codex".to_owned(),
        manifest: "codex".to_owned(),
        name: None,
        session_id: None,
        previous_session_ids: Vec::new(),
        transcript_path: None,
        binding: "suggested".to_owned(),
        binding_confidence: 0.9,
        launch_mode: "tui".to_owned(),
        version: None,
        capabilities: serde_json::to_value(super::runs::observe_only()).expect("capabilities"),
        facets,
        yolo: false,
        resume: None,
        model: None,
        task: None,
        process: None,
        started_at: 1,
        ended_at: None,
    };
    // One run without a pane, and one whose facets this version cannot read.
    let (orphan, unreadable) = (row(5, facets), row(6, serde_json::json!({})));
    let pane = f.pane;
    let (loaded, open_after) = f
        .thread
        .handle()
        .store(move |store| {
            store.tx(1, |tx| {
                tx.put_run(&orphan)?;
                tx.put_run(&unreadable)
            })?;
            // The runs that the next core reads: the layout still has the first pane.
            let runs = Runs::load(store, &|p| p == pane)?;
            Ok((runs, store.open_runs()?))
        })
        .await
        .expect("a load");
    let first = loaded.of_pane(f.pane).expect("the run continues");
    assert_eq!(first.id, run);
    assert_eq!(first.process, Some((4242, 1_760_000_000_000)));
    assert_eq!(
        first.binding,
        nh_proto::api::model::Binding::Suggested { confidence: 0.9 }
    );
    assert!(first.live());
    assert!(matches!(first.arbiter.liveness().value, Liveness::Alive));
    assert_eq!(first.arbiter.health().value, AdapterHealth::Healthy);
    // The run without a pane ended at the load. The row that cannot be read ended in the table.
    let orphan = loaded
        .by_id
        .get(&RunId::new(ulid::Ulid::from(5)))
        .expect("kept in memory");
    assert!(!orphan.live());
    assert!(!loaded.by_id.contains_key(&RunId::new(ulid::Ulid::from(6))));
    assert_eq!(
        open_after.iter().map(|r| r.id).collect::<Vec<_>>(),
        [run.ulid()],
        "only the run with a pane stays open in the table"
    );
}

fn read_state(item: Option<Value>) -> String {
    item.expect("a row")["facets"]["read_state"]
        .as_str()
        .expect("a read state")
        .to_owned()
}

#[tokio::test]
async fn a_window_that_opens_later_gets_the_runs_and_has_seen_them() {
    let f = Fixture::new();
    let run = start_claude(&f).await;
    let window = f.open_window().await;
    let item = f.item(Projection::AgentsFor(window), &run.to_string());
    assert_eq!(read_state(item), "seen");
    f.hub.flush();
    assert_eq!(
        f.hub.snapshot(Projection::AgentsFor(window)).order,
        vec![run.to_string()]
    );
    // When the window closes, its projection goes.
    f.call("window.close", serde_json::json!({ "window": window }))
        .await;
    f.hub.flush();
    assert!(
        f.hub
            .snapshot(Projection::AgentsFor(window))
            .items
            .is_empty()
    );
}

#[tokio::test]
async fn a_run_moves_on_the_board_and_a_new_run_of_its_pane_takes_its_place() {
    let f = Fixture::new();
    let window = f.open_window().await;
    let mut panes = vec![f.pane];
    for _ in 0..2 {
        let made = f
            .call(
                "pane.split",
                serde_json::json!({"pane": f.pane.to_string(), "direction": "right"}),
            )
            .await;
        panes.push(
            made["pane"]
                .as_str()
                .expect("an id")
                .parse()
                .expect("a pane id"),
        );
    }
    let mut ids = Vec::new();
    for pane in panes.clone() {
        let detected = detection("claude", Harness::Claude);
        ids.push(
            f.update(move |parts| runs::start(parts, pane, &detected, false).expect("a run"))
                .await,
        );
    }
    let order = |window: WindowId| {
        f.hub.flush();
        f.hub.snapshot(Projection::AgentsFor(window)).order.clone()
    };
    let keys = |runs: &[RunId]| runs.iter().map(ToString::to_string).collect::<Vec<_>>();
    assert_eq!(order(window), keys(&ids), "the order in which they started");

    // `agent.move` runs this on the actor.
    let r#move = |agent: RunId, index: usize| {
        f.update(move |parts| runs::move_on_board(parts, agent, index))
    };
    r#move(ids[2], 0).await;
    assert_eq!(order(window), keys(&[ids[2], ids[0], ids[1]]));
    // A place past the end is the end.
    r#move(ids[2], 9).await;
    assert_eq!(order(window), keys(&ids));
    r#move(ids[0], 1).await;
    assert_eq!(order(window), keys(&[ids[1], ids[0], ids[2]]));

    // The run of the middle ends, and the next run of its pane takes its place.
    let ended = ids[0];
    f.update(move |parts| runs::end(parts, ended, Source::Holder, Some(0)))
        .await;
    let pane = panes[0];
    let detected = detection("codex", Harness::Codex);
    let next = f
        .update(move |parts| runs::start(parts, pane, &detected, false).expect("a run"))
        .await;
    assert_eq!(order(window), keys(&[ids[1], next, ids[2]]));

    // A window that opens later and the next core have the same order.
    let later = f.open_window().await;
    assert_eq!(order(later), keys(&[ids[1], next, ids[2]]));
    let open = f
        .thread
        .handle()
        .store(|store| store.open_runs())
        .await
        .expect("a read");
    assert_eq!(
        open.iter()
            .map(|row| RunId::new(row.id))
            .collect::<Vec<_>>(),
        [ids[1], next, ids[2]]
    );
}

#[tokio::test]
async fn each_window_has_its_own_read_state_of_a_change() {
    let f = Fixture::new();
    let run = start_claude(&f).await;
    let (first, second) = (f.open_window().await, f.open_window().await);
    let at = Timestamp(now_ms());
    f.update(move |parts| {
        runs::input(
            parts,
            run,
            Input::new(
                Source::Structured,
                at,
                Observation::Execution(Execution::Working),
            ),
        );
    })
    .await;
    // Nobody looked at the pane when the state changed.
    let key = run.to_string();
    assert_eq!(
        read_state(f.item(Projection::AgentsFor(first), &key)),
        "unseen"
    );
    assert_eq!(
        read_state(f.item(Projection::AgentsFor(second), &key)),
        "unseen"
    );
    assert_eq!(read_state(f.item(Projection::Agents, &key)), "unseen");
    // The first window looks at the pane. It has seen the run, and the other has not.
    f.look(first).await;
    assert_eq!(
        read_state(f.item(Projection::AgentsFor(first), &key)),
        "seen"
    );
    assert_eq!(
        read_state(f.item(Projection::AgentsFor(second), &key)),
        "unseen"
    );
    assert_eq!(read_state(f.item(Projection::Agents, &key)), "seen");
    // The next change happens while the first window looks: it sees it, and the other does not.
    let later = Timestamp(now_ms() + 10);
    f.update(move |parts| {
        runs::input(
            parts,
            run,
            Input::new(
                Source::Structured,
                later,
                Observation::Execution(Execution::Idle),
            ),
        );
    })
    .await;
    assert_eq!(
        read_state(f.item(Projection::AgentsFor(first), &key)),
        "seen"
    );
    assert_eq!(
        read_state(f.item(Projection::AgentsFor(second), &key)),
        "unseen"
    );
}

#[tokio::test]
async fn a_run_that_starts_after_a_window_is_seen_by_it_until_it_changes() {
    let f = Fixture::new();
    let window = f.open_window().await;
    let run = start_claude(&f).await;
    let key = run.to_string();
    assert_eq!(
        read_state(f.item(Projection::AgentsFor(window), &key)),
        "seen"
    );
    let at = Timestamp(now_ms() + 5);
    f.update(move |parts| {
        runs::input(
            parts,
            run,
            Input::new(
                Source::Structured,
                at,
                Observation::Execution(Execution::Idle),
            ),
        );
    })
    .await;
    assert_eq!(
        read_state(f.item(Projection::AgentsFor(window), &key)),
        "unseen"
    );
}

// --- hooks: the signal path ---------------------------------------------------------------------

/// What a hook of Claude Code carries, as the golden recordings show it.
pub(crate) fn hook(session: &str, extra: Value) -> Value {
    let mut payload = serde_json::json!({
        "session_id": session,
        "transcript_path": format!("/t/{session}.jsonl"),
        "cwd": "/work",
        "model": "claude-opus-5-5",
    });
    if let (Some(base), Some(more)) = (payload.as_object_mut(), extra.as_object()) {
        base.extend(more.clone());
    }
    payload
}

impl Fixture {
    /// Sends a hook the way the API does, for the pane of the fixture.
    pub(crate) async fn hook(&self, harness: Harness, event: &str, payload: Value) {
        let (pane, hosts, event) = (self.pane, Arc::clone(&self.hosts), event.to_owned());
        self.update(move |parts| {
            runs::signal(
                parts,
                &hosts,
                pane,
                harness,
                event,
                payload,
                Timestamp(now_ms()),
            );
        })
        .await;
    }

    /// Waits until the row of the run in `agents` has what the test wants. The adapter runs in a
    /// task, so the state follows a moment after the hook.
    pub(crate) async fn eventually(&self, run: RunId, want: impl Fn(&Value) -> bool) -> Value {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let item = self.item(Projection::Agents, &run.to_string());
            if let Some(item) = item.filter(|item| want(item)) {
                return item;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the run never got there"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    pub(crate) fn the_run(&self) -> RunId {
        self.hub.flush();
        let snapshot = self.hub.snapshot(Projection::Agents);
        assert_eq!(snapshot.items.len(), 1, "{:?}", snapshot.items.keys());
        snapshot
            .items
            .keys()
            .next()
            .and_then(|k| k.parse().ok())
            .expect("a run id")
    }
}

pub(crate) fn execution(item: &Value) -> &str {
    item["facets"]["execution"]["value"]["state"]
        .as_str()
        .unwrap_or("")
}

#[tokio::test]
async fn the_first_hook_of_a_pane_makes_a_bound_run_that_shows_the_states_of_the_session() {
    let mut f = Fixture::new();
    f.hook(
        Harness::Claude,
        "SessionStart",
        hook("s1", serde_json::json!({"source": "startup"})),
    )
    .await;
    let run = loop {
        f.hub.flush();
        if let Some(key) = f.hub.snapshot(Projection::Agents).items.keys().next() {
            break key.parse::<RunId>().expect("a run id");
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };
    let item = f.eventually(run, |item| execution(item) == "idle").await;
    assert_eq!(item["harness"], "claude");
    assert_eq!(item["binding"]["state"], "bound");
    assert_eq!(item["session_id"], "s1");
    assert_eq!(item["model"], "claude-opus-5-5");
    assert_eq!(
        item["facets"]["execution"]["provenance"]["source"],
        "structured"
    );
    let kinds = f.kinds();
    assert!(kinds.contains(&"agent.detected".to_owned()), "{kinds:?}");
    assert!(kinds.contains(&"agent.identified".to_owned()), "{kinds:?}");

    // A prompt makes it work, and `Stop` makes it idle again.
    f.hook(
        Harness::Claude,
        "UserPromptSubmit",
        hook("s1", serde_json::json!({"prompt": "make a note"})),
    )
    .await;
    f.eventually(run, |item| execution(item) == "working").await;
    f.hook(Harness::Claude, "Stop", hook("s1", Value::Null))
        .await;
    f.eventually(run, |item| execution(item) == "idle").await;
    // The run is the same one all along.
    assert_eq!(f.the_run(), run);

    // The detection said `hook`.
    let stored = f
        .thread
        .handle()
        .store(move |store| store.run(run.ulid()))
        .await
        .expect("a read")
        .expect("a row");
    assert_eq!(stored.binding, "bound");
    assert_eq!(stored.session_id.as_deref(), Some("s1"));
    assert_eq!(stored.transcript_path.as_deref(), Some("/t/s1.jsonl"));
    assert_eq!(stored.model.as_deref(), Some("claude-opus-5-5"));
}

#[tokio::test]
async fn a_new_session_id_in_the_same_run_identifies_again_and_clear_keeps_the_run() {
    let mut f = Fixture::new();
    f.hook(
        Harness::Claude,
        "SessionStart",
        hook("s1", serde_json::json!({"source": "startup"})),
    )
    .await;
    let run = loop {
        f.hub.flush();
        if let Some(key) = f.hub.snapshot(Projection::Agents).items.keys().next() {
            break key.parse::<RunId>().expect("a run id");
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };
    f.eventually(run, |item| item["session_id"] == "s1").await;
    f.kinds();
    // `/clear`: the session ends with the reason `clear`, and a new one starts. The run stays.
    f.hook(
        Harness::Claude,
        "SessionEnd",
        hook("s1", serde_json::json!({"reason": "clear"})),
    )
    .await;
    f.hook(
        Harness::Claude,
        "SessionStart",
        hook("s2", serde_json::json!({"source": "clear"})),
    )
    .await;
    let item = f.eventually(run, |item| item["session_id"] == "s2").await;
    assert_eq!(execution(&item), "idle");
    assert_eq!(f.the_run(), run);
    let identified = f
        .kinds()
        .iter()
        .filter(|kind| *kind == "agent.identified")
        .count();
    assert_eq!(identified, 1, "one more identification");
    // A real end of the session ends the execution state, and the process lives.
    f.hook(
        Harness::Claude,
        "SessionEnd",
        hook("s2", serde_json::json!({"reason": "logout"})),
    )
    .await;
    let item = f.eventually(run, |item| execution(item) == "exited").await;
    assert_eq!(item["facets"]["liveness"]["value"]["state"], "alive");
}

#[tokio::test]
async fn a_hook_binds_a_run_that_the_process_tree_found_and_the_version_gives_it_a_gate() {
    let f = Fixture::new();
    let run = start_claude(&f).await;
    let item = f.item(Projection::Agents, &run.to_string()).expect("a row");
    assert_eq!(item["binding"]["state"], "suggested");
    f.update(move |parts| runs::set_version(parts, run, Ok(Version::new(2, 1, 292))))
        .await;
    let item = f.item(Projection::Agents, &run.to_string()).expect("a row");
    assert_eq!(
        item["capabilities"]["gate"], false,
        "a suggested run has no gate"
    );
    // The hook carries the token of the pane: the run is bound, and the hooks are a transport.
    f.hook(Harness::Claude, "SessionStart", hook("s1", Value::Null))
        .await;
    let item = f
        .eventually(run, |item| item["binding"]["state"] == "bound")
        .await;
    assert_eq!(f.the_run(), run, "no second run");
    assert_eq!(item["capabilities"]["gate"], true);
    assert_eq!(item["capabilities"]["answer_keystroke"], true);
    assert_eq!(
        item["capabilities"]["answer_native"],
        serde_json::json!(["approval", "plan_review"])
    );
}

#[tokio::test]
async fn the_process_tree_takes_over_the_run_that_a_hook_made() {
    let f = Fixture::new();
    f.hook(Harness::Claude, "SessionStart", hook("s1", Value::Null))
        .await;
    let run = loop {
        f.hub.flush();
        if let Some(key) = f.hub.snapshot(Projection::Agents).items.keys().next() {
            break key.parse::<RunId>().expect("a run id");
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };
    // The watcher finds the program a moment later. It is the process of the same run.
    let pane = f.pane;
    let adopted = f
        .update(move |parts| runs::start(parts, pane, &detection("claude", Harness::Claude), true))
        .await;
    assert_eq!(adopted, Some(run));
    assert_eq!(f.the_run(), run);
    let item = f.item(Projection::Agents, &run.to_string()).expect("a row");
    assert_eq!(item["binding"]["state"], "bound", "the binding stays");
    assert_eq!(item["yolo"], true);
    let id = run.ulid();
    let stored = f
        .thread
        .handle()
        .store(move |store| store.run(id))
        .await
        .expect("a read")
        .expect("a row");
    assert_eq!(stored.process, Some((4242, 1_760_000_000_000)));
}

#[tokio::test]
async fn a_hook_of_another_harness_replaces_the_run_of_the_pane() {
    let f = Fixture::new();
    f.hook(Harness::Claude, "SessionStart", hook("s1", Value::Null))
        .await;
    let first = loop {
        f.hub.flush();
        if let Some(key) = f.hub.snapshot(Projection::Agents).items.keys().next() {
            break key.parse::<RunId>().expect("a run id");
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };
    f.hook(Harness::Codex, "SessionStart", hook("c1", Value::Null))
        .await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let second = loop {
        f.hub.flush();
        let found = f
            .hub
            .snapshot(Projection::Agents)
            .items
            .iter()
            .find(|(_, item)| item["harness"] == "codex")
            .map(|(key, _)| key.parse::<RunId>().expect("a run id"));
        if let Some(found) = found {
            break found;
        }
        assert!(std::time::Instant::now() < deadline);
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };
    assert_ne!(first, second);
    let ended = f.item(Projection::Agents, &first.to_string());
    assert!(
        ended.is_none(),
        "the old run left the projection: {ended:?}"
    );
}

#[tokio::test]
async fn the_end_of_a_run_stops_its_adapter() {
    let f = Fixture::new();
    f.hook(Harness::Claude, "SessionStart", hook("s1", Value::Null))
        .await;
    let run = loop {
        f.hub.flush();
        if let Some(key) = f.hub.snapshot(Projection::Agents).items.keys().next() {
            break key.parse::<RunId>().expect("a run id");
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };
    f.eventually(run, |item| execution(item) == "idle").await;
    assert_eq!(f.hosts.count(), 1);
    f.update(move |parts| runs::end(parts, run, Source::Holder, Some(0)))
        .await;
    assert_eq!(f.hosts.count(), 0);
}

/// An adapter that panics when it gets a signal.
struct Boom;

#[async_trait::async_trait]
impl Adapter for Boom {
    fn capabilities(&self) -> AdapterCaps {
        AdapterCaps::default()
    }

    async fn start(&mut self, _ctx: AdapterCtx) -> nh_agents::adapter::Result<()> {
        Ok(())
    }

    async fn on_signal(
        &mut self,
        _signal: AdapterSignal,
    ) -> nh_agents::adapter::Result<SignalReply> {
        panic!("this adapter is broken");
    }
}

#[tokio::test]
async fn an_adapter_that_panics_degrades_its_run_and_the_next_signal_starts_a_new_one() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let made = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&made);
    let hosts = AdapterHosts::with_factory(
        tokio::runtime::Handle::current(),
        Box::new(move |start| {
            // The first adapter is broken, and the ones after it are right.
            if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                Box::new(Boom)
            } else {
                Box::new(nh_agents::adapter::hooks::ManifestAdapter::new(
                    &start.manifest,
                ))
            }
        }),
    );
    let f = Fixture::with_hosts(hosts);
    f.hook(Harness::Claude, "SessionStart", hook("s1", Value::Null))
        .await;
    let run = loop {
        f.hub.flush();
        if let Some(key) = f.hub.snapshot(Projection::Agents).items.keys().next() {
            break key.parse::<RunId>().expect("a run id");
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };
    let item = f
        .eventually(run, |item| {
            item["facets"]["adapter_health"]["value"]["state"] == "degraded"
        })
        .await;
    assert!(
        item["facets"]["adapter_health"]["value"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("adapter failed"))
    );
    // The task is gone. A new signal starts a new adapter, and the state follows.
    f.hook(Harness::Claude, "UserPromptSubmit", hook("s1", Value::Null))
        .await;
    f.eventually(run, |item| execution(item) == "working").await;
    assert_eq!(made.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_hook_for_a_pane_that_is_not_in_the_layout_makes_nothing() {
    let f = Fixture::new();
    let stranger = PaneId::new(ulid::Ulid::from(99));
    let hosts = Arc::clone(&f.hosts);
    f.update(move |parts| {
        runs::signal(
            parts,
            &hosts,
            stranger,
            Harness::Claude,
            "SessionStart".to_owned(),
            hook("s1", Value::Null),
            Timestamp(now_ms()),
        );
    })
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    f.hub.flush();
    assert!(f.hub.snapshot(Projection::Agents).items.is_empty());
    assert_eq!(f.hosts.count(), 0);
}

#[tokio::test]
async fn a_claude_run_without_hooks_gets_a_note_of_the_doctor_until_the_hooks_come() {
    let f = Fixture::new();
    let run = start_claude(&f).await;
    let notes = |f: &Fixture| -> Vec<String> {
        f.health
            .value()
            .warnings
            .iter()
            .filter(|w| w.id.starts_with("integration:"))
            .map(|w| w.text.clone())
            .collect()
    };
    // Too early: the run is new, and the hooks may still come.
    f.update(move |parts| runs::settle(parts, run)).await;
    assert!(notes(&f).is_empty());
    // Five seconds later, nobody has spoken.
    f.update(move |parts| {
        if let Some(run) = parts.state.runs.by_id.get_mut(&run) {
            run.started_at = Timestamp(now_ms() - runs::SETTLE_AFTER_MS - 1_000);
        }
        runs::settle(parts, run);
    })
    .await;
    let text = notes(&f);
    assert_eq!(text.len(), 1, "{text:?}");
    assert!(
        text[0].contains("w1:p1") && text[0].contains("nh integration install claude"),
        "{text:?}"
    );
    // A hook is the end of the note.
    f.hook(Harness::Claude, "SessionStart", hook("s1", Value::Null))
        .await;
    f.eventually(run, |item| item["binding"]["state"] == "bound")
        .await;
    assert!(notes(&f).is_empty());
}

/// The notes of the doctor about the hooks.
fn integration_notes(f: &Fixture) -> Vec<String> {
    f.health
        .value()
        .warnings
        .iter()
        .filter(|w| w.id.starts_with("integration:"))
        .map(|w| w.text.clone())
        .collect()
}

/// A Codex run that started some time ago, and what the screen reads for it.
async fn start_codex(f: &Fixture, age_ms: i64, shared_daemon: bool) -> RunId {
    let pane = f.pane;
    f.update(move |parts| {
        let run =
            runs::start(parts, pane, &detection("codex", Harness::Codex), false).expect("a run");
        runs::set_shared_daemon(parts, run, shared_daemon);
        if let Some(run) = parts.state.runs.by_id.get_mut(&run) {
            run.started_at = Timestamp(now_ms() - age_ms);
        }
        run
    })
    .await
}

async fn screen_reads(f: &Fixture, run: RunId, state: nh_agents::screen::ScreenState) {
    let seen = runs::ScreenSeen {
        state: Some((state, 0.8)),
        dialog: None,
    };
    f.update(move |parts| runs::screen(parts, run, &seen)).await;
}

#[tokio::test]
async fn a_codex_run_gets_no_note_before_its_first_turn_because_codex_has_no_hook_until_then() {
    let f = Fixture::new();
    let run = start_codex(&f, runs::SETTLE_AFTER_MS + 1_000, false).await;
    f.update(move |parts| runs::settle(parts, run)).await;
    assert!(integration_notes(&f).is_empty());
    // The screen of a Codex that starts can look busy for a moment: that is no turn.
    let f = Fixture::new();
    let young = start_codex(&f, 0, false).await;
    screen_reads(&f, young, nh_agents::screen::ScreenState::Working).await;
    screen_reads(&f, young, nh_agents::screen::ScreenState::Idle).await;
    f.update(move |parts| {
        if let Some(run) = parts.state.runs.by_id.get_mut(&young) {
            run.started_at = Timestamp(now_ms() - runs::SETTLE_AFTER_MS - 1_000);
        }
        runs::settle(parts, young);
    })
    .await;
    assert!(integration_notes(&f).is_empty());
}

#[tokio::test]
async fn a_codex_turn_that_ends_without_a_hook_gives_a_note_with_its_own_command() {
    use nh_agents::screen::ScreenState;
    let f = Fixture::new();
    let run = start_codex(&f, runs::SETTLE_AFTER_MS + 1_000, false).await;
    screen_reads(&f, run, ScreenState::Working).await;
    assert!(integration_notes(&f).is_empty(), "the turn is not over");
    screen_reads(&f, run, ScreenState::Idle).await;
    let notes = integration_notes(&f);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(
        notes[0].starts_with("Codex in w1:p1") && notes[0].contains("nh integration install codex"),
        "{notes:?}"
    );
    // A hook is the end of the note.
    f.hook(Harness::Codex, "UserPromptSubmit", hook("t1", Value::Null))
        .await;
    f.eventually(run, |item| item["binding"]["state"] == "bound")
        .await;
    assert!(integration_notes(&f).is_empty());
}

#[tokio::test]
async fn a_codex_turn_with_a_hook_gives_no_note() {
    use nh_agents::screen::ScreenState;
    let f = Fixture::new();
    let run = start_codex(&f, runs::SETTLE_AFTER_MS + 1_000, false).await;
    screen_reads(&f, run, ScreenState::Working).await;
    f.hook(Harness::Codex, "UserPromptSubmit", hook("t1", Value::Null))
        .await;
    f.eventually(run, |item| item["binding"]["state"] == "bound")
        .await;
    screen_reads(&f, run, ScreenState::Idle).await;
    assert!(integration_notes(&f).is_empty());
}

#[tokio::test]
async fn a_codex_without_the_flag_of_the_shim_gets_a_note_about_the_shared_daemon() {
    use nh_agents::screen::ScreenState;
    let f = Fixture::new();
    let run = start_codex(&f, runs::SETTLE_AFTER_MS + 1_000, true).await;
    screen_reads(&f, run, ScreenState::Working).await;
    screen_reads(&f, run, ScreenState::Idle).await;
    let notes = integration_notes(&f);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(
        notes[0].starts_with("Codex in w1:p1")
            && notes[0].contains("without `--no-daemon`")
            && notes[0].contains("start `codex` again"),
        "{notes:?}"
    );
}

// --- the screen ---------------------------------------------------------------------

fn dialog(
    rule: &str,
    kind: nh_proto::api::model::InteractionKind,
    command: &str,
) -> nh_agents::screen::ScreenMatch {
    use nh_agents::screen::{Dialog, DialogOption, ScreenMatch};
    let option = |n: u32, label: &str, pointed: bool| DialogOption {
        number: Some(n),
        label: label.to_owned(),
        row: n as u16,
        pointed,
        checked: None,
    };
    ScreenMatch {
        rule_id: rule.to_owned(),
        state: None,
        opens: Some(kind),
        confidence: 0.95,
        captures: [("command".to_owned(), command.to_owned())].into(),
        dialog: Some(Dialog {
            options: vec![option(1, "Yes", true), option(2, "No", false)],
            pointer: Some(0),
            accelerators: true,
            map: Default::default(),
            choices: Default::default(),
            multi_toggle: None,
            rows: (1, 2),
        }),
        keystroke_answerable: true,
    }
}

fn seen_dialog(found: nh_agents::screen::ScreenMatch) -> runs::ScreenSeen {
    runs::ScreenSeen {
        state: None,
        dialog: Some(found),
    }
}

#[tokio::test]
async fn the_state_that_the_screen_reads_goes_to_the_arbiter_and_validates_the_manifest() {
    use nh_agents::screen::ScreenState;
    let f = Fixture::new();
    let run = start_claude(&f).await;
    let seen = runs::ScreenSeen {
        state: Some((ScreenState::Working, 0.8)),
        dialog: None,
    };
    f.update(move |parts| runs::screen(parts, run, &seen)).await;
    let value = f
        .item(Projection::Agents, &run.to_string())
        .expect("a row in agents");
    assert_eq!(value["facets"]["execution"]["value"]["state"], "working");
    assert_eq!(
        value["facets"]["execution"]["provenance"]["source"],
        "screen"
    );
    let validates = f
        .update(move |parts| parts.state.runs.by_id[&run].screen_validates)
        .await;
    assert!(validates, "a rule read the live screen");
}

#[tokio::test]
async fn a_dialog_on_the_screen_opens_a_provisional_interaction_that_ends_with_the_dialog() {
    use nh_proto::api::model::InteractionKind;
    let f = Fixture::new();
    let run = start_claude(&f).await;
    let found = dialog("approval", InteractionKind::Approval, "touch note.txt");
    let seen = seen_dialog(found);
    let again = seen.clone();
    f.update(move |parts| runs::screen(parts, run, &seen)).await;
    f.hub.flush();
    let snapshot = f.hub.snapshot(Projection::Interactions);
    let items = &snapshot.items;
    assert_eq!(items.len(), 1, "one provisional interaction");
    let shown = items.values().next().expect("a row").clone();
    assert_eq!(shown["kind"], "approval");
    assert_eq!(shown["state"], "open");
    assert_eq!(shown["mode"], "observe");
    assert_eq!(shown["answerable"], false, "no keystroke verifier yet");
    let from_screen = f
        .update(|parts| {
            parts
                .state
                .interactions
                .open()
                .iter()
                .all(|i| i.source == crate::interaction::model::Source::Screen)
        })
        .await;
    assert!(from_screen, "the interaction came from the screen");
    assert_eq!(shown["title"], "Run `touch note.txt`");
    // The same dialog on the next look is the same interaction.
    f.update(move |parts| runs::screen(parts, run, &again))
        .await;
    f.hub.flush();
    assert_eq!(f.hub.snapshot(Projection::Interactions).items.len(), 1);
    // The dialog is gone: the provisional interaction ends.
    let gone = runs::ScreenSeen {
        state: None,
        dialog: None,
    };
    f.update(move |parts| runs::screen(parts, run, &gone)).await;
    f.hub.flush();
    let snapshot = f.hub.snapshot(Projection::Interactions);
    let items = &snapshot.items;
    let ended = items.values().next().expect("a row");
    assert_eq!(ended["state"], "cancelled");
}

#[tokio::test]
async fn a_dialog_that_no_rule_reads_opens_a_question_with_the_hint() {
    use nh_proto::api::model::InteractionKind;
    let f = Fixture::new();
    let run = start_claude(&f).await;
    let mut found = dialog(
        nh_agents::screen::engine::UNKNOWN_DIALOG_RULE,
        InteractionKind::Question,
        "",
    );
    found.captures.clear();
    found.captures.insert(
        "hint".to_owned(),
        nh_agents::screen::engine::UNKNOWN_DIALOG_HINT.to_owned(),
    );
    found.confidence = 0.5;
    found.keystroke_answerable = false;
    let seen = seen_dialog(found);
    f.update(move |parts| runs::screen(parts, run, &seen)).await;
    f.hub.flush();
    let snapshot = f.hub.snapshot(Projection::Interactions);
    let items = &snapshot.items;
    let shown = items.values().next().expect("a row").clone();
    assert_eq!(shown["kind"], "question");
    assert_eq!(shown["title"], "neohar can't read this dialog");
    assert_eq!(shown["answerable"], false);
    assert_eq!(
        shown["payload"]["questions"][0]["options"][0]["label"],
        "Yes"
    );
    // The heuristic does not tell that the manifest validates.
    let validates = f
        .update(move |parts| parts.state.runs.by_id[&run].screen_validates)
        .await;
    assert!(!validates);
}
