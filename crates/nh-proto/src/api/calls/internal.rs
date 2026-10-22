//! Params and results of `adapter.*` (pane scope) and `debug.*` (dev builds only).
//!
//! The UI cannot call `adapter.*`. These methods need the token of a pane, which only a hook shim
//! holds. The wire format of the gate is the one that the shim (`nh hook`) speaks.

use serde_json::Value;

use crate::api::{
    calls::agent::Decision,
    common::{Harness, Via, wire},
    ids::{InteractionId, PaneId},
    model::InteractionState,
};

wire! {
    /// A hook of a harness, as the shim sends it. The payload is the raw JSON that the
    /// harness wrote to the stdin of the hook.
    pub struct AdapterSignal {
        /// The `NEOHAR_PANE_TOKEN` of the pane.
        pub token: String,
        pub harness: Harness,
        /// The name of the hook event, for example `PreToolUse`.
        pub event: String,
        pub payload: Value,
        /// The shim cut the payload at its cap of 4 MiB.
        #[serde(default)]
        pub truncated: bool,
    }

    /// The receipt for a signal. The shim does not care what is in it.
    pub struct AdapterReceipt {
        pub ok: bool,
    }

    /// The params of `adapter.gate` are those of a signal. The shim holds the hook until the core
    /// answers.
    pub struct AdapterGate {
        pub token: String,
        pub harness: Harness,
        pub event: String,
        pub payload: Value,
        #[serde(default)]
        pub truncated: bool,
    }

    /// The first reply to `adapter.gate` and to `adapter.gate_resume`.
    #[serde(untagged)]
    pub enum GateReply {
        /// The core opened an interaction, and the hook waits. A `adapter.gate_result`
        /// notification follows on the same connection.
        Wait {
            wait: bool,
            interaction: InteractionId,
            /// The lease of this connection.
            lease: String,
        },
        /// The decision is known at once (policy, or a decision recorded before a resume).
        Decision {
            interaction: InteractionId,
            /// What the shim prints on stdout. The shim does not read it.
            output: String,
            idempotency_key: String,
        },
        /// The hook prints nothing, and the harness shows its own dialog.
        NoDecision {
            no_decision: bool,
            reason: String,
        },
    }

    /// The params of the notification `adapter.gate_result`: the end of a wait.
    #[serde(untagged)]
    pub enum GateResult {
        Decision {
            interaction: InteractionId,
            output: String,
            idempotency_key: String,
        },
        NoDecision {
            interaction: InteractionId,
            no_decision: bool,
            reason: String,
        },
    }

    /// A shim whose connection dropped asks for its interaction again, once.
    pub struct AdapterGateResume {
        pub interaction: InteractionId,
    }

    /// The shim applied the decision. It sends this just before it prints.
    pub struct AdapterDeliveryAck {
        pub interaction: InteractionId,
        pub idempotency_key: String,
        pub applied: bool,
    }

    // --- debug ---

    /// `debug.screen`: what a pane shows, with the journal offset it belongs to. The pane can be
    /// one that `debug.spawn_panes` started, outside the layout.
    pub struct DebugScreen {
        pub pane: PaneId,
        /// Include the cells (`dump`). A call without them is cheap.
        #[serde(default = "yes")]
        pub dump: bool,
    }

    pub struct DebugScreenResult {
        pub pane: PaneId,
        /// The journal offset that the engine read up to. With `dump`, the offset of the dump.
        pub fed_offset: u64,
        /// How long ago the engine last changed its size.
        pub since_resize_ms: u64,
        /// The pane reads the journal after a restart and shows no final screen yet.
        pub restoring: bool,
        /// The pane has read its journal and the resize nudge is over. A pane that was not
        /// restored is settled from the start.
        pub settled: bool,
        /// How the pane got its screen: `fresh`, `snapshot`, `journal` or `ring_only`.
        pub recovery: String,
        /// The size that the last resize request asked for, as columns and rows.
        pub asked_size: (u16, u16),
        /// The size of the engine, as columns and rows.
        pub size: (u16, u16),
        /// The cells, the cursor, the modes and the title (a `ScreenDump` of `nh-term`). Only with
        /// `dump`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub dump: Option<Value>,
        /// The screen at the end of the replay. Only with `dump`, and only for a pane that a
        /// restart restored.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub restored: Option<DebugRestoredScreen>,
    }

    /// The screen of a restored pane at the end of the replay.
    pub struct DebugRestoredScreen {
        /// The journal offset at the end of the replay.
        pub fed_offset: u64,
        /// A `ScreenDump` of `nh-term`.
        pub dump: Value,
    }

    /// `debug.answer`: decides an interaction the way a client would, and may hold the delivery
    /// back so that a test can control the order of events.
    pub struct DebugAnswer {
        pub interaction: crate::api::ids::Ref<InteractionId>,
        pub decision: Decision,
        #[serde(default = "debug_name")]
        pub by: String,
        pub via: Via,
        /// Record the decision and do not deliver it. A later `debug.deliver` does.
        #[serde(default)]
        pub hold: bool,
    }

    pub struct DebugAnswered {
        pub rev: u32,
        pub state: InteractionState,
    }

    /// `debug.spawn_panes`: panes for a test, outside the layout. No window draws them. The panes
    /// start one after the other and run the same program.
    pub struct DebugSpawnPanes {
        /// How many panes, from 1 to 64.
        #[serde(default = "one")]
        pub count: u32,
        pub cols: u16,
        pub rows: u16,
        /// The program and its arguments. Without it, the `SHELL` of the core, else `/bin/sh`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub argv: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub cwd: Option<String>,
        /// Extra variables for the environment, as pairs of a name and a value.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub env: Vec<(String, String)>,
        /// Give each pane its own `HOME`, `CLAUDE_CONFIG_DIR` and `CODEX_HOME` in the data
        /// directory of the session, so that a harness never touches the real ones. Without
        /// `cwd`, the pane starts in that `HOME`.
        #[serde(default)]
        pub isolated_home: bool,
    }

    /// A pane that `debug.spawn_panes` started.
    pub struct DebugSpawnedPane {
        pub pane: PaneId,
        /// The number of the pane in the render frames.
        pub render_id: u32,
    }

    pub struct DebugSpawned {
        pub panes: Vec<DebugSpawnedPane>,
    }
}

fn debug_name() -> String {
    "debug".to_owned()
}

fn yes() -> bool {
    true
}

fn one() -> u32 {
    1
}
