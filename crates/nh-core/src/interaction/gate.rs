//! The hooks that shims hold.
//!
//! A shim that calls `adapter.gate` keeps its connection open until the core decides. This module
//! keeps the connections: the sender for each one, the deadline of each interaction, and the grace
//! time for a shim that lost its connection. It decides nothing. The state of an interaction is in
//! the model, and the actor changes it. A decision for a held hook goes out through the connection
//! of the lease, and a hook that the core lets go gets `no_decision`.
//!
//! ```text
//! shim ── adapter.gate ──► Gates::open ──► the adapter reads the event ──► the actor opens the interaction
//!      ◄─ {wait, interaction} ──────────────────────────────────────────────┘
//! client ── interaction.answer ──► the actor records the decision ──► Effect::Deliver
//!      ◄─ adapter.gate_result {output, key} ◄── Gates::send ◄── apply::deliver
//! shim ── adapter.delivery_ack ──► the actor marks the delivery acknowledged
//! ```

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock, PoisonError},
    time::Duration,
};

use nh_proto::api::{common::Harness, ids::InteractionId, time::Timestamp};
use serde_json::{Value, json};
use tokio::{
    runtime::Handle,
    sync::{
        mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
        oneshot,
    },
};
use ulid::Ulid;

use super::{
    apply,
    model::{Effect, Holder, Interaction, Lease},
};
use crate::{
    ErrorCode,
    actor::ActorHandle,
    agents::{AdapterHosts, host::GateTicket, runs},
    error::RpcError,
    persist::now_ms,
};

/// How long a shim that lost its connection has to come back.
const RESUME_GRACE: Duration = Duration::from_secs(15);

/// The longest time to the first reply of `adapter.gate`.
const FIRST_REPLY: Duration = Duration::from_secs(5);

/// What the core sends to the connection of a shim after the first reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GateOutcome {
    /// The decision. The shim prints `output` and exits.
    Decision {
        interaction: InteractionId,
        output: String,
        key: String,
    },
    /// Nothing to print. The harness shows its own dialog.
    NoDecision {
        interaction: InteractionId,
        reason: &'static str,
    },
}

impl GateOutcome {
    /// The notification line `adapter.gate_result`.
    pub fn line(&self) -> Vec<u8> {
        let params = match self {
            Self::Decision {
                interaction,
                output,
                key,
            } => json!({
                "interaction": interaction.to_string(),
                "output": output,
                "idempotency_key": key,
            }),
            Self::NoDecision {
                interaction,
                reason,
            } => json!({
                "interaction": interaction.to_string(),
                "no_decision": true,
                "reason": reason,
            }),
        };
        let mut line = serde_json::to_vec(
            &json!({"jsonrpc": "2.0", "method": "adapter.gate_result", "params": params}),
        )
        .unwrap_or_default();
        line.push(b'\n');
        line
    }
}

/// What the adapter and the actor tell the gate when a hook asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GateReply {
    /// An interaction opened for the hook. The shim waits.
    Wait { interaction: InteractionId },
    /// Nothing to decide: the shim prints nothing.
    NoDecision(&'static str),
}

/// The first reply to `adapter.gate` and `adapter.gate_resume`, and the channel for the rest.
pub(crate) struct GateStart {
    pub result: Value,
    pub rx: Option<UnboundedReceiver<GateOutcome>>,
}

impl GateStart {
    fn no_decision(reason: &str) -> Self {
        Self {
            result: json!({"no_decision": true, "reason": reason}),
            rx: None,
        }
    }
}

struct Bound {
    actor: ActorHandle,
    hosts: Arc<AdapterHosts>,
}

/// The connections of the shims that hold a hook.
pub(crate) struct Gates {
    conns: Mutex<HashMap<u64, UnboundedSender<GateOutcome>>>,
    bound: OnceLock<Bound>,
    handle: Handle,
    /// A test or a recording sets the gate timeout. Without it the setting
    /// `approvals.gate_timeout` counts.
    timeout: Option<Duration>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Gates {
    pub fn new(handle: Handle, timeout: Option<Duration>) -> Arc<Self> {
        Arc::new(Self {
            conns: Mutex::new(HashMap::new()),
            bound: OnceLock::new(),
            handle,
            timeout,
        })
    }

    /// The gate timeout that a test or a recording asked for.
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    /// Gives the gates the actor and the host of the adapters, once the core exists. Only the
    /// first call counts.
    pub fn bind(&self, actor: ActorHandle, hosts: Arc<AdapterHosts>) {
        let _ = self.bound.set(Bound { actor, hosts });
    }

    fn bound(&self) -> Result<&Bound, RpcError> {
        self.bound.get().ok_or_else(|| {
            RpcError::new(ErrorCode::GateUnavailable, "this core does not hold hooks")
        })
    }

    // --- the connections -----------------------------------------------------------------------

    fn register(&self, conn: u64, tx: UnboundedSender<GateOutcome>) {
        lock(&self.conns).insert(conn, tx);
    }

    /// Whether a shim waits on a gate now. `nh daemon` keeps the machine awake then.
    pub fn holding(&self) -> bool {
        !lock(&self.conns).is_empty()
    }

    /// Sends an outcome to the shim on a connection. `false` when the connection is gone.
    pub fn send(&self, conn: u64, outcome: GateOutcome) -> bool {
        match lock(&self.conns).get(&conn) {
            Some(tx) => tx.send(outcome).is_ok(),
            None => false,
        }
    }

    /// The connection of a shim ended. The interactions that it held lose their lease.
    pub fn connection_closed(&self, conn: u64) {
        let removed = lock(&self.conns).remove(&conn).is_some();
        if !removed {
            return;
        }
        if let Some(bound) = self.bound.get() {
            let gates = bound.actor.clone();
            gates.update_detached(move |mut parts| apply::connection_closed(&mut parts, conn));
        }
    }

    // --- the requests of the shim --------------------------------------------------------------

    /// `adapter.gate`: a shim asks what to do with a request of the harness. The adapter reads the
    /// event. If the event opens an interaction, the shim waits for the decision on `rx`.
    pub async fn open(
        self: &Arc<Self>,
        pane: Ulid,
        harness: Harness,
        event: String,
        payload: Value,
        conn: u64,
        pid: Option<i32>,
    ) -> Result<GateStart, RpcError> {
        let bound = self.bound()?;
        let (tx, rx) = unbounded_channel();
        self.register(conn, tx);
        let lease = Lease {
            holder: Holder::Shim,
            conn,
            pid,
            expires_at: Timestamp(i64::MAX),
        };
        // The adapter and the actor answer in milliseconds. A shim does not wait longer than this
        // for a core that does not answer.
        let (reply_tx, reply_rx) = oneshot::channel();
        let hosts = Arc::clone(&bound.hosts);
        let at = Timestamp(now_ms());
        bound.actor.update_detached(move |mut parts| {
            runs::gate(
                &mut parts,
                &hosts,
                nh_proto::api::ids::PaneId::new(pane),
                harness,
                event,
                payload,
                at,
                GateTicket {
                    lease,
                    reply: reply_tx,
                },
            );
        });
        let reply = tokio::time::timeout(FIRST_REPLY, reply_rx)
            .await
            .map_or(Err(()), |reply| reply.map_err(|_| ()));
        match reply {
            Ok(GateReply::Wait { interaction }) => Ok(GateStart {
                result: json!({
                    "wait": true,
                    "interaction": interaction.to_string(),
                    "lease": conn.to_string(),
                }),
                rx: Some(rx),
            }),
            Ok(GateReply::NoDecision(reason)) => {
                lock(&self.conns).remove(&conn);
                Ok(GateStart::no_decision(reason))
            }
            Err(_) => {
                lock(&self.conns).remove(&conn);
                Ok(GateStart::no_decision("adapter_failed"))
            }
        }
    }

    /// `adapter.gate_resume`: a shim whose connection dropped asks for its interaction again.
    /// `pane` is the pane of the token of the shim. The interaction of another pane is "no such
    /// interaction", so the token of one pane cannot take the hook of another.
    pub async fn resume(
        self: &Arc<Self>,
        interaction: InteractionId,
        pane: Ulid,
        conn: u64,
        pid: Option<i32>,
    ) -> Result<GateStart, RpcError> {
        let bound = self.bound()?;
        let (tx, rx) = unbounded_channel();
        self.register(conn, tx);
        let lease = Lease {
            holder: Holder::Shim,
            conn,
            pid,
            expires_at: Timestamp(i64::MAX),
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        let pane = nh_proto::api::ids::PaneId::new(pane);
        bound.actor.update_detached(move |mut parts| {
            let _ = reply_tx.send(apply::resume(&mut parts, interaction, pane, lease));
        });
        match reply_rx.await {
            Ok(Some(GateReply::Wait { interaction })) => Ok(GateStart {
                result: json!({
                    "wait": true,
                    "interaction": interaction.to_string(),
                    "lease": conn.to_string(),
                }),
                rx: Some(rx),
            }),
            Ok(Some(GateReply::NoDecision(reason))) => {
                lock(&self.conns).remove(&conn);
                Ok(GateStart::no_decision(reason))
            }
            Ok(None) => {
                lock(&self.conns).remove(&conn);
                Err(RpcError::new(ErrorCode::NotFound, "no such interaction"))
            }
            Err(_) => {
                lock(&self.conns).remove(&conn);
                Ok(GateStart::no_decision("core_stopping"))
            }
        }
    }

    /// `adapter.delivery_ack`: the shim printed the decision. `pane` is the pane of the token of the
    /// shim, as for [`Gates::resume`].
    pub async fn ack(
        self: &Arc<Self>,
        interaction: InteractionId,
        pane: Ulid,
        key: String,
        applied: bool,
    ) -> Result<Value, RpcError> {
        let bound = self.bound()?;
        let pane = nh_proto::api::ids::PaneId::new(pane);
        let known = bound
            .actor
            .update(move |mut parts| {
                if !apply::belongs_to(&parts, interaction, pane) {
                    return None;
                }
                apply::acked(&mut parts, interaction, &key, applied)
            })
            .await
            .map_err(|e| RpcError::internal(e.to_string()))?;
        match known {
            Some(_) => Ok(json!({})),
            None => Err(RpcError::new(ErrorCode::NotFound, "no such interaction")),
        }
    }

    // --- the effects of the model --------------------------------------------------------------

    /// Does what a step of the model asks outside the actor: lets a hook go, or schedules the
    /// delivery of a decision on the actor.
    pub fn perform(&self, interaction: &Interaction, effect: &Effect) {
        match effect {
            Effect::Release { conn, reason } => {
                self.send(
                    *conn,
                    GateOutcome::NoDecision {
                        interaction: interaction.id,
                        reason,
                    },
                );
            }
            Effect::Deliver => {
                if let Some(bound) = self.bound.get() {
                    let id = interaction.id;
                    bound
                        .actor
                        .update_detached(move |mut parts| apply::deliver(&mut parts, id));
                }
            }
        }
    }

    /// Runs a job on the actor, after the command that is running now.
    pub fn post(&self, job: impl FnOnce(crate::actor::Parts<'_>) + Send + 'static) {
        if let Some(bound) = self.bound.get() {
            bound.actor.update_detached(job);
        }
    }

    /// Arms the deadline of an interaction that a shim holds.
    pub fn arm_deadline(&self, id: InteractionId, deadline: Timestamp) {
        let Some(bound) = self.bound.get() else {
            return;
        };
        let actor = bound.actor.clone();
        let wait = u64::try_from((deadline.0 - now_ms()).max(0)).unwrap_or(0);
        self.handle.spawn(async move {
            tokio::time::sleep(Duration::from_millis(wait)).await;
            actor.update_detached(move |mut parts| apply::deadline(&mut parts, id));
        });
    }

    /// A shim lost its connection while the interaction is open: it has a time to come back.
    pub fn arm_grace(&self, id: InteractionId) {
        let Some(bound) = self.bound.get() else {
            return;
        };
        let actor = bound.actor.clone();
        self.handle.spawn(async move {
            tokio::time::sleep(RESUME_GRACE).await;
            actor.update_detached(move |mut parts| apply::grace_over(&mut parts, id));
        });
    }
}
