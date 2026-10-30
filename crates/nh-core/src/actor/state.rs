//! The state that the actor owns.
//!
//! The state tables in `state.db` are the source of truth. This struct is the same state in
//! memory, in the shape that the handlers read. Only the actor thread changes it, in two ways:
//! the [`crate::actor::Mutation`] of a command, which runs after the commit, and a closure of
//! [`crate::actor::ActorHandle::update`], which gets the state and the store together.

use std::sync::Arc;

use nh_proto::api::model::StorageHealth;

use crate::{
    agents::{AdapterHosts, Runs},
    config::ConfigStore,
    focus::{Focus, FocusClock},
    harness::Harnesses,
    health::Health,
    interaction::{Gates, Interactions},
    jobs::Jobs,
    keystrokes::Keystrokes,
    layout::{launch, launch::RuntimeLink, model::Layout, project::publish_palette},
    policy::Policy,
    projection::ProjectionHub,
    render::RenderServer,
    router::HostRouter,
};

#[cfg(test)]
use crate::host::Host;

/// The device of the host in the process of a test core.
#[cfg(test)]
pub const TEST_DEVICE: ulid::Ulid = ulid::Ulid(0x0100_0000_0000_0000_0000_0000_0000_0001);

/// What the core knows about its own session.
#[derive(Clone)]
pub(crate) struct State {
    /// The name of the session.
    pub session: String,
    /// The label of the machine. The local machine is `local`.
    pub machine_label: String,
    /// When the core started, in unix milliseconds.
    pub started_at_ms: i64,
    /// The configuration (`config.toml`). A handler reads it and does not hold it.
    pub config: Arc<ConfigStore>,
    /// The long commands that run. The registry has its own lock: the workers of the
    /// jobs report from other threads.
    pub jobs: Arc<Jobs>,
    /// What the user needs to know about the core itself (`health`).
    pub health: Arc<Health>,
    /// The workspaces, tabs and panes (`tree`). A command changes a copy, and the mutation of
    /// the command replaces this value after the commit.
    pub layout: Layout,
    /// How the layout reaches the pane runtime. Tests of the layout alone have none.
    pub runtime: Option<RuntimeLink>,
    /// The render server, which draws the panes that the windows of the layout name.
    pub render: Option<Arc<RenderServer>>,
    /// The host router: the host in the process and the link clients. A command that
    /// needs a window of a client asks it.
    pub host: Option<Arc<HostRouter>>,
    /// The manifests of the harnesses in force.
    pub harnesses: Option<Arc<Harnesses>>,
    /// The runs of agents.
    pub runs: Runs,
    /// The adapters of the live runs.
    pub adapters: Option<Arc<AdapterHosts>>,
    /// The interactions of the runs.
    pub interactions: Interactions,
    /// The connections of the shims that hold a hook.
    pub gates: Option<Arc<Gates>>,
    /// The panes that have the focus of the user, after the debounce.
    pub focus: Focus,
    /// The rules of the table `policy_rules`, and those of the file.
    pub policy: Policy,
    /// Types the answers into the panes.
    pub keystrokes: Option<Arc<Keystrokes>>,
    /// The notifications, the badge and the tray that the host shows.
    pub native: crate::native::Native,
    /// Keys, prompts and launches for the runs.
    pub control: Option<Arc<crate::agents::Control>>,
    /// The rate limits and the usage of today for each harness.
    pub limits: crate::transcripts::Limits,
    /// The panes whose run waits after a turn, shared with the input path of the panes.
    pub turn_waits: Arc<crate::agents::turn::TurnWaits>,
    /// The paired phones and the offer of a pairing.
    pub phones: crate::phones::Phones,
    /// Where the runs of workflow nodes send their turns and their changes.
    pub workflows: Option<tokio::sync::mpsc::UnboundedSender<crate::workflows::Signal>>,
}

impl State {
    pub fn new(
        session: &str,
        started_at_ms: i64,
        config: Arc<ConfigStore>,
        jobs: Arc<Jobs>,
        health: Arc<Health>,
    ) -> Self {
        Self {
            session: session.to_owned(),
            machine_label: "local".to_owned(),
            started_at_ms,
            config,
            jobs,
            health,
            layout: Layout::default(),
            runtime: None,
            render: None,
            host: None,
            harnesses: None,
            runs: Runs::new(),
            adapters: None,
            interactions: Interactions::new(),
            gates: None,
            focus: Focus::default(),
            policy: Policy::default(),
            keystrokes: None,
            native: crate::native::Native::default(),
            control: None,
            limits: crate::transcripts::Limits::default(),
            turn_waits: Arc::default(),
            phones: crate::phones::Phones::default(),
            workflows: None,
        }
    }

    /// Sends a signal to the executor of the workflows, if it runs.
    pub fn workflow_signal(&self, signal: crate::workflows::Signal) {
        if let Some(sender) = &self.workflows {
            let _ = sender.send(signal);
        }
    }

    /// The same state with the set of waiting panes that the input path of the panes reads, and
    /// with the settle time of a turn end.
    pub fn with_turns(
        mut self,
        waits: Arc<crate::agents::turn::TurnWaits>,
        settle: std::time::Duration,
    ) -> Self {
        self.turn_waits = waits;
        self.native.set_turn_settle(settle);
        self
    }

    /// The same state with the channel to the executor of the workflows.
    pub fn with_workflows(
        mut self,
        sender: tokio::sync::mpsc::UnboundedSender<crate::workflows::Signal>,
    ) -> Self {
        self.workflows = Some(sender);
        self
    }

    /// The same state with the paired phones of the store.
    pub fn with_phones(mut self, phones: crate::phones::Phones) -> Self {
        self.phones = phones;
        self
    }

    /// The same state with the sender of the push to a phone.
    pub fn with_push(mut self, slot: Arc<crate::phones::PushSlot>) -> Self {
        self.native.push = crate::native::push::Pushes::with_slot(slot);
        self
    }

    /// The same state with a clock for the notifications that wait for their window.
    pub fn with_native_clock(
        mut self,
        clock: Arc<crate::native::NativeClock>,
        window: std::time::Duration,
    ) -> Self {
        let settle = self.native.turn_settle();
        let push = std::mem::take(&mut self.native.push);
        self.native = crate::native::Native::with_clock(clock, window);
        self.native.set_turn_settle(settle);
        self.native.push = push;
        self
    }

    /// The same state with the service that types into panes and launches agents.
    pub fn with_control(mut self, control: Arc<crate::agents::Control>) -> Self {
        self.control = Some(control);
        self
    }

    /// The same state with a link to the pane runtime.
    pub fn with_runtime(mut self, runtime: RuntimeLink) -> Self {
        self.runtime = Some(runtime);
        self
    }

    /// The same state with the render server of the core.
    pub fn with_render(mut self, render: Arc<RenderServer>) -> Self {
        self.render = Some(render);
        self
    }

    /// The reason that the store is degraded, or `None` when it works (`health`).
    pub fn degraded_reason(&self) -> Option<String> {
        match self.health.value().storage {
            StorageHealth::Degraded { reason } => Some(reason),
            StorageHealth::Ok => None,
        }
    }

    /// Publishes the commands of the palette for the layout and the health of the store.
    pub fn publish_palette(&self, hub: &ProjectionHub) {
        publish_palette(hub, &self.layout, self.degraded_reason().as_deref());
    }

    /// The same state with the registry of the harness manifests.
    pub fn with_harnesses(mut self, harnesses: Arc<Harnesses>) -> Self {
        self.harnesses = Some(harnesses);
        self
    }

    /// The same state with the runs that the last core left open.
    pub fn with_runs(mut self, runs: Runs) -> Self {
        self.runs = runs;
        self
    }

    /// The same state with the interactions that the last core left open.
    pub fn with_interactions(mut self, interactions: Interactions) -> Self {
        self.interactions = interactions;
        self
    }

    /// The same state with the rules that the last core left in the table.
    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    /// The same state with the service that types the answers.
    pub fn with_keystrokes(mut self, keystrokes: Arc<Keystrokes>) -> Self {
        self.keystrokes = Some(keystrokes);
        self
    }

    /// The same state with the gates of the shims.
    pub fn with_gates(mut self, gates: Arc<Gates>) -> Self {
        self.gates = Some(gates);
        self
    }

    /// The same state with a focus whose changes wait for the debounce.
    pub fn with_focus_clock(mut self, clock: Arc<FocusClock>) -> Self {
        self.focus = Focus::with_clock(clock);
        self
    }

    /// The same state with the host of the adapters.
    pub fn with_adapters(mut self, adapters: Arc<AdapterHosts>) -> Self {
        self.adapters = Some(adapters);
        self
    }

    /// The same state with the host router of the core.
    pub fn with_router(mut self, router: Arc<HostRouter>) -> Self {
        router.set_owners(&self.layout);
        self.host = Some(router);
        self
    }

    /// The same state with a router whose only host is `host`, in the process, with the device
    /// [`TEST_DEVICE`]. For tests.
    #[cfg(test)]
    pub fn with_host(self, host: Arc<dyn Host>) -> Self {
        let sidecar = host.sidecar_path();
        let local = crate::host::InProcessHost::new(
            crate::host::ClientInfo {
                device: TEST_DEVICE,
                name: "test".to_owned(),
            },
            host,
        );
        self.with_router(Arc::new(HostRouter::new(Some(local), sidecar)))
    }

    /// The layout changed: tell the pane runtime what to start and end, and the render server
    /// what each window draws. `old` is the layout before.
    pub fn layout_changed(&self, old: &Layout) {
        // The router sends a window request to the client that owns the row.
        if let Some(router) = &self.host {
            router.set_owners(&self.layout);
        }
        if let Some(link) = &self.runtime {
            for command in launch::commands(old, &self.layout) {
                link.send(command);
            }
        }
        if let Some(render) = &self.render {
            render.sync_layout(&self.layout);
        }
        // A pane that left the window, or a window that closed, ends the focus of the pane.
        self.focus.nudge();
    }

    /// The same state with the layout that the database held.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        if let Some(router) = &self.host {
            router.set_owners(&self.layout);
        }
        self
    }
}
