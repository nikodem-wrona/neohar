//! The machines of the client: the projection `machines` and the methods `machine.*`.
//!
//! The local core owns the links. The links need `ssh` and the client of `link/1`, which live in
//! `nh-remote`, and that crate depends on this one. So the core holds a [`MachineLinks`], a trait
//! that the host in the process gives it ([`crate::CoreHandle::set_machine_links`]), and
//! `nh-remote` implements it. A core without one (`nh daemon`, or the app as a client of a daemon)
//! has the row `local` only, and `machine.connect` answers `unsupported`.
//!
//! The core owns the order and the contents of the projection. The links give the rows of the
//! remote machines. The core puts `local` first, then the order of `config.toml`. The webview
//! sorts nothing.

use std::{
    sync::{Arc, Mutex, OnceLock, Weak},
    time::Duration,
};

use async_trait::async_trait;
use nh_proto::api::{
    calls::workspace::{MachineList, MachineTarget},
    common::Empty,
    methods::{Call, Method},
    model::{MachineState, MachineValue},
    projections::Projection,
};
use serde_json::Value;

use crate::ErrorCode;
pub use crate::config::MachineConfig;
use crate::{error::RpcError, pane::lock, runtime::Inner};

/// The links to the remote machines. The host in the process implements it with `ssh`.
#[async_trait]
pub trait MachineLinks: Send + Sync + 'static {
    /// The machines of `config.toml` in the order of the file. Called once at the start and after
    /// each change of `[machines]`, with the whole list. The links start the machines with
    /// `connect = "launch"` that are not up, end the links of machines that are gone, and take a
    /// new target for a machine that changed. It must not wait.
    fn configure(&self, machines: Vec<MachineConfig>);

    /// Opens the link of a machine and waits for the result. Done at once for a machine that is up.
    async fn connect(&self, label: &str) -> Result<(), RpcError>;

    /// Opens a window on a machine for this client (`window.open` over its link), after it
    /// connects the machine. The window row names this client as its owner, so the app opens a
    /// native window for it.
    async fn open_window(&self, label: &str) -> Result<(), RpcError>;

    /// Ends the link of a machine. The programs of its panes keep running.
    fn disconnect(&self, label: &str) -> Result<(), RpcError>;

    /// The rows of the remote machines, without `local`.
    fn rows(&self) -> Vec<MachineValue>;

    /// Gives the links a function to call after each change of a row.
    fn on_change(&self, notify: Arc<dyn Fn() + Send + Sync>);
}

/// The links of a core, and the keys that the projection `machines` has now.
#[derive(Default)]
pub(crate) struct MachineSlot {
    links: OnceLock<Arc<dyn MachineLinks>>,
    published: Mutex<Vec<String>>,
}

impl MachineSlot {
    fn links(&self) -> Option<&Arc<dyn MachineLinks>> {
        self.links.get()
    }

    /// Stores the links. Only the first call takes effect.
    pub(crate) fn set(&self, links: Arc<dyn MachineLinks>) -> bool {
        self.links.set(links).is_ok()
    }
}

/// The row of this machine.
fn local_row() -> MachineValue {
    MachineValue {
        label: "local".to_owned(),
        state: MachineState::Local,
        latency_ms: None,
        version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        reason: None,
        forwards: Vec::new(),
        harnesses: Vec::new(),
        update: None,
    }
}

/// The rows in the order of the projection: `local`, then the machines of `config.toml` in the
/// order of the file. A row that the config does not know (a machine that was just removed) goes
/// last, by label.
pub(crate) fn rows(inner: &Inner) -> Vec<MachineValue> {
    let mut out = vec![local_row()];
    if let Some(links) = inner.machines.links() {
        let order: Vec<String> = inner
            .config
            .machines()
            .into_iter()
            .map(|machine| machine.label)
            .collect();
        let mut remote = links.rows();
        remote.sort_by_key(|row| {
            order
                .iter()
                .position(|label| *label == row.label)
                .unwrap_or(usize::MAX)
        });
        out.extend(remote);
    }
    out
}

/// Puts the rows in the projection `machines`, and removes the rows that are gone.
pub(crate) fn publish(inner: &Inner) {
    let rows = rows(inner);
    let hub = inner.projections.hub();
    let keys: Vec<String> = rows.iter().map(|row| row.label.clone()).collect();
    let mut published = lock(&inner.machines.published);
    for gone in published.iter().filter(|key| !keys.contains(key)) {
        hub.remove(Projection::Machines, gone);
    }
    for row in &rows {
        hub.set(
            Projection::Machines,
            &row.label,
            serde_json::to_value(row).unwrap_or(Value::Null),
        );
    }
    hub.set_order(Projection::Machines, &keys);
    *published = keys;
}

/// Connects the links to the core: they get the machines, and a change of a row or of the table
/// publishes the projection.
pub(crate) fn attach(inner: &Arc<Inner>, links: Arc<dyn MachineLinks>) {
    if !inner.machines.set(Arc::clone(&links)) {
        tracing::warn!("the core has machine links already; the second set is ignored");
        return;
    }
    let weak: Weak<Inner> = Arc::downgrade(inner);
    {
        let weak = weak.clone();
        links.on_change(Arc::new(move || {
            if let Some(inner) = weak.upgrade() {
                publish(&inner);
            }
        }));
    }
    inner.config.on_machines_changed(Arc::new(move |machines| {
        if let Some(inner) = weak.upgrade() {
            if let Some(links) = inner.machines.links() {
                links.configure(machines);
            }
            publish(&inner);
        }
    }));
    links.configure(inner.config.machines());
    publish(inner);
}

/// Whether this module runs the method.
pub(crate) fn handles(method: &str) -> bool {
    matches!(
        Method::from_name(method),
        Some(
            Method::MachineList
                | Method::MachineConnect
                | Method::MachineDisconnect
                | Method::MachineOpenWindow
        )
    )
}

/// How long `machine.connect` waits for the link: a question of ssh waits for the person, so the
/// limit is the one of the ssh start and a little more.
pub const CONNECT_WAIT: Duration = Duration::from_secs(130);

fn no_links() -> RpcError {
    RpcError::new(
        ErrorCode::Unsupported,
        "this core does not connect to machines: it is a daemon, or the app is a client of one",
    )
}

/// The links, and the label checked against the config.
fn target(inner: &Inner, label: &str) -> Result<Arc<dyn MachineLinks>, RpcError> {
    let links = inner.machines.links().ok_or_else(no_links)?;
    if label == "local" {
        return Err(RpcError::invalid_params(
            "local is this machine: there is no link to connect",
        ));
    }
    if !inner
        .config
        .machines()
        .iter()
        .any(|machine| machine.label == label)
    {
        return Err(RpcError::new(
            ErrorCode::NotFound,
            format!(
                "no machine {label:?} in the configuration: add [machines.{label}] to config.toml"
            ),
        ));
    }
    Ok(Arc::clone(links))
}

/// Runs a call that [`handles`] accepted.
pub(crate) async fn run(inner: &Arc<Inner>, call: Call) -> Result<Value, RpcError> {
    match call {
        Call::MachineList(_) => to_json(&MachineList {
            machines: rows(inner),
        }),
        Call::MachineConnect(MachineTarget { machine }) => {
            let links = target(inner, &machine)?;
            tokio::time::timeout(CONNECT_WAIT, links.connect(&machine))
                .await
                .map_err(|_| {
                    RpcError::new(ErrorCode::LinkDown, "the link did not open in time")
                })??;
            to_json(&Empty {})
        }
        Call::MachineOpenWindow(MachineTarget { machine }) => {
            if machine == "local" {
                // A window of this machine for the device of this core, as a menu item asks for it.
                let core = crate::runtime::CoreHandle::from_inner(Arc::clone(inner));
                let origin = match core.local_device() {
                    Some(device) => crate::Origin::ui().from_device(device),
                    None => crate::Origin::ui(),
                };
                // `call_as` runs this method, so the future needs a box.
                Box::pin(core.call_as(origin, "window.open", serde_json::json!({}))).await?;
                return to_json(&Empty {});
            }
            let links = target(inner, &machine)?;
            tokio::time::timeout(CONNECT_WAIT, links.open_window(&machine))
                .await
                .map_err(|_| {
                    RpcError::new(ErrorCode::LinkDown, "the link did not open in time")
                })??;
            to_json(&Empty {})
        }
        Call::MachineDisconnect(MachineTarget { machine }) => {
            target(inner, &machine)?.disconnect(&machine)?;
            to_json(&Empty {})
        }
        other => Err(RpcError::internal(format!(
            "{} is not a method of the machines",
            other.method().name()
        ))),
    }
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|e| RpcError::internal(e.to_string()))
}
