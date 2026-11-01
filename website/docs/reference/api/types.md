---
title: "Types"
description: "Every one of the 494 named types of the API contract, with fields and the places that use it."
sidebar_position: 7
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The contract names 494 types. They are Rust types in `nh-proto` (behind its `api` feature) with `serde` and `schemars::JsonSchema`, and `mise run gen:api` turns the same schema into TypeScript in [`packages/transport/src/generated.ts`](https://github.com/nikodem-wrona/neohar/blob/main/packages/transport/src/generated.ts).

Jump to a letter: [A](#letter-a) · [B](#letter-b) · [C](#letter-c) · [D](#letter-d) · [E](#letter-e) · [F](#letter-f) · [G](#letter-g) · [H](#letter-h) · [I](#letter-i) · [J](#letter-j) · [K](#letter-k) · [L](#letter-l) · [M](#letter-m) · [N](#letter-n) · [P](#letter-p) · [Q](#letter-q) · [R](#letter-r) · [S](#letter-s) · [T](#letter-t) · [U](#letter-u) · [V](#letter-v) · [W](#letter-w)

## A {#letter-a}

### `AdapterDeliveryAck` {#adapterdeliveryack}

The shim applied the decision. It sends this just before it prints.

| Field | Type | Required | Description |
|---|---|---|---|
| `applied` | boolean | yes |  |
| `idempotency_key` | string | yes |  |
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/internal.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/internal.rs#L92) · **Used by:** [`adapter.delivery_ack`](/docs/reference/api/methods/adapter#adapter-delivery-ack)

### `AdapterDisagreement` {#adapterdisagreement}

A lower source disagreed with a structured one.

| Field | Type | Required | Description |
|---|---|---|---|
| `facet` | [`Facet`](/docs/reference/api/types#facet) | yes |  |
| `other` | any | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) | yes |  |
| `structured` | any | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L108) · **Used by:** [event `adapter.disagreement`](/docs/reference/api/events#adapter-disagreement)

### `AdapterGate` {#adaptergate}

The params of `adapter.gate` are those of a signal. The shim holds the hook until the core answers.

| Field | Type | Required | Description |
|---|---|---|---|
| `event` | string | yes |  |
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `payload` | any | yes |  |
| `token` | string | yes |  |
| `truncated` | boolean |  | Default: `false`. |

**Rust:** [`crates/nh-proto/src/api/calls/internal.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/internal.rs#L37) · **Used by:** [`adapter.gate`](/docs/reference/api/methods/adapter#adapter-gate)

### `AdapterGateResume` {#adaptergateresume}

A shim whose connection dropped asks for its interaction again, once.

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/internal.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/internal.rs#L87) · **Used by:** [`adapter.gate_resume`](/docs/reference/api/methods/adapter#adapter-gate-resume)

### `AdapterHealth` {#adapterhealth}

One of these forms:

#### `state: "healthy"`

| Field | Type | Required | Description |
|---|---|---|---|
| `state` | `"healthy"` | yes |  |

#### `state: "degraded"`

| Field | Type | Required | Description |
|---|---|---|---|
| `reason` | string | yes |  |
| `state` | `"degraded"` | yes |  |

#### `state: "disconnected"`

| Field | Type | Required | Description |
|---|---|---|---|
| `state` | `"disconnected"` | yes |  |

#### `state: "unvalidated_version"`

| Field | Type | Required | Description |
|---|---|---|---|
| `state` | `"unvalidated_version"` | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L238) · **Used by:** [`HealthFacet`](/docs/reference/api/types#healthfacet)

### `AdapterReceipt` {#adapterreceipt}

The receipt for a signal. The shim does not care what is in it.

| Field | Type | Required | Description |
|---|---|---|---|
| `ok` | boolean | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/internal.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/internal.rs#L31) · **Used by:** [`adapter.signal`](/docs/reference/api/methods/adapter#adapter-signal)

### `AdapterSignal` {#adaptersignal}

A hook of a harness, as the shim sends it. The payload is the raw JSON that the harness wrote to the stdin of the hook.

| Field | Type | Required | Description |
|---|---|---|---|
| `event` | string | yes | The name of the hook event, for example `PreToolUse`. |
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `payload` | any | yes |  |
| `token` | string | yes | The `NEOHAR_PANE_TOKEN` of the pane. |
| `truncated` | boolean |  | The shim cut the payload at its cap of 4 MiB. Default: `false`. |

**Rust:** [`crates/nh-proto/src/api/calls/internal.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/internal.rs#L18) · **Used by:** [`adapter.signal`](/docs/reference/api/methods/adapter#adapter-signal)

### `AgentChoices` {#agentchoices}

The models and the effort levels that `agent.start` can choose.

| Field | Type | Required | Description |
|---|---|---|---|
| `harness` | [`Harness`](/docs/reference/api/types#harness) \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L72) · **Used by:** [`agent.choices`](/docs/reference/api/methods/agent#agent-choices)

### `AgentChoicesResult` {#agentchoicesresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `harnesses` | [`HarnessChoices`](/docs/reference/api/types#harnesschoices)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L104) · **Used by:** [`agent.choices`](/docs/reference/api/methods/agent#agent-choices)

### `AgentDetected` {#agentdetected}

A run was seen. It is not identified yet.

| Field | Type | Required | Description |
|---|---|---|---|
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) | yes |  |
| `via` | string | yes | How the core found it: `launch`, `hook`, `process`, `screen`. |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L69) · **Used by:** [event `agent.detected`](/docs/reference/api/events#agent-detected)

### `AgentDriftDetected` {#agentdriftdetected}

| Field | Type | Required | Description |
|---|---|---|---|
| `bad_signal_rate` | number (float) | yes |  |
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `observations` | integer (uint32) | yes |  |
| `version` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L100) · **Used by:** [event `agent.drift_detected`](/docs/reference/api/events#agent-drift-detected)

### `AgentHarnessVersionUnvalidated` {#agentharnessversionunvalidated}

| Field | Type | Required | Description |
|---|---|---|---|
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `version` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L95) · **Used by:** [event `agent.harness_version_unvalidated`](/docs/reference/api/events#agent-harness-version-unvalidated)

### `AgentIdentified` {#agentidentified}

A run got its session id and its pane.

| Field | Type | Required | Description |
|---|---|---|---|
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) \| `null` |  |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) | yes |  |
| `session_id` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L77) · **Used by:** [event `agent.identified`](/docs/reference/api/events#agent-identified)

### `AgentInterrupt` {#agentinterrupt}

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L123) · **Used by:** [`agent.interrupt`](/docs/reference/api/methods/agent#agent-interrupt)

### `AgentLink` {#agentlink}

Binds a run to a pane by the user's decision.

| Field | Type | Required | Description |
|---|---|---|---|
| `harness` | [`Harness`](/docs/reference/api/types#harness) \| `null` |  |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `session_id` | string \| `null` |  |  |
| `thread` | string \| `null` |  | The thread of a Codex run on the shared daemon. |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L177) · **Used by:** [`agent.link`](/docs/reference/api/methods/agent#agent-link)

### `AgentLinked` {#agentlinked}

| Field | Type | Required | Description |
|---|---|---|---|
| `run` | [`RunId`](/docs/reference/api/types#runid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L188) · **Used by:** [`agent.link`](/docs/reference/api/methods/agent#agent-link)

### `AgentList` {#agentlist}

| Field | Type | Required | Description |
|---|---|---|---|
| `harness` | [`Harness`](/docs/reference/api/types#harness) \| `null` |  |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) \| `null` |  |  |
| `workspace` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L12) · **Used by:** [`agent.list`](/docs/reference/api/methods/agent#agent-list)

### `AgentListResult` {#agentlistresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `agents` | [`AgentValue`](/docs/reference/api/types#agentvalue)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L21) · **Used by:** [`agent.list`](/docs/reference/api/methods/agent#agent-list)

### `AgentMove` {#agentmove}

Puts a run at another place on the board of every window. The order stays across a restart of the core, and a new run in the pane of a run that ended takes its place.

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |
| `index` | integer (uint32) | yes | The place among the runs, counted from 0. A place after the last run is the end. |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L198) · **Used by:** [`agent.move`](/docs/reference/api/methods/agent#agent-move)

### `AgentPrompt` {#agentprompt}

Types a prompt into a bound run (never into a suggested one).

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |
| `submit` | boolean |  | Press Enter after the text. Default: `true`. |
| `text` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L128) · **Used by:** [`agent.prompt`](/docs/reference/api/methods/agent#agent-prompt)

### `AgentReply` {#agentreply}

The next prompt for a run that is `idle` and has no open interaction. The core writes the text as a bracketed paste and a carriage return, at most once for an `input_id`, and takes no geometry lease.

| Field | Type | Required | Description |
|---|---|---|---|
| `input_id` | integer (uint64) | yes |  |
| `run` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |
| `text` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L139) · **Used by:** [`agent.reply`](/docs/reference/api/methods/agent#agent-reply)

### `AgentResume` {#agentresume}

Starts the harness again with the resume handle of the run. It returns a job.

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |
| `placement` | [`Placement`](/docs/reference/api/types#placement) \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L146) · **Used by:** [`agent.resume`](/docs/reference/api/methods/agent#agent-resume)

### `AgentShow` {#agentshow}

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L25) · **Used by:** [`agent.show`](/docs/reference/api/methods/agent#agent-show)

### `AgentShown` {#agentshown}

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`AgentValue`](/docs/reference/api/types#agentvalue) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L29) · **Used by:** [`agent.show`](/docs/reference/api/methods/agent#agent-show)

### `AgentStart` {#agentstart}

`agent.start` returns a job. The run appears in the `agents` projection.

| Field | Type | Required | Description |
|---|---|---|---|
| `args` | string[] |  | Extra arguments for the harness. |
| `cwd` | string \| `null` |  |  |
| `effort` | string \| `null` |  | The effort level (`high`). Without it, the harness uses the level of its own settings. A manifest without `effort_arg` refuses it. |
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `model` | string \| `null` |  | The model, as the harness names it (`opus`, `gpt-5.5`). Without it, the harness uses the model of its own settings. |
| `name` | string \| `null` |  | A unique name for the run. A call can use it instead of the id. |
| `placement` | [`Placement`](/docs/reference/api/types#placement) | yes |  |
| `prompt` | string \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L48) · **Used by:** [`agent.start`](/docs/reference/api/methods/agent#agent-start)

### `AgentStateChanged` {#agentstatechanged}

Every change of a facet.

| Field | Type | Required | Description |
|---|---|---|---|
| `confidence` | number (float) | yes |  |
| `facet` | [`Facet`](/docs/reference/api/types#facet) | yes |  |
| `from` | any | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) | yes |  |
| `source` | [`Source`](/docs/reference/api/types#source) | yes |  |
| `to` | any | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L86) · **Used by:** [event `agent.state_changed`](/docs/reference/api/events#agent-state-changed)

### `AgentStop` {#agentstop}

Asks the agent to end. With `force` the core ends the process.

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |
| `force` | boolean |  | Default: `false`. |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L117) · **Used by:** [`agent.stop`](/docs/reference/api/methods/agent#agent-stop)

### `AgentUnlink` {#agentunlink}

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L192) · **Used by:** [`agent.unlink`](/docs/reference/api/methods/agent#agent-unlink)

### `AgentValue` {#agentvalue}

`agents`: one run of Claude Code or Codex.

| Field | Type | Required | Description |
|---|---|---|---|
| `binding` | [`Binding`](/docs/reference/api/types#binding) | yes |  |
| `capabilities` | [`Capabilities`](/docs/reference/api/types#capabilities) | yes |  |
| `facets` | [`Facets`](/docs/reference/api/types#facets) | yes |  |
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `harness_version` | string \| `null` |  |  |
| `id` | [`RunId`](/docs/reference/api/types#runid) | yes |  |
| `isolation` | string \| `null` |  |  |
| `model` | string \| `null` |  |  |
| `name` | string \| `null` |  | The unique name, if the run has one. A call can use it instead of the id. |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) \| `null` |  |  |
| `rate_limit` | [`RateLimitWindow`](/docs/reference/api/types#ratelimitwindow) \| `null` |  |  |
| `resumable` | boolean |  | A resume handle exists (`claude --resume`, `codex resume`). Default: `false`. |
| `session_id` | string \| `null` |  |  |
| `started_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `usage` | [`Usage`](/docs/reference/api/types#usage) | yes |  |
| `yolo` | boolean |  | The run bypasses the permission prompts of the harness. Default: `false`. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L322) · **Used by:** [projection `agents`](/docs/reference/api/projections#agents), [projection `agents_for`](/docs/reference/api/projections#agents-for), [`AgentListResult`](/docs/reference/api/types#agentlistresult), [`AgentShown`](/docs/reference/api/types#agentshown), [`AgentWaitResult`](/docs/reference/api/types#agentwaitresult)

### `AgentWait` {#agentwait}

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |
| `timeout_ms` | integer (uint64) \| `null` |  |  |
| `until` | [`WaitFor`](/docs/reference/api/types#waitfor) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L163) · **Used by:** [`agent.wait`](/docs/reference/api/methods/agent#agent-wait)

### `AgentWaitResult` {#agentwaitresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`AgentValue`](/docs/reference/api/types#agentvalue) | yes |  |
| `reached` | boolean | yes | `false` when the time ran out. |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L170) · **Used by:** [`agent.wait`](/docs/reference/api/methods/agent#agent-wait)

### `AnswerChannel` {#answerchannel}

How an answer can reach the harness.

Values: `"native"`, `"keystroke"`.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L416) · **Used by:** [`InteractionValue`](/docs/reference/api/types#interactionvalue)

### `ApprovalDecision` {#approvaldecision}

| Field | Type | Required | Description |
|---|---|---|---|
| `always` | boolean |  | "Allow always": a rule for the session. Default: `false`. |
| `behavior` | [`Behavior`](/docs/reference/api/types#behavior) | yes |  |
| `message` | string \| `null` |  | The text for a deny. The model sees it. |
| `persist` | boolean |  | With `always`: write the rule to the settings of the harness. It needs `approvals.claude.persist_always`. Default: `false`. |
| `stop` | boolean |  | "Deny and stop": the turn ends. Default: `false`. |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L237) · **Used by:** [`Decision`](/docs/reference/api/types#decision)

### `ApprovalPayload` {#approvalpayload}

| Field | Type | Required | Description |
|---|---|---|---|
| `command` | [`Command`](/docs/reference/api/types#command) \| `null` |  |  |
| `cwd` | string \| `null` |  |  |
| `diff` | [`Diff`](/docs/reference/api/types#diff) \| `null` |  |  |
| `input` | any |  | The input of a tool that has no special card, cut at a cap. |
| `paths` | string[] |  |  |
| `peek` | string[] |  | The last lines of the pane, for the context strip. |
| `policy_note` | string \| `null` |  | A rule of the policy matches this request, but the core cannot answer it, for example "The rule `pnpm-test` would allow this." |
| `tool` | string | yes | The tool: `Bash`, `Edit`, `exec_command`, an MCP tool. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L436) · **Used by:** [`InteractionPayload`](/docs/reference/api/types#interactionpayload)

### `AskpassAnswered` {#askpassanswered}

The answer of the person, or `null` when the person cancelled. The core keeps it nowhere: not in an event, not in a log.

| Field | Type | Required | Description |
|---|---|---|---|
| `answer` | string \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L457) · **Used by:** [`askpass.ask`](/docs/reference/api/methods/askpass#askpass-ask)

### `AskpassAsk` {#askpassask}

`askpass.ask`: a question of `ssh` that the app asks the person. `nh askpass` calls it with the text that ssh gave. Only the host in the process of the core answers.

| Field | Type | Required | Description |
|---|---|---|---|
| `machine` | string \| `null` |  | The label of the machine that the link is for, when the core knows it. |
| `prompt` | string | yes | The question as ssh wrote it: a host key, a passphrase or a password. |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L447) · **Used by:** [`askpass.ask`](/docs/reference/api/methods/askpass#askpass-ask)

### `AttentionReason` {#attentionreason}

One of these forms:

- `"approval"` | `"question"` | `"plan_review"` | `"error"` | `"rate_limited"`

- `"turn_ended"` — The turn of the run ended while its pane had no focus, and nobody looked at the pane or typed into it since.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L355) · **Used by:** [`AttentionValue`](/docs/reference/api/types#attentionvalue), [`PhonePushed`](/docs/reference/api/types#phonepushed)

### `AttentionValue` {#attentionvalue}

`attention`: a run that needs the user, in the order of urgency. The number of entries is the badge count.

| Field | Type | Required | Description |
|---|---|---|---|
| `interactions` | [`InteractionId`](/docs/reference/api/types#interactionid)[] | yes |  |
| `reasons` | [`AttentionReason`](/docs/reference/api/types#attentionreason)[] | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) | yes |  |
| `since` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `urgency` | [`Urgency`](/docs/reference/api/types#urgency) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L368) · **Used by:** [projection `attention`](/docs/reference/api/projections#attention)

### `Axis` {#axis}

How a split node lays out its children.

| Value | Meaning |
|---|---|
| `"row"` | Side by side, from left to right. |
| `"column"` | One above the other, from top to bottom. |

**Rust:** [`crates/nh-proto/src/api/common.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/common.rs#L72) · **Used by:** [`LayoutNode`](/docs/reference/api/types#layoutnode)

## B {#letter-b}

### `Behavior` {#behavior}

Values: `"allow"`, `"deny"`.

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L232) · **Used by:** [`ApprovalDecision`](/docs/reference/api/types#approvaldecision)

### `Binding` {#binding}

How sure the core is about the pane of a run.

One of these forms:

#### `state: "bound"`

Deterministic: neohar started the run, a signal carried the token of the pane, or the user linked it.

| Field | Type | Required | Description |
|---|---|---|---|
| `state` | `"bound"` | yes |  |

#### `state: "suggested"`

Correlation only. It can drive display, and never an answer.

| Field | Type | Required | Description |
|---|---|---|---|
| `confidence` | number (float) | yes |  |
| `state` | `"suggested"` | yes |  |

#### `state: "unlinked"`

| Field | Type | Required | Description |
|---|---|---|---|
| `state` | `"unlinked"` | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L312) · **Used by:** [`AgentValue`](/docs/reference/api/types#agentvalue)

### `Bounds` {#bounds}

The bounds of a window on the screen, in logical pixels.

| Field | Type | Required | Description |
|---|---|---|---|
| `height` | integer (uint32) | yes |  |
| `width` | integer (uint32) | yes |  |
| `x` | integer (int32) | yes |  |
| `y` | integer (int32) | yes |  |

**Rust:** [`crates/nh-proto/src/api/common.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/common.rs#L52) · **Used by:** [`WindowOpen`](/docs/reference/api/types#windowopen), [`WindowSetBounds`](/docs/reference/api/types#windowsetbounds), [`WindowValue`](/docs/reference/api/types#windowvalue)

### `Bytes` {#bytes}

Bytes as base64 (standard alphabet, with padding).

Type: string (base64).

**Rust:** [`crates/nh-proto/src/api/common.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/common.rs#L177) · **Used by:** [`PaneSendBytes`](/docs/reference/api/types#panesendbytes)

## C {#letter-c}

### `CallError` {#callerror}

The error as a caller sees it: `{code, message, details}`.

| Field | Type | Required | Description |
|---|---|---|---|
| `code` | [`ErrorCode`](/docs/reference/api/types#errorcode) | yes |  |
| `details` | any |  |  |
| `message` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/envelope.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/envelope.rs#L411) · **Used by:** [`JobValue`](/docs/reference/api/types#jobvalue)

### `Capabilities` {#capabilities}

What neohar can do with the run. The UI offers only what is listed.

| Field | Type | Required | Description |
|---|---|---|---|
| `answer_keystroke` | boolean | yes |  |
| `answer_native` | [`InteractionKind`](/docs/reference/api/types#interactionkind)[] | yes | The kinds of interaction that have a native channel. |
| `gate` | boolean | yes |  |
| `observe` | boolean | yes |  |
| `reconcile` | boolean | yes |  |
| `resume` | boolean | yes |  |
| `steer` | boolean | yes |  |
| `survive_disconnect` | boolean | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L279) · **Used by:** [`AgentValue`](/docs/reference/api/types#agentvalue), [`CapabilityRow`](/docs/reference/api/types#capabilityrow)

### `CapabilityRow` {#capabilityrow}

One row of the capability matrix.

| Field | Type | Required | Description |
|---|---|---|---|
| `capabilities` | [`Capabilities`](/docs/reference/api/types#capabilities) | yes |  |
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `mode` | [`LaunchMode`](/docs/reference/api/types#launchmode) | yes |  |
| `version` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L315) · **Used by:** [`IntegrationCapabilitiesResult`](/docs/reference/api/types#integrationcapabilitiesresult)

### `ChangeRow` {#changerow}

`changes_for:<window>`: one changed file. The key is `path`.

| Field | Type | Required | Description |
|---|---|---|---|
| `add` | integer (uint32) | yes |  |
| `binary` | boolean |  | Git shows no lines for the file. Default: `false`. |
| `del` | integer (uint32) | yes |  |
| `dir` | string | yes | The folder of the path with a `/` at its end, or empty for a file at the root. |
| `name` | string | yes | The last part of the path. |
| `old_path` | string \| `null` |  | The path in `HEAD` of a renamed file. |
| `path` | string | yes | The path from the root of the worktree. |
| `stamp` | integer (uint64) | yes | Changes when the file in the worktree changes, so that an open diff loads again. It is 0 for a deleted file, and less than 2^53 so that JavaScript holds it exactly. |
| `status` | [`ChangeStatus`](/docs/reference/api/types#changestatus) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L601) · **Used by:** [projection `changes_for`](/docs/reference/api/projections#changes-for), [projection `git_changes`](/docs/reference/api/projections#git-changes), [`GitShown`](/docs/reference/api/types#gitshown), [`GitStatus`](/docs/reference/api/types#gitstatus)

### `ChangeStatus` {#changestatus}

What happened to a file since `HEAD`. A change of type counts as `modified`.

One of these forms:

- `"modified"` | `"added"` | `"deleted"` | `"renamed"`

- `"untracked"` — A file that git does not track and does not ignore.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L591) · **Used by:** [`ChangeRow`](/docs/reference/api/types#changerow)

### `ChangeTotals` {#changetotals}

`changes` of `changes_head:<window>`: the totals of the changes of a worktree.

| Field | Type | Required | Description |
|---|---|---|---|
| `error` | string \| `null` |  | Why the last count failed, for example "the worktree is gone". The other values and the rows are those of the count before. |
| `files` | integer (uint32) | yes | The changed files, also those after the first 1,000. |
| `truncated` | boolean |  | The worktree has more than 1,000 changed files, and `changes_for:<window>` has the first 1,000. Default: `false`. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L575) · **Used by:** [`ChangesHead`](/docs/reference/api/types#changeshead), [`GitHead`](/docs/reference/api/types#githead)

### `ChangesDiff` {#changesdiff}

`changes.diff` gives the diff of one path of the rows of `changes_for:<window>`.

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L345) · **Used by:** [`changes.diff`](/docs/reference/api/methods/changes#changes-diff)

### `ChangesFollow` {#changesfollow}

`changes.follow`: the Changes tab of a window shows the changes of the git worktree that holds `path`. The core fills `changes_head:<window>` and `changes_for:<window>` while the window follows one of them, and counts again when the worktree changes.

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | An absolute path: the folder of the Files tab of the window. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L338) · **Used by:** [`changes.follow`](/docs/reference/api/methods/changes#changes-follow)

### `ChangesHead` {#changeshead}

`changes_head:<window>`: the head of the Changes tab of a window. The window names a folder with `changes.follow`, and the changes are those of the git worktree that holds it, from `HEAD`: what is not committed. The core makes the row when it knows the worktree.

| Field | Type | Required | Description |
|---|---|---|---|
| `branch` | string \| `null` |  | The branch of `HEAD`, or `None` for a detached `HEAD`. |
| `changes` | [`ChangeTotals`](/docs/reference/api/types#changetotals) \| `null` |  | The totals of the last count, or `None` before the first count. |
| `diff_stats` | [`DiffStats`](/docs/reference/api/types#diffstats) \| `null` |  | The lines of all changed files. |
| `folder` | string | yes | The folder that the window named. |
| `root` | string \| `null` |  | The root of the git worktree that holds the folder, or `None` when the folder is not in a git worktree. |
| `trust` | [`GitTrustAsk`](/docs/reference/api/types#gittrustask) \| `null` |  | The core runs no git command in the repository until the person trusts it. Then `root` is the root of the worktree, and the head has no branch and no count. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L625) · **Used by:** [projection `changes_head`](/docs/reference/api/projections#changes-head)

### `ChatId` {#chatid}

The ULID of a hermes chat, as 26 characters.

Type: string. Pattern: `^[0-9A-HJKMNP-TV-Z]{26}$`.

**Used by:** [`HermesAnswer`](/docs/reference/api/types#hermesanswer), [`HermesChatEvent`](/docs/reference/api/types#hermeschatevent), [`HermesChatRef`](/docs/reference/api/types#hermeschatref), [`HermesChatValue`](/docs/reference/api/types#hermeschatvalue), [`HermesPrompt`](/docs/reference/api/types#hermesprompt), [`HermesSet`](/docs/reference/api/types#hermesset), [`HermesStarted`](/docs/reference/api/types#hermesstarted)

### `CheckStatus` {#checkstatus}

How a check of the doctor came out.

One of these forms:

- `"ok"`

- `"info"` — A fact that needs no action.

- `"warn"` — Something works, with a limit or a risk.

- `"fail"` — Something does not work.

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L333) · **Used by:** [`DoctorCheck`](/docs/reference/api/types#doctorcheck)

### `ClipboardDropped` {#clipboarddropped}

An OSC 52 clipboard write that no client got: the client that sent the last input to the pane is not connected.

| Field | Type | Required | Description |
|---|---|---|---|
| `at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `bytes` | integer (uint64) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L153) · **Used by:** [`PaneValue`](/docs/reference/api/types#panevalue)

### `ClipboardRequest` {#clipboardrequest}

An OSC 52 clipboard write that waits for an answer.

| Field | Type | Required | Description |
|---|---|---|---|
| `allowed` | boolean |  | The pane has a remembered Allow, and the last input came from a phone: the write waits only for the tap on Copy, which the clipboard of a phone needs. Default: `false`. |
| `bytes` | integer (uint64) | yes |  |
| `preview` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L141) · **Used by:** [`PaneValue`](/docs/reference/api/types#panevalue)

### `Command` {#command}

A shell command, for an approval card.

| Field | Type | Required | Description |
|---|---|---|---|
| `argv` | string[] | yes |  |
| `cwd` | string | yes |  |
| `display` | [`Spans`](/docs/reference/api/types#spans) | yes |  |
| `policy_hint` | string \| `null` |  |  |
| `reasons` | string[] | yes |  |
| `risk` | [`Risk`](/docs/reference/api/types#risk) | yes |  |

**Rust:** [`crates/nh-proto/src/api/rich.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/rich.rs#L79) · **Used by:** [`ApprovalPayload`](/docs/reference/api/types#approvalpayload)

### `ConfigGet` {#configget}

| Field | Type | Required | Description |
|---|---|---|---|
| `section` | string \| `null` |  | One section, for example `approvals`. Without it all sections. |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L94) · **Used by:** [`config.get`](/docs/reference/api/methods/config#config-get)

### `ConfigReload` {#configreload}

An object with no fields: `{}`.

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L112) · **Used by:** [`config.reload`](/docs/reference/api/methods/config#config-reload)

### `ConfigReloaded` {#configreloaded}

| Field | Type | Required | Description |
|---|---|---|---|
| `errors` | string[] | yes | Errors of the files. The old values stay in force. |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L114) · **Used by:** [`config.reload`](/docs/reference/api/methods/config#config-reload)

### `ConfigSet` {#configset}

Sets a value in `config.toml`. The file keeps its comments and its order. A `null` removes the key, so the default applies again.

| Field | Type | Required | Description |
|---|---|---|---|
| `key` | string | yes | The path, for example `approvals.mode`. |
| `value` | any | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L106) · **Used by:** [`config.set`](/docs/reference/api/methods/config#config-set)

### `ConfigValues` {#configvalues}

| Field | Type | Required | Description |
|---|---|---|---|
| `sections` | map of string to [`SettingsSection`](/docs/reference/api/types#settingssection) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L100) · **Used by:** [`config.get`](/docs/reference/api/methods/config#config-get)

### `CopyResult` {#copyresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `text` | string \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L204) · **Used by:** [`pane.copy`](/docs/reference/api/methods/pane#pane-copy), [`pane.clipboard_allow`](/docs/reference/api/methods/pane#pane-clipboard-allow), [`fs.copy_path`](/docs/reference/api/methods/fs#fs-copy-path)

### `Cursor` {#cursor}

A place in the event log. `log_epoch` changes when the log starts again, and a cursor of another epoch is stale.

| Field | Type | Required | Description |
|---|---|---|---|
| `log_epoch` | integer (uint64) | yes |  |
| `machine_uuid` | string | yes |  |
| `seq` | integer (uint64) | yes |  |
| `session_uuid` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L24) · **Used by:** [`EventsSubscribe`](/docs/reference/api/types#eventssubscribe), [`EventsSubscribed`](/docs/reference/api/types#eventssubscribed), [`EventsTruncated`](/docs/reference/api/types#eventstruncated)

## D {#letter-d}

### `DebugAnswer` {#debuganswer}

`debug.answer`: decides an interaction the way a client would, and may hold the delivery back so that a test can control the order of events.

| Field | Type | Required | Description |
|---|---|---|---|
| `by` | string |  | Default: `"debug"`. |
| `decision` | [`Decision`](/docs/reference/api/types#decision) | yes |  |
| `hold` | boolean |  | Record the decision and do not deliver it. A later `debug.deliver` does. Default: `false`. |
| `interaction` | [`RefInteractionId`](/docs/reference/api/types#refinteractionid) | yes |  |
| `via` | [`Via`](/docs/reference/api/types#via) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/internal.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/internal.rs#L146) · **Used by:** [`debug.answer`](/docs/reference/api/methods/debug#debug-answer)

### `DebugAnswered` {#debuganswered}

| Field | Type | Required | Description |
|---|---|---|---|
| `rev` | integer (uint32) | yes |  |
| `state` | [`InteractionState`](/docs/reference/api/types#interactionstate) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/internal.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/internal.rs#L157) · **Used by:** [`debug.answer`](/docs/reference/api/methods/debug#debug-answer)

### `DebugBundle` {#debugbundle}

`debug.bundle`: writes a zip for a bug report. Everything in it has passed the redaction.

| Field | Type | Required | Description |
|---|---|---|---|
| `include_scrollback` | boolean |  | Add the text of the panes. Without it the bundle has no scrollback. Default: `false`. |
| `out` | string \| `null` |  | The file to write. The CLI always gives one. Without it the core writes `neohar-debug-<time>.zip` in the directory of the logs. |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L360) · **Used by:** [`debug.bundle`](/docs/reference/api/methods/debug#debug-bundle)

### `DebugBundled` {#debugbundled}

| Field | Type | Required | Description |
|---|---|---|---|
| `bytes` | integer (uint64) | yes |  |
| `files` | integer (uint32) | yes |  |
| `path` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L370) · **Used by:** [`debug.bundle`](/docs/reference/api/methods/debug#debug-bundle)

### `DebugRestoredScreen` {#debugrestoredscreen}

The screen of a restored pane at the end of the replay.

| Field | Type | Required | Description |
|---|---|---|---|
| `dump` | any | yes | A `ScreenDump` of `nh-term`. |
| `fed_offset` | integer (uint64) | yes | The journal offset at the end of the replay. |

**Rust:** [`crates/nh-proto/src/api/calls/internal.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/internal.rs#L137) · **Used by:** [`DebugScreenResult`](/docs/reference/api/types#debugscreenresult)

### `DebugScreen` {#debugscreen}

`debug.screen`: what a pane shows, with the journal offset it belongs to. The pane can be one that `debug.spawn_panes` started, outside the layout.

| Field | Type | Required | Description |
|---|---|---|---|
| `dump` | boolean |  | Include the cells (`dump`). A call without them is cheap. Default: `true`. |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/internal.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/internal.rs#L102) · **Used by:** [`debug.screen`](/docs/reference/api/methods/debug#debug-screen)

### `DebugScreenResult` {#debugscreenresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `asked_size` | any[] | yes | The size that the last resize request asked for, as columns and rows. |
| `dump` | any |  | The cells, the cursor, the modes and the title (a `ScreenDump` of `nh-term`). Only with `dump`. |
| `fed_offset` | integer (uint64) | yes | The journal offset that the engine read up to. With `dump`, the offset of the dump. |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `recovery` | string | yes | How the pane got its screen: `fresh`, `snapshot`, `journal` or `ring_only`. |
| `restored` | [`DebugRestoredScreen`](/docs/reference/api/types#debugrestoredscreen) \| `null` |  | The screen at the end of the replay. Only with `dump`, and only for a pane that a restart restored. |
| `restoring` | boolean | yes | The pane reads the journal after a restart and shows no final screen yet. |
| `settled` | boolean | yes | The pane has read its journal and the resize nudge is over. A pane that was not restored is settled from the start. |
| `since_resize_ms` | integer (uint64) | yes | How long ago the engine last changed its size. |
| `size` | any[] | yes | The size of the engine, as columns and rows. |

**Rust:** [`crates/nh-proto/src/api/calls/internal.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/internal.rs#L109) · **Used by:** [`debug.screen`](/docs/reference/api/methods/debug#debug-screen)

### `DebugSpawnPanes` {#debugspawnpanes}

`debug.spawn_panes`: panes for a test, outside the layout. No window draws them. The panes start one after the other and run the same program.

| Field | Type | Required | Description |
|---|---|---|---|
| `argv` | string[] \| `null` |  | The program and its arguments. Without it, the `SHELL` of the core, else `/bin/sh`. |
| `cols` | integer (uint16) | yes |  |
| `count` | integer (uint32) |  | How many panes, from 1 to 64. Default: `1`. |
| `cwd` | string \| `null` |  |  |
| `env` | any[][] |  | Extra variables for the environment, as pairs of a name and a value. |
| `isolated_home` | boolean |  | Give each pane its own `HOME`, `CLAUDE_CONFIG_DIR` and `CODEX_HOME` in the data directory of the session, so that a harness never touches the real ones. Without `cwd`, the pane starts in that `HOME`. Default: `false`. |
| `rows` | integer (uint16) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/internal.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/internal.rs#L164) · **Used by:** [`debug.spawn_panes`](/docs/reference/api/methods/debug#debug-spawn-panes)

### `DebugSpawned` {#debugspawned}

| Field | Type | Required | Description |
|---|---|---|---|
| `panes` | [`DebugSpawnedPane`](/docs/reference/api/types#debugspawnedpane)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/internal.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/internal.rs#L192) · **Used by:** [`debug.spawn_panes`](/docs/reference/api/methods/debug#debug-spawn-panes)

### `DebugSpawnedPane` {#debugspawnedpane}

A pane that `debug.spawn_panes` started.

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `render_id` | integer (uint32) | yes | The number of the pane in the render frames. |

**Rust:** [`crates/nh-proto/src/api/calls/internal.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/internal.rs#L186) · **Used by:** [`DebugSpawned`](/docs/reference/api/types#debugspawned)

### `Decision` {#decision}

One of these forms:

#### `kind: "approval"`

| Field | Type | Required | Description |
|---|---|---|---|
| `kind` | `"approval"` | yes |  |

#### `kind: "question"`

| Field | Type | Required | Description |
|---|---|---|---|
| `kind` | `"question"` | yes |  |

#### `kind: "plan_review"`

| Field | Type | Required | Description |
|---|---|---|---|
| `kind` | `"plan_review"` | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L276) · **Used by:** [`DebugAnswer`](/docs/reference/api/types#debuganswer), [`InteractionAnswer`](/docs/reference/api/types#interactionanswer)

### `DeliveryInfo` {#deliveryinfo}

| Field | Type | Required | Description |
|---|---|---|---|
| `attempts` | integer (uint32) | yes | How many times the core tried to deliver. |
| `decided_by` | string \| `null` |  | Who decided: a client name, `policy`, or the id of a window. |
| `fail_reason` | string \| `null` |  | Why the delivery failed, when the state is `failed`. |
| `rev` | integer (uint32) | yes | The revision of the decision. |
| `via` | [`Via`](/docs/reference/api/types#via) \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L421) · **Used by:** [`InteractionValue`](/docs/reference/api/types#interactionvalue)

### `DeviceId` {#deviceid}

The ULID of a client device, as 26 characters.

Type: string. Pattern: `^[0-9A-HJKMNP-TV-Z]{26}$`.

**Used by:** [`PhoneEvent`](/docs/reference/api/types#phoneevent), [`PhonePairingValue`](/docs/reference/api/types#phonepairingvalue), [`PhonePushFailed`](/docs/reference/api/types#phonepushfailed), [`PhonePushTest`](/docs/reference/api/types#phonepushtest), [`PhonePushed`](/docs/reference/api/types#phonepushed), [`PhoneRemove`](/docs/reference/api/types#phoneremove), [`PhoneRename`](/docs/reference/api/types#phonerename), [`PhoneReplied`](/docs/reference/api/types#phonereplied), [`PhoneValue`](/docs/reference/api/types#phonevalue), [`WindowValue`](/docs/reference/api/types#windowvalue)

### `Diff` {#diff}

| Field | Type | Required | Description |
|---|---|---|---|
| `files` | [`FileDiff`](/docs/reference/api/types#filediff)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/rich.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/rich.rs#L66) · **Used by:** [`rich.diff`](/docs/reference/api/methods/rich#rich-diff), [`ApprovalPayload`](/docs/reference/api/types#approvalpayload), [`HermesItemBody`](/docs/reference/api/types#hermesitembody), [`TimelineBody`](/docs/reference/api/types#timelinebody)

### `DiffLine` {#diffline}

| Field | Type | Required | Description |
|---|---|---|---|
| `intra` | any[][] \| `null` |  | The changed ranges inside the line, in UTF-16 code units of `text`. |
| `kind` | [`LineKind`](/docs/reference/api/types#linekind) | yes |  |
| `new` | integer (uint32) \| `null` |  | The line number in the new file. A context line and an added line have one. |
| `old` | integer (uint32) \| `null` |  | The line number in the old file. A context line and a deleted line have one. |
| `text` | [`Spans`](/docs/reference/api/types#spans) | yes |  |

**Rust:** [`crates/nh-proto/src/api/rich.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/rich.rs#L31) · **Used by:** [`Hunk`](/docs/reference/api/types#hunk)

### `DiffStats` {#diffstats}

| Field | Type | Required | Description |
|---|---|---|---|
| `add` | integer (uint32) | yes |  |
| `del` | integer (uint32) | yes |  |

**Rust:** [`crates/nh-proto/src/api/rich.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/rich.rs#L51) · **Used by:** [`ChangesHead`](/docs/reference/api/types#changeshead), [`FileDiff`](/docs/reference/api/types#filediff), [`GitHead`](/docs/reference/api/types#githead), [`GitShown`](/docs/reference/api/types#gitshown)

### `DoctorCheck` {#doctorcheck}

| Field | Type | Required | Description |
|---|---|---|---|
| `detail` | string | yes |  |
| `hint` | string \| `null` |  | What the person can do about it. |
| `id` | string | yes | A stable name, for example `runtime_dir`. |
| `status` | [`CheckStatus`](/docs/reference/api/types#checkstatus) | yes |  |
| `title` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L343) · **Used by:** [`DoctorReport`](/docs/reference/api/types#doctorreport)

### `DoctorReport` {#doctorreport}

`doctor.run`: the checks of `nh doctor`. The same facts feed `health`.

| Field | Type | Required | Description |
|---|---|---|---|
| `checks` | [`DoctorCheck`](/docs/reference/api/types#doctorcheck)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L355) · **Used by:** [`doctor.run`](/docs/reference/api/methods/doctor#doctor-run)

### `DoctorRun` {#doctorrun}

An object with no fields: `{}`.

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L328) · **Used by:** [`doctor.run`](/docs/reference/api/methods/doctor#doctor-run)

### `DoctorWarning` {#doctorwarning}

| Field | Type | Required | Description |
|---|---|---|---|
| `hint` | string \| `null` |  | What the user can do, for example a command. |
| `id` | string | yes |  |
| `text` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L894) · **Used by:** [`HealthValue`](/docs/reference/api/types#healthvalue)

### `DroppedBlob` {#droppedblob}

A file that a client put in the blob store (`BlobPut`), with the name it had.

| Field | Type | Required | Description |
|---|---|---|---|
| `hash` | string | yes | The BLAKE3 digest in hex that the upload answered. |
| `name` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L120) · **Used by:** [`PaneDropBlobs`](/docs/reference/api/types#panedropblobs)

## E {#letter-e}

### `Empty` {#empty}

An empty result or empty params: `{}`.

An object with no fields: `{}`.

**Rust:** [`crates/nh-proto/src/api/common.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/common.rs#L34) · **Used by:** [`session.info`](/docs/reference/api/methods/session#session-info), [`machine.info`](/docs/reference/api/methods/machine#machine-info), [`machine.list`](/docs/reference/api/methods/machine#machine-list), [`machine.connect`](/docs/reference/api/methods/machine#machine-connect), [`machine.disconnect`](/docs/reference/api/methods/machine#machine-disconnect), [`machine.open_window`](/docs/reference/api/methods/machine#machine-open-window), [`session.shutdown`](/docs/reference/api/methods/session#session-shutdown), [`workspace.rename`](/docs/reference/api/methods/workspace#workspace-rename), [`workspace.close`](/docs/reference/api/methods/workspace#workspace-close), [`workspace.move`](/docs/reference/api/methods/workspace#workspace-move), [`group.rename`](/docs/reference/api/methods/group#group-rename), [`group.close`](/docs/reference/api/methods/group#group-close), [`group.move`](/docs/reference/api/methods/group#group-move), [`tab.rename`](/docs/reference/api/methods/tab#tab-rename), [`tab.close`](/docs/reference/api/methods/tab#tab-close), [`tab.move`](/docs/reference/api/methods/tab#tab-move), [`tab.select`](/docs/reference/api/methods/tab#tab-select), [`layout.set_ratios`](/docs/reference/api/methods/layout#layout-set-ratios), [`layout.equalize`](/docs/reference/api/methods/layout#layout-equalize), [`layout.zoom`](/docs/reference/api/methods/layout#layout-zoom), [`pane.close`](/docs/reference/api/methods/pane#pane-close), [`pane.focus`](/docs/reference/api/methods/pane#pane-focus), [`pane.claim`](/docs/reference/api/methods/pane#pane-claim), [`pane.move`](/docs/reference/api/methods/pane#pane-move), …

### `ErrorCode` {#errorcode}

The string code of an error. The codes are part of the contract, and this list is complete: the core, the hosts and the link send no other code. To send a new code, add it here first. The UI and the CLI act on a few of them (`already_answered`, `confirm_required`, `not_found`, `unsupported`). On the wire a code is its `snake_case` name.

| Value | Meaning |
|---|---|
| `"parse_error"` | The line is not JSON (-32700). |
| `"invalid_request"` | The JSON is not a request (-32600). |
| `"method_not_found"` | The method does not exist (-32601). |
| `"invalid_params"` | The params do not fit the method (-32602). |
| `"not_found"` | The thing that the call names does not exist. |
| `"pane_gone"` | The pane exists and has no process: its holder starts, the program ended before the core restarted, or the holder is lost. `pane.restart` starts the program again. |
| `"permission_denied"` | The caller may not call this method, or not on this thing. |
| `"already_answered"` | Another client answered the interaction first. `details` has `{by, via, decision}`. |
| `"input_unconfirmed"` | An input was in flight when the holder or the core went away. `details` has `{input_id, reason}`. |
| `"delivery_unknown"` | The core cannot say whether an answer reached the harness. |
| `"degraded_storage"` | The store is read-only or full. The core runs, but it does not keep changes. |
| `"confirm_required"` | The call needs a second call with `force`. `details` says what the user would lose. |
| `"stale_version"` | The caller worked with an older version of the thing. Read it again. |
| `"busy"` | The core is busy with the same work. Try again. |
| `"harness_unavailable"` | The harness of a start is not installed, or it is logged out after a new probe. `details` has `{harness, reason, hint}`, and `reason` is `not_installed` or `logged_out`. |
| `"unsupported"` | The method exists in the contract, and the core has no handler for it yet. |
| `"link_down"` | The link to the core of a machine is down. A call fails with it at once, and a call in flight when the link went down fails with it too: its outcome is unknown. |
| `"version_mismatch"` | The core of a machine has another major version of the API. |
| `"not_idle"` | `agent.reply` on a run that works or has an open interaction. `details` has `{execution, pending}`. |
| `"pane_busy"` | The pane runs a program or an agent, and the call needs an idle shell. |
| `"pane_spawn_failed"` | The core cannot start the holder or the program of a pane. |
| `"holder_error"` | The holder of a pane failed or sent something that the core does not accept. |
| `"agent_live"` | The agent still runs, and the call needs an agent that has ended. |
| `"agent_gone"` | The agent has ended, and the call needs a live agent. |
| `"not_bound"` | The run is not linked to its pane, so the core types nothing into it. |
| `"name_taken"` | A live agent has the name already. |
| `"harness_mismatch"` | The run in the pane is of another harness than the call names. |
| `"manifest_error"` | The manifest of a start has a value that is not correct. |
| `"interaction_closed"` | The interaction has ended. `details` has `{state}`. |
| `"not_answerable"` | No channel reaches the harness of the interaction, so nothing can answer it. |
| `"gate_unavailable"` | This core does not hold the gates of the hooks. |
| `"cancelled"` | The job of the call was cancelled. |
| `"timeout"` | The work of the call did not end in time. |
| `"too_large"` | The request, the file or the blob is larger than the limit. |
| `"blob_corrupt"` | The bytes of a blob do not agree with its hash. |
| `"conflict"` | A policy rule with the same id exists. |
| `"config_rule"` | The policy rule is in `config.toml`, so only a change of that file removes it. |
| `"git_failed"` | A command of git failed. |
| `"forward_failed"` | The core cannot forward the port of a machine. |
| `"proto_unsupported"` | The client decodes another version of the frame format. |
| `"io_error"` | The core cannot write a file. |
| `"store_error"` | The database failed. |
| `"internal"` | A fault of the core. The log has the reason. |

**Rust:** [`crates/nh-proto/src/api/envelope.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/envelope.rs#L123) · **Used by:** [`CallError`](/docs/reference/api/types#callerror), [`ErrorData`](/docs/reference/api/types#errordata)

### `ErrorData` {#errordata}

The `error.data` of an application error.

| Field | Type | Required | Description |
|---|---|---|---|
| `code` | [`ErrorCode`](/docs/reference/api/types#errorcode) | yes |  |
| `details` | any |  |  |

**Rust:** [`crates/nh-proto/src/api/envelope.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/envelope.rs#L353) · **Used by:** [`RpcError`](/docs/reference/api/types#rpcerror)

### `Event` {#event}

An entry of the log.

| Field | Type | Required | Description |
|---|---|---|---|
| `at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `kind` | string | yes | The kind, for example `agent.state_changed`. |
| `payload` | any | yes |  |
| `seq` | integer (uint64) | yes | The place in the log. It grows by one for each event. |
| `subject` | string \| `null` |  | The id of the main object, as text. |
| `v` | integer (uint16) | yes | The version of the payload. A payload only gets new fields within a version. |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L32) · **Used by:** [`EventNotification`](/docs/reference/api/types#eventnotification)

### `EventFilter` {#eventfilter}

Which events a subscription wants.

| Field | Type | Required | Description |
|---|---|---|---|
| `kinds` | string[] |  | Kinds. A trailing `*` is a prefix: `interaction.*`. Without it all kinds match. |
| `subject` | string \| `null` |  | Only events about this object. |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L48) · **Used by:** [`EventsSubscribe`](/docs/reference/api/types#eventssubscribe)

### `EventNotification` {#eventnotification}

The params of the notification `events.event`.

| Field | Type | Required | Description |
|---|---|---|---|
| `event` | [`Event`](/docs/reference/api/types#event) | yes |  |
| `subscription` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L43)

### `EventsSubscribe` {#eventssubscribe}

`events.subscribe {cursor, filter}`. Events arrive as `events.event` notifications. A client whose cursor is older than the retention, or from another `log_epoch`, gets `events.truncated` and takes a fresh snapshot.

| Field | Type | Required | Description |
|---|---|---|---|
| `cursor` | [`Cursor`](/docs/reference/api/types#cursor) \| `null` |  | Where to start. Without it the subscription starts at the end of the log. |
| `filter` | [`EventFilter`](/docs/reference/api/types#eventfilter) |  | Default: `{}`. |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L23) · **Used by:** [`events.subscribe`](/docs/reference/api/methods/events#events-subscribe)

### `EventsSubscribed` {#eventssubscribed}

| Field | Type | Required | Description |
|---|---|---|---|
| `cursor` | [`Cursor`](/docs/reference/api/types#cursor) | yes | The cursor of the end of the log at the moment of the call. |
| `subscription` | string | yes | The id of the subscription, as it is in the notifications. |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L31) · **Used by:** [`events.subscribe`](/docs/reference/api/methods/events#events-subscribe)

### `EventsTruncated` {#eventstruncated}

The params of the notification `events.truncated`.

| Field | Type | Required | Description |
|---|---|---|---|
| `cursor` | [`Cursor`](/docs/reference/api/types#cursor) | yes | The cursor to start from after a fresh snapshot. |
| `subscription` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L49)

### `EventsUnsubscribe` {#eventsunsubscribe}

| Field | Type | Required | Description |
|---|---|---|---|
| `subscription` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L38) · **Used by:** [`events.unsubscribe`](/docs/reference/api/methods/events#events-unsubscribe)

### `Execution` {#execution}

One of these forms:

#### `state: "starting"`

| Field | Type | Required | Description |
|---|---|---|---|
| `state` | `"starting"` | yes |  |

#### `state: "working"`

| Field | Type | Required | Description |
|---|---|---|---|
| `state` | `"working"` | yes |  |

#### `state: "idle"`

| Field | Type | Required | Description |
|---|---|---|---|
| `state` | `"idle"` | yes |  |

#### `state: "error"`

| Field | Type | Required | Description |
|---|---|---|---|
| `state` | `"error"` | yes |  |

#### `state: "rate_limited"`

| Field | Type | Required | Description |
|---|---|---|---|
| `resets_at` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  |  |
| `state` | `"rate_limited"` | yes |  |

#### `state: "exited"`

| Field | Type | Required | Description |
|---|---|---|---|
| `state` | `"exited"` | yes |  |

#### `state: "unknown"`

| Field | Type | Required | Description |
|---|---|---|---|
| `state` | `"unknown"` | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L224) · **Used by:** [`ExecutionFacet`](/docs/reference/api/types#executionfacet)

### `ExecutionFacet` {#executionfacet}

| Field | Type | Required | Description |
|---|---|---|---|
| `provenance` | [`Provenance`](/docs/reference/api/types#provenance) | yes |  |
| `value` | [`Execution`](/docs/reference/api/types#execution) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L258) · **Used by:** [`Facets`](/docs/reference/api/types#facets)

### `ExecutionId` {#executionid}

The ULID of a workflow execution, as 26 characters.

Type: string. Pattern: `^[0-9A-HJKMNP-TV-Z]{26}$`.

**Used by:** [`ExecutionValue`](/docs/reference/api/types#executionvalue), [`WorkflowContinue`](/docs/reference/api/types#workflowcontinue), [`WorkflowExecuted`](/docs/reference/api/types#workflowexecuted), [`WorkflowExecutionEvent`](/docs/reference/api/types#workflowexecutionevent), [`WorkflowExecutionRef`](/docs/reference/api/types#workflowexecutionref), [`WorkflowNodeEvent`](/docs/reference/api/types#workflownodeevent), [`WorkflowNodeRef`](/docs/reference/api/types#workflownoderef), [`WorkflowReportGet`](/docs/reference/api/types#workflowreportget), [`WorkflowReveal`](/docs/reference/api/types#workflowreveal)

### `ExecutionNodeValue` {#executionnodevalue}

One node of a `workflow_executions` row.

| Field | Type | Required | Description |
|---|---|---|---|
| `node` | string | yes | The id of the node in the graph of the execution. |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) \| `null` |  | The pane of that agent run, while the agent runs. |
| `reason` | string \| `null` |  | Why the node failed, or what it waits for, as a person reads it. |
| `report` | string \| `null` |  | The absolute path of the report of the node. |
| `resumable` | boolean |  | The agent of the node can start again with `workflow.continue`. Default: `false`. |
| `run` | [`RunId`](/docs/reference/api/types#runid) \| `null` |  | The agent run of the node, while it has one. |
| `state` | [`NodeState`](/docs/reference/api/types#nodestate) | yes |  |
| `time_text` | string \| `null` |  | The time of the node as a person reads it, for example "6 min". |
| `verdict` | [`Verdict`](/docs/reference/api/types#verdict) \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1372) · **Used by:** [`ExecutionValue`](/docs/reference/api/types#executionvalue)

### `ExecutionState` {#executionstate}

The state of an execution of a workflow.

| Value | Meaning |
|---|---|
| `"running"` | A node works, or can start. |
| `"paused"` | No node can start, and a node failed. Retry or skip the failed node, or cancel. |
| `"done"` | Each node is done or skipped. |
| `"cancelled"` | The person cancelled the execution. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1327) · **Used by:** [`ExecutionValue`](/docs/reference/api/types#executionvalue), [`WorkflowExecutionEvent`](/docs/reference/api/types#workflowexecutionevent)

### `ExecutionValue` {#executionvalue}

`workflow_executions`: one execution of a workflow. The key is the id. The core orders the rows, the newest first.

| Field | Type | Required | Description |
|---|---|---|---|
| `base` | string \| `null` |  | The base of the changes, for example `main`. |
| `branch` | string \| `null` |  |  |
| `dir` | string | yes | The folder of the files of the execution. |
| `ended_at` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  |  |
| `folder` | string | yes | The folder in which the agents work. |
| `head` | string \| `null` |  | The `HEAD` commit at the start, short. |
| `id` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |
| `label` | string | yes | The name of the execution, for example "Review 3". |
| `nodes` | [`ExecutionNodeValue`](/docs/reference/api/types#executionnodevalue)[] | yes | The nodes, in the order of the graph. |
| `number` | integer (uint32) | yes | The number of the execution of this workflow, from 1. |
| `only` | string \| `null` |  | The node of "Execute this node", when the execution has only that node. |
| `report` | string \| `null` |  | The path of `report.md`, when a Report node wrote it. |
| `started_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `state` | [`ExecutionState`](/docs/reference/api/types#executionstate) | yes |  |
| `time_text` | string | yes | The time and the length as a person reads it, for example "14:12 · 14 min". |
| `tree_changed` | boolean |  | The `HEAD` commit or the changes of the worktree are different from the start. Default: `false`. |
| `workflow` | string | yes | The name of the workflow file without `.toml`. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1400) · **Used by:** [projection `workflow_executions`](/docs/reference/api/projections#workflow-executions), [`WorkflowExecutionList`](/docs/reference/api/types#workflowexecutionlist), [`WorkflowExecutionShown`](/docs/reference/api/types#workflowexecutionshown)

## F {#letter-f}

### `Facet` {#facet}

Values: `"liveness"`, `"execution"`, `"pending"`, `"adapter_health"`.

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L61) · **Used by:** [`AdapterDisagreement`](/docs/reference/api/types#adapterdisagreement), [`AgentStateChanged`](/docs/reference/api/types#agentstatechanged)

### `Facets` {#facets}

The five facets of a run.

| Field | Type | Required | Description |
|---|---|---|---|
| `adapter_health` | [`HealthFacet`](/docs/reference/api/types#healthfacet) | yes |  |
| `execution` | [`ExecutionFacet`](/docs/reference/api/types#executionfacet) | yes |  |
| `liveness` | [`LivenessFacet`](/docs/reference/api/types#livenessfacet) | yes |  |
| `pending` | [`InteractionId`](/docs/reference/api/types#interactionid)[] | yes | The ids of the open interactions. |
| `read_state` | [`ReadState`](/docs/reference/api/types#readstate) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L269) · **Used by:** [`AgentValue`](/docs/reference/api/types#agentvalue)

### `FileAction` {#fileaction}

Values: `"created"`, `"modified"`, `"removed"`, `"unchanged"`.

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L248) · **Used by:** [`FileChange`](/docs/reference/api/types#filechange)

### `FileChange` {#filechange}

| Field | Type | Required | Description |
|---|---|---|---|
| `action` | [`FileAction`](/docs/reference/api/types#fileaction) | yes |  |
| `backup` | string \| `null` |  | The backup that the installer wrote before the first change. |
| `path` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L255) · **Used by:** [`IntegrationChanges`](/docs/reference/api/types#integrationchanges)

### `FileDiff` {#filediff}

| Field | Type | Required | Description |
|---|---|---|---|
| `hunks` | [`Hunk`](/docs/reference/api/types#hunk)[] | yes |  |
| `path` | string | yes |  |
| `stats` | [`DiffStats`](/docs/reference/api/types#diffstats) | yes |  |
| `status` | [`FileStatus`](/docs/reference/api/types#filestatus) | yes |  |
| `truncated` | boolean |  | The diff of this file was cut at 2,000 lines. The UI offers "open full". Default: `false`. |

**Rust:** [`crates/nh-proto/src/api/rich.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/rich.rs#L56) · **Used by:** [`changes.diff`](/docs/reference/api/methods/changes#changes-diff), [`git.diff`](/docs/reference/api/methods/git#git-diff), [`Diff`](/docs/reference/api/types#diff)

### `FileStatus` {#filestatus}

Values: `"modified"`, `"added"`, `"deleted"`, `"renamed"`.

**Rust:** [`crates/nh-proto/src/api/rich.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/rich.rs#L16) · **Used by:** [`FileDiff`](/docs/reference/api/types#filediff)

### `FloatingPane` {#floatingpane}

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `rect` | [`Rect`](/docs/reference/api/types#rect) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L114) · **Used by:** [`TabValue`](/docs/reference/api/types#tabvalue)

### `FsCopyPath` {#fscopypath}

`fs.copy_path`: the core writes a path to the clipboard of the device that called.

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | An absolute path. |
| `relative_to` | string \| `null` |  | A folder that holds `path`. The clipboard gets the path from this folder. |
| `return_text` | boolean |  | Return the text, and write no clipboard of a host: a phone writes it in the tap. Default: `false`. |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L423) · **Used by:** [`fs.copy_path`](/docs/reference/api/methods/fs#fs-copy-path)

### `FsEntry` {#fsentry}

| Field | Type | Required | Description |
|---|---|---|---|
| `hidden` | boolean | yes | The name starts with a dot. The tree shows the entry dim. |
| `icon` | string | yes | The name of the icon of the entry in the Material Icon Theme, by its name: `folder` and `file` when no other icon fits. A folder has the name of its closed icon. |
| `kind` | [`FsEntryKind`](/docs/reference/api/types#fsentrykind) | yes |  |
| `link` | boolean | yes | The entry is a symlink. `kind` is the kind of its target. |
| `name` | string | yes |  |
| `path` | string | yes | The absolute path, for `fs.list`, `fs.read` and the actions of a row. |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L348) · **Used by:** [`FsListResult`](/docs/reference/api/types#fslistresult)

### `FsEntryKind` {#fsentrykind}

| Value | Meaning |
|---|---|
| `"dir"` | A folder, or a symlink to a folder. It opens in the tree. |
| `"file"` | A regular file, or a symlink to one. It opens in the file view. |
| `"other"` | A socket, a pipe, a device or a broken link. The tree shows it and opens nothing. |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L364) · **Used by:** [`FsEntry`](/docs/reference/api/types#fsentry)

### `FsList` {#fslist}

`fs.list`: the entries of a folder of the machine, for the Files tab.

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | An absolute path. |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L333) · **Used by:** [`fs.list`](/docs/reference/api/methods/fs#fs-list)

### `FsListResult` {#fslistresult}

The entries of a folder in the order of the tree: the folders first, then the other entries, each group by name without case. `.git` and `.DS_Store` are not there.

| Field | Type | Required | Description |
|---|---|---|---|
| `entries` | [`FsEntry`](/docs/reference/api/types#fsentry)[] | yes |  |
| `path` | string | yes | The folder that the core listed, as an absolute path. |
| `truncated` | boolean | yes | True when the folder has more entries than the core sends (2,000). |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L340) · **Used by:** [`fs.list`](/docs/reference/api/methods/fs#fs-list)

### `FsOpen` {#fsopen}

`fs.open`: the host of the window opens a path, or shows it in the Finder.

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | An absolute path. |
| `reveal` | boolean |  | Show the path in the Finder, and do not open it. Default: `false`. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L413) · **Used by:** [`fs.open`](/docs/reference/api/methods/fs#fs-open)

### `FsPreviewKind` {#fspreviewkind}

| Value | Meaning |
|---|---|
| `"text"` | Lines of text with their highlight. |
| `"image"` | An image that the page shows from `image`. |
| `"binary"` | Bytes that are not text. The view shows only `info`. |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L381) · **Used by:** [`FsReadResult`](/docs/reference/api/types#fsreadresult)

### `FsRead` {#fsread}

`fs.read`: the preview of a regular file of the machine.

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | An absolute path. |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L374) · **Used by:** [`fs.read`](/docs/reference/api/methods/fs#fs-read)

### `FsReadResult` {#fsreadresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `icon` | string | yes | The icon of the file, as in `FsEntry`. |
| `image` | string \| `null` |  | A `data:` URL of an image of at most 8 MiB. |
| `info` | string | yes | The text of the head of the file view, for example "120 lines · 4.2 KB". |
| `kind` | [`FsPreviewKind`](/docs/reference/api/types#fspreviewkind) | yes |  |
| `lines` | [`Spans`](/docs/reference/api/types#spans)[] | yes | One line of text for each line of the file, with its highlight. Empty for an image and for a binary file. |
| `markdown` | [`Markdown`](/docs/reference/api/types#markdown) \| `null` |  | The HTML of a Markdown file of at most 256 KiB. |
| `name` | string | yes | The last part of the path. |
| `path` | string | yes |  |
| `truncated` | boolean | yes | The file has more than the core sends: 1 MiB or 10,000 lines. |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L390) · **Used by:** [`fs.read`](/docs/reference/api/methods/fs#fs-read)

## G {#letter-g}

### `GateMode` {#gatemode}

Whether the core holds the hook of the harness.

Values: `"gate"`, `"observe"`.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L408) · **Used by:** [`InteractionValue`](/docs/reference/api/types#interactionvalue)

### `GateReply` {#gatereply}

The first reply to `adapter.gate` and to `adapter.gate_resume`.

One of these forms:

#### Form 1

The core opened an interaction, and the hook waits. A `adapter.gate_result` notification follows on the same connection.

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `lease` | string | yes | The lease of this connection. |
| `wait` | boolean | yes |  |

#### Form 2

The decision is known at once (policy, or a decision recorded before a resume).

| Field | Type | Required | Description |
|---|---|---|---|
| `idempotency_key` | string | yes |  |
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `output` | string | yes | What the shim prints on stdout. The shim does not read it. |

#### Form 3

The hook prints nothing, and the harness shows its own dialog.

| Field | Type | Required | Description |
|---|---|---|---|
| `no_decision` | boolean | yes |  |
| `reason` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/internal.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/internal.rs#L48) · **Used by:** [`adapter.gate`](/docs/reference/api/methods/adapter#adapter-gate), [`adapter.gate_resume`](/docs/reference/api/methods/adapter#adapter-gate-resume)

### `GateResult` {#gateresult}

The params of the notification `adapter.gate_result`: the end of a wait.

One of these forms:

#### Form 1

| Field | Type | Required | Description |
|---|---|---|---|
| `idempotency_key` | string | yes |  |
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `output` | string | yes |  |

#### Form 2

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `no_decision` | boolean | yes |  |
| `reason` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/internal.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/internal.rs#L73)

### `GitAgent` {#gitagent}

An agent whose pane folder is in a worktree.

| Field | Type | Required | Description |
|---|---|---|---|
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `name` | string \| `null` |  |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) | yes |  |
| `state` | [`GitAgentState`](/docs/reference/api/types#gitagentstate) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L998) · **Used by:** [`GitHead`](/docs/reference/api/types#githead), [`GitWorktreeValue`](/docs/reference/api/types#gitworktreevalue)

### `GitAgentState` {#gitagentstate}

The state of an agent in a worktree, as the Git view shows it.

One of these forms:

- `"starting"` | `"working"` | `"idle"` | `"exited"` | `"unknown"`

- `"waiting"` — The agent has an open approval, question or plan.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L964) · **Used by:** [`GitAgent`](/docs/reference/api/types#gitagent)

### `GitBranchKind` {#gitbranchkind}

Values: `"local"`, `"remote"`.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1113) · **Used by:** [`GitBranchValue`](/docs/reference/api/types#gitbranchvalue)

### `GitBranchList` {#gitbranchlist}

The rows of `git_branches:<window>` for a worktree, in their order.

| Field | Type | Required | Description |
|---|---|---|---|
| `branches` | [`GitBranchValue`](/docs/reference/api/types#gitbranchvalue)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L101) · **Used by:** [`git.branches`](/docs/reference/api/methods/git#git-branches)

### `GitBranchValue` {#gitbranchvalue}

`git_branches:<window>`: a branch of the repository of the Git view. The key is `reference`. The branch of `HEAD` comes first, then the recent ones, then the other local ones and then the remote ones, each the newest commit first.

| Field | Type | Required | Description |
|---|---|---|---|
| `ahead` | integer (uint32) |  | Default: `0`. |
| `behind` | integer (uint32) |  | Default: `0`. |
| `current` | boolean |  | The worktree of the Git view has it out. Default: `false`. |
| `kind` | [`GitBranchKind`](/docs/reference/api/types#gitbranchkind) | yes |  |
| `name` | string | yes | For example `main` or `origin/main`. |
| `recent` | boolean |  | The worktree had it out a short time ago. Default: `false`. |
| `reference` | string | yes | For example `refs/heads/main`. |
| `subject` | string \| `null` |  | The subject of its last commit. |
| `time` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  | The time of its last commit. |
| `time_text` | string \| `null` |  | For example "3 days ago". |
| `upstream` | string \| `null` |  | The upstream of a local branch, for example `origin/main`. |
| `worktree` | string \| `null` |  | Another worktree has the branch out, so a switch to it goes to that worktree. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1121) · **Used by:** [projection `git_branches`](/docs/reference/api/projections#git-branches), [`GitBranchList`](/docs/reference/api/types#gitbranchlist)

### `GitChanges` {#gitchanges}

What a switch does with the changes of the worktree.

| Value | Meaning |
|---|---|
| `"bring"` | The changes go to the other branch, as `git switch` takes them. A change that the other branch would overwrite stops the switch (`conflict`). |
| `"leave"` | The changes go to a stash that names the branch. A switch back to that branch offers them again (`stash` of `git_head`). |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L164) · **Used by:** [`GitSwitch`](/docs/reference/api/types#gitswitch)

### `GitCommit` {#gitcommit}

A commit of the History of the Git view.

| Field | Type | Required | Description |
|---|---|---|---|
| `author` | string | yes |  |
| `branches` | string[] |  | The branches at the commit, for example `main` and `origin/main`. |
| `hash` | string | yes |  |
| `head` | boolean |  | `HEAD` is at this commit. Default: `false`. |
| `short` | string | yes |  |
| `subject` | string | yes |  |
| `tags` | string[] |  |  |
| `time` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes | The time of the author. |
| `time_text` | string | yes | For example "3 hours ago". |
| `unpushed` | boolean |  | No remote branch has the commit, so a push would send it. Always false in a repository with no remote. Default: `false`. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1155) · **Used by:** [`GitLogResult`](/docs/reference/api/types#gitlogresult), [`GitShown`](/docs/reference/api/types#gitshown)

### `GitDiff` {#gitdiff}

`git.diff`: the diff of one file. Without `commit`, the file is a row of the changes of the worktree, from `HEAD`. With `commit`, it is a file of that commit, from its first parent.

| Field | Type | Required | Description |
|---|---|---|---|
| `commit` | string \| `null` |  |  |
| `file` | string | yes | The path of the file from the root of the worktree. |
| `path` | string \| `null` |  | The worktree that holds this absolute path. The CLI gives the current directory. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  | The worktree that the Git view of this window shows. |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L238) · **Used by:** [`git.diff`](/docs/reference/api/methods/git#git-diff)

### `GitFetch` {#gitfetch}

`git.fetch`: fetches the remote of the head with `--prune`, as a job. A question of ssh or of a credential helper shows the sheet of the app.

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string \| `null` |  | The worktree that holds this absolute path. The CLI gives the current directory. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  | The worktree that the Git view of this window shows. |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L213) · **Used by:** [`git.fetch`](/docs/reference/api/methods/git#git-fetch)

### `GitFollow` {#gitfollow}

`git.follow`: the Git view of a window shows the git worktree that holds `path`. The core fills `git_head:<window>`, `git_worktrees:<window>`, `git_branches:<window>` and `git_changes:<window>` while the window follows one of them, and looks again when the worktree or its git directory changes.

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | An absolute path in the worktree: the folder of a pane, or a row of `git_worktrees`. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L36) · **Used by:** [`git.follow`](/docs/reference/api/methods/git#git-follow)

### `GitHead` {#githead}

`git_head:<window>`: what the Git view of a window shows. The window names a folder with `git.follow`, and the head is that of the git worktree that holds it. The core makes the row when it knows the worktree.

| Field | Type | Required | Description |
|---|---|---|---|
| `agents` | [`GitAgent`](/docs/reference/api/types#gitagent)[] |  | The agents whose pane folder is in the worktree. |
| `ahead` | integer (uint32) |  | The commits of the branch that its upstream does not have. Default: `0`. |
| `behind` | integer (uint32) |  | The commits of the upstream that the branch does not have. Default: `0`. |
| `branch` | string \| `null` |  | The branch of `HEAD`, or `None` for a detached `HEAD`. |
| `changes` | [`ChangeTotals`](/docs/reference/api/types#changetotals) \| `null` |  | The totals of the last count of the changes, or `None` before the first count. |
| `commit` | string \| `null` |  | The short hash of `HEAD`, or `None` before the first commit. |
| `diff_stats` | [`DiffStats`](/docs/reference/api/types#diffstats) \| `null` |  | The lines of all changed files. |
| `error` | string \| `null` |  | Why the last look at the branches and the worktrees failed. The other values are those of the look before. |
| `fetch_note` | string \| `null` |  | Why the last fetch in the background failed, for example "The fetch needs your passphrase. Click Fetch.". A fetch that works clears it. |
| `fetched_at` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  | The time of the last fetch, from `FETCH_HEAD`. |
| `fetched_text` | string \| `null` |  | For example "Last fetched 18 minutes ago" or "Never fetched". `None` without a remote. |
| `folder` | string | yes | The folder that the window named. |
| `job` | [`GitJob`](/docs/reference/api/types#gitjob) \| `null` |  | The fetch or the pull that runs in the repository. |
| `remote` | string \| `null` |  | The remote of a fetch: the remote of the upstream, else `origin`, else the first one. |
| `repo` | string \| `null` |  | The key of the repository in `git_repos`: its common git directory. |
| `repo_name` | string \| `null` |  |  |
| `stash` | [`GitStash`](/docs/reference/api/types#gitstash) \| `null` |  | The stash that a switch left for the branch of `HEAD`, the newest one. |
| `trust` | [`GitTrustAsk`](/docs/reference/api/types#gittrustask) \| `null` |  | The core runs no git command in the repository until the person trusts it. Then the head has the worktree and the repository, and no branch, no count and no rows. |
| `upstream` | string \| `null` |  | The upstream of the branch, for example `origin/main`. |
| `upstream_gone` | boolean |  | The upstream is gone from its remote. Default: `false`. |
| `worktree` | string \| `null` |  | The root of the worktree that holds the folder, or `None` when the folder is not in a git worktree. |
| `worktree_name` | string \| `null` |  | The last part of the root of the worktree. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1010) · **Used by:** [projection `git_head`](/docs/reference/api/projections#git-head), [`GitStatus`](/docs/reference/api/types#gitstatus)

### `GitJob` {#gitjob}

The job of `git.fetch` or `git.pull` that runs. The `jobs` projection has its progress.

| Field | Type | Required | Description |
|---|---|---|---|
| `job` | [`JobId`](/docs/reference/api/types#jobid) | yes |  |
| `kind` | [`GitJobKind`](/docs/reference/api/types#gitjobkind) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L983) · **Used by:** [`GitHead`](/docs/reference/api/types#githead)

### `GitJobKind` {#gitjobkind}

A long command of git that runs in the worktree of a Git view.

Values: `"fetch"`, `"pull"`.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L977) · **Used by:** [`GitJob`](/docs/reference/api/types#gitjob)

### `GitLog` {#gitlog}

`git.log`: the commits of `HEAD`, the newest first, in pages.

| Field | Type | Required | Description |
|---|---|---|---|
| `limit` | integer (uint32) |  | The most commits of the page, at most 200. Default: `200`. |
| `path` | string \| `null` |  | The worktree that holds this absolute path. The CLI gives the current directory. |
| `skip` | integer (uint32) |  | The commits to leave out at the start: the number that the pages before had. Default: `0`. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  | The worktree that the Git view of this window shows. |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L106) · **Used by:** [`git.log`](/docs/reference/api/methods/git#git-log)

### `GitLogResult` {#gitlogresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `commits` | [`GitCommit`](/docs/reference/api/types#gitcommit)[] | yes |  |
| `head` | string \| `null` |  | The commit of `HEAD` when the page was read. Another value than the page before means that the history changed: read it again from the start. |
| `more` | boolean | yes | More commits follow this page. |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L121) · **Used by:** [`git.log`](/docs/reference/api/methods/git#git-log)

### `GitPlace` {#gitplace}

The worktree of a read: the one that the Git view of `window` follows, or the one that holds `path`.

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string \| `null` |  | The worktree that holds this absolute path. The CLI gives the current directory. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  | The worktree that the Git view of this window shows. |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L74) · **Used by:** [`git.status`](/docs/reference/api/methods/git#git-status), [`git.worktrees`](/docs/reference/api/methods/git#git-worktrees), [`git.branches`](/docs/reference/api/methods/git#git-branches)

### `GitPull` {#gitpull}

`git.pull`: moves the branch to its upstream with a fast-forward only, as a job. A branch that has commits of its own and is behind is `conflict` with `{"kind": "diverged"}`.

| Field | Type | Required | Description |
|---|---|---|---|
| `force` | boolean |  | Pull also when an agent works in the worktree. Default: `false`. |
| `path` | string \| `null` |  | The worktree that holds this absolute path. The CLI gives the current directory. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  | The worktree that the Git view of this window shows. |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L224) · **Used by:** [`git.pull`](/docs/reference/api/methods/git#git-pull)

### `GitRepoAdd` {#gitrepoadd}

`git.repo_add`: `git_repos` keeps the repository that holds `path`, also when no workspace and no pane is in it.

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | An absolute path in a git worktree. |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L44) · **Used by:** [`git.repo_add`](/docs/reference/api/methods/git#git-repo-add)

### `GitRepoAdded` {#gitrepoadded}

The payload of `git.repo_added`: `git_repos` keeps a repository that the person added.

| Field | Type | Required | Description |
|---|---|---|---|
| `common_dir` | string | yes | The common git directory of the repository. |
| `path` | string | yes | The path that the person gave. |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L275) · **Used by:** [event `git.repo_added`](/docs/reference/api/events#git-repo-added)

### `GitRepoList` {#gitrepolist}

The rows of `git_repos`, in their order.

| Field | Type | Required | Description |
|---|---|---|---|
| `repos` | [`GitRepoValue`](/docs/reference/api/types#gitrepovalue)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L84) · **Used by:** [`git.repos`](/docs/reference/api/methods/git#git-repos)

### `GitRepoSource` {#gitreposource}

Where the core found a repository of `git_repos`.

| Value | Meaning |
|---|---|
| `"workspace"` | The root of a workspace is in it. |
| `"pane"` | The folder of a pane is in it. |
| `"added"` | The person added it (`git.repo_add`). |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L937) · **Used by:** [`GitRepoValue`](/docs/reference/api/types#gitrepovalue)

### `GitRepoValue` {#gitrepovalue}

`git_repos`: a git repository of this machine. The key is `common_dir`. The core fills it only while a client follows it. The rows with agents come first, then by name.

| Field | Type | Required | Description |
|---|---|---|---|
| `agents` | integer (uint32) | yes | The live agents whose pane folder is in a worktree of the repository. |
| `common_dir` | string | yes | The git directory that the worktrees of the repository share. |
| `name` | string | yes | The name of the folder of the main worktree. |
| `root` | string | yes | The worktree that the Git view opens: the main one, or the one where the core found the repository when the main one has no files (a bare repository). |
| `source` | [`GitRepoSource`](/docs/reference/api/types#gitreposource) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L948) · **Used by:** [projection `git_repos`](/docs/reference/api/projections#git-repos), [`GitRepoList`](/docs/reference/api/types#gitrepolist)

### `GitShow` {#gitshow}

`git.show`: one commit with its message and its files.

| Field | Type | Required | Description |
|---|---|---|---|
| `commit` | string | yes | The full or the short hash of the commit. |
| `path` | string \| `null` |  | The worktree that holds this absolute path. The CLI gives the current directory. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  | The worktree that the Git view of this window shows. |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L132) · **Used by:** [`git.show`](/docs/reference/api/methods/git#git-show)

### `GitShown` {#gitshown}

| Field | Type | Required | Description |
|---|---|---|---|
| `body` | string | yes | The message after the subject, without the blank line between them. |
| `commit` | [`GitCommit`](/docs/reference/api/types#gitcommit) | yes |  |
| `email` | string | yes | The e-mail address of the author. |
| `files` | [`ChangeRow`](/docs/reference/api/types#changerow)[] | yes | The changed files, in the byte order of the path. `stamp` is 0. |
| `parents` | string[] | yes | The short hashes of the parents. A merge has two or more. The files are the changes from the first parent. |
| `stats` | [`DiffStats`](/docs/reference/api/types#diffstats) | yes | The lines of all files. |
| `truncated` | boolean |  | The commit has more than 1,000 files, and `files` has the first 1,000. Default: `false`. |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L143) · **Used by:** [`git.show`](/docs/reference/api/methods/git#git-show)

### `GitStash` {#gitstash}

A stash that a switch with `leave` made for a branch.

| Field | Type | Required | Description |
|---|---|---|---|
| `branch` | string | yes |  |
| `commit` | string | yes | The commit of the stash, for `git.unstash`. |
| `time_text` | string | yes | For example "3 hours ago". |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L989) · **Used by:** [`GitHead`](/docs/reference/api/types#githead)

### `GitStatus` {#gitstatus}

`git.status`: the head and the changed files of a worktree, as `git_head:<window>` and `git_changes:<window>` have them.

| Field | Type | Required | Description |
|---|---|---|---|
| `changes` | [`ChangeRow`](/docs/reference/api/types#changerow)[] | yes |  |
| `head` | [`GitHead`](/docs/reference/api/types#githead) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L90) · **Used by:** [`git.status`](/docs/reference/api/methods/git#git-status)

### `GitSwitch` {#gitswitch}

`git.switch`: the worktree goes to another branch. A worktree with changes needs `changes`, else the answer is `confirm_required` with `{"kind": "changes", "files"}`. A branch that another worktree has out is `conflict` with `{"kind": "worktree", "worktree"}`.

| Field | Type | Required | Description |
|---|---|---|---|
| `branch` | string | yes | A branch of `git_branches`: its reference (`refs/heads/main`) or its name (`main`, `origin/topic`). A remote branch with no local branch of the same name gets a local branch that tracks it. |
| `changes` | [`GitChanges`](/docs/reference/api/types#gitchanges) \| `null` |  | What happens to the changes of the worktree: `bring` or `leave`. A worktree with changes needs it. |
| `force` | boolean |  | Switch also when an agent works in the worktree. Default: `false`. |
| `path` | string \| `null` |  | The worktree that holds this absolute path. The CLI gives the current directory. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  | The worktree that the Git view of this window shows. |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L176) · **Used by:** [`git.switch`](/docs/reference/api/methods/git#git-switch)

### `GitTrust` {#gittrust}

`git.trust`: the person trusts the git repository that holds `path`. The core then runs git in it and in its worktrees. It keeps the settings of the repository that name a program, and asks again when they change. A trust again keeps the settings of now.

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | An absolute path in a git worktree, for example `path` of `trust`. |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L52) · **Used by:** [`git.trust`](/docs/reference/api/methods/git#git-trust)

### `GitTrustAsk` {#gittrustask}

`trust` of `changes_head` and `git_head`: the settings of a repository can name programs that git runs, so the core runs git in a repository only after the person trusts it (`git.trust`).

| Field | Type | Required | Description |
|---|---|---|---|
| `changed` | string[] |  | What changed after the trust: the keys of the settings, for example `core.fsmonitor`, and the hook files, for example `hooks/post-checkout`. |
| `common_dir` | string | yes | The common git directory: the key of the trust, which its worktrees share. |
| `path` | string | yes | The root of the worktree that holds the folder. `git.trust` takes it. |
| `reason` | [`GitTrustReason`](/docs/reference/api/types#gittrustreason) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L662) · **Used by:** [`ChangesHead`](/docs/reference/api/types#changeshead), [`GitHead`](/docs/reference/api/types#githead)

### `GitTrustEvent` {#gittrustevent}

The payload of `git.trusted` and `git.untrusted`: the person trusts a repository, or takes the trust back.

| Field | Type | Required | Description |
|---|---|---|---|
| `common_dir` | string | yes | The common git directory of the repository: the key of the trust. |
| `path` | string | yes | The path that the person gave. |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L284) · **Used by:** [event `git.trusted`](/docs/reference/api/events#git-trusted), [event `git.untrusted`](/docs/reference/api/events#git-untrusted)

### `GitTrustReason` {#gittrustreason}

Why the core runs no git command in a repository.

| Value | Meaning |
|---|---|
| `"new"` | The person did not trust the repository, or took the trust back. |
| `"changed"` | A setting of the repository in the fingerprint of the trust, or a hook file, changed after the trust. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L650) · **Used by:** [`GitTrustAsk`](/docs/reference/api/types#gittrustask)

### `GitTrustResult` {#gittrustresult}

The repository of `git.trust` or `git.untrust`.

| Field | Type | Required | Description |
|---|---|---|---|
| `common_dir` | string | yes | The common git directory: the key of the trust, which its worktrees share. |
| `root` | string | yes | The root of the worktree that holds the path. |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L65) · **Used by:** [`git.trust`](/docs/reference/api/methods/git#git-trust), [`git.untrust`](/docs/reference/api/methods/git#git-untrust)

### `GitUnstash` {#gitunstash}

`git.unstash`: puts the changes of a stash back into the worktree, and drops the stash.

| Field | Type | Required | Description |
|---|---|---|---|
| `force` | boolean |  | Put the changes back also when an agent works in the worktree. Default: `false`. |
| `path` | string \| `null` |  | The worktree that holds this absolute path. The CLI gives the current directory. |
| `stash` | string | yes | The commit of the stash, as `stash` of `git_head` gives it. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  | The worktree that the Git view of this window shows. |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L197) · **Used by:** [`git.unstash`](/docs/reference/api/methods/git#git-unstash)

### `GitUntrust` {#gituntrust}

`git.untrust`: the person takes back the trust of the git repository that holds `path`. The core runs no git command in it until the person trusts it again.

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | An absolute path in a git worktree. |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L59) · **Used by:** [`git.untrust`](/docs/reference/api/methods/git#git-untrust)

### `GitWorktreeList` {#gitworktreelist}

The rows of `git_worktrees:<window>` for a worktree, in their order.

| Field | Type | Required | Description |
|---|---|---|---|
| `worktrees` | [`GitWorktreeValue`](/docs/reference/api/types#gitworktreevalue)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/git.rs#L96) · **Used by:** [`git.worktrees`](/docs/reference/api/methods/git#git-worktrees)

### `GitWorktreeValue` {#gitworktreevalue}

`git_worktrees:<window>`: a worktree of the repository of the Git view. The key is `path`. The main worktree comes first, then the others in the order of git.

| Field | Type | Required | Description |
|---|---|---|---|
| `agents` | [`GitAgent`](/docs/reference/api/types#gitagent)[] |  |  |
| `branch` | string \| `null` |  | The branch of the worktree, or `None` for a detached `HEAD`. |
| `commit` | string \| `null` |  | The short hash of its `HEAD`. |
| `current` | boolean |  | The worktree that the Git view shows. Default: `false`. |
| `dirty` | integer (uint32) \| `null` |  | The files that are not committed, or `None` when the core could not count them. |
| `locked` | boolean |  | Default: `false`. |
| `main` | boolean |  | The main worktree of the repository. Default: `false`. |
| `name` | string | yes | The last part of the path. |
| `path` | string | yes |  |
| `prunable` | boolean |  | The folder of the worktree is gone, and `git worktree prune` would remove it. Default: `false`. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1083) · **Used by:** [projection `git_worktrees`](/docs/reference/api/projections#git-worktrees), [`GitWorktreeList`](/docs/reference/api/types#gitworktreelist)

### `GitWrite` {#gitwrite}

The payload of `git.switched`, `git.unstashed`, `git.fetched` and `git.pulled`: a write of the Git view that worked. A write that failed appends no event.

| Field | Type | Required | Description |
|---|---|---|---|
| `agents` | [`RunId`](/docs/reference/api/types#runid)[] |  | The agents in the worktree at that time. |
| `background` | boolean |  | The fetch ran in the background, not from a click. Default: `false`. |
| `command` | string | yes | The git command, for example `git switch --end-of-options main`. |
| `from` | string \| `null` |  | The branch before a switch. |
| `path` | string | yes | The worktree, or the common git directory for a fetch. |
| `stash` | string \| `null` |  | The stash that a switch made or that `git.unstash` put back. |
| `to` | string \| `null` |  | The branch after a switch or a pull. |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L293) · **Used by:** [event `git.switched`](/docs/reference/api/events#git-switched), [event `git.unstashed`](/docs/reference/api/events#git-unstashed), [event `git.fetched`](/docs/reference/api/events#git-fetched), [event `git.pulled`](/docs/reference/api/events#git-pulled)

### `GroupClose` {#groupclose}

Closing a group keeps its workspaces. They have no group afterwards.

| Field | Type | Required | Description |
|---|---|---|---|
| `group` | [`RefGroupId`](/docs/reference/api/types#refgroupid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L170) · **Used by:** [`group.close`](/docs/reference/api/methods/group#group-close)

### `GroupCreate` {#groupcreate}

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L156) · **Used by:** [`group.create`](/docs/reference/api/methods/group#group-create)

### `GroupCreated` {#groupcreated}

| Field | Type | Required | Description |
|---|---|---|---|
| `group` | [`GroupId`](/docs/reference/api/types#groupid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L160) · **Used by:** [`group.create`](/docs/reference/api/methods/group#group-create)

### `GroupEvent` {#groupevent}

The payload of the `group.*` events.

| Field | Type | Required | Description |
|---|---|---|---|
| `group` | [`GroupId`](/docs/reference/api/types#groupid) | yes |  |
| `name` | string \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L208) · **Used by:** [event `group.created`](/docs/reference/api/events#group-created), [event `group.renamed`](/docs/reference/api/events#group-renamed), [event `group.moved`](/docs/reference/api/events#group-moved), [event `group.closed`](/docs/reference/api/events#group-closed)

### `GroupId` {#groupid}

The ULID of a group of workspaces, as 26 characters.

Type: string. Pattern: `^[0-9A-HJKMNP-TV-Z]{26}$`.

**Used by:** [`GroupCreated`](/docs/reference/api/types#groupcreated), [`GroupEvent`](/docs/reference/api/types#groupevent), [`GroupRef`](/docs/reference/api/types#groupref), [`GroupSummary`](/docs/reference/api/types#groupsummary), [`WorkspaceSummary`](/docs/reference/api/types#workspacesummary)

### `GroupList` {#grouplist}

An object with no fields: `{}`.

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L181) · **Used by:** [`group.list`](/docs/reference/api/methods/group#group-list)

### `GroupListResult` {#grouplistresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `groups` | [`GroupSummary`](/docs/reference/api/types#groupsummary)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L189) · **Used by:** [`group.list`](/docs/reference/api/methods/group#group-list)

### `GroupMove` {#groupmove}

| Field | Type | Required | Description |
|---|---|---|---|
| `before` | [`RefGroupId`](/docs/reference/api/types#refgroupid) \| `null` |  | Put it before this group. Without it the group goes to the end. |
| `group` | [`RefGroupId`](/docs/reference/api/types#refgroupid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L174) · **Used by:** [`group.move`](/docs/reference/api/methods/group#group-move)

### `GroupRef` {#groupref}

The group of a workspace in the sidebar.

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | [`GroupId`](/docs/reference/api/types#groupid) | yes |  |
| `name` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L67) · **Used by:** [`WorkspaceValue`](/docs/reference/api/types#workspacevalue)

### `GroupRename` {#grouprename}

| Field | Type | Required | Description |
|---|---|---|---|
| `group` | [`RefGroupId`](/docs/reference/api/types#refgroupid) | yes |  |
| `name` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L164) · **Used by:** [`group.rename`](/docs/reference/api/methods/group#group-rename)

### `GroupShow` {#groupshow}

| Field | Type | Required | Description |
|---|---|---|---|
| `group` | [`RefGroupId`](/docs/reference/api/types#refgroupid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L193) · **Used by:** [`group.show`](/docs/reference/api/methods/group#group-show)

### `GroupSummary` {#groupsummary}

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | [`GroupId`](/docs/reference/api/types#groupid) | yes |  |
| `name` | string | yes |  |
| `workspaces` | [`WorkspaceId`](/docs/reference/api/types#workspaceid)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L183) · **Used by:** [`group.show`](/docs/reference/api/methods/group#group-show), [`GroupListResult`](/docs/reference/api/types#grouplistresult)

## H {#letter-h}

### `Harness` {#harness}

The two harnesses of v1.

Values: `"claude"`, `"codex"`.

**Rust:** [`crates/nh-proto/src/api/common.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/common.rs#L128) · **Used by:** [`AdapterGate`](/docs/reference/api/types#adaptergate), [`AdapterSignal`](/docs/reference/api/types#adaptersignal), [`AgentChoices`](/docs/reference/api/types#agentchoices), [`AgentDetected`](/docs/reference/api/types#agentdetected), [`AgentDriftDetected`](/docs/reference/api/types#agentdriftdetected), [`AgentHarnessVersionUnvalidated`](/docs/reference/api/types#agentharnessversionunvalidated), [`AgentIdentified`](/docs/reference/api/types#agentidentified), [`AgentLink`](/docs/reference/api/types#agentlink), [`AgentList`](/docs/reference/api/types#agentlist), [`AgentStart`](/docs/reference/api/types#agentstart), [`AgentValue`](/docs/reference/api/types#agentvalue), [`CapabilityRow`](/docs/reference/api/types#capabilityrow), [`GitAgent`](/docs/reference/api/types#gitagent), [`HarnessChoices`](/docs/reference/api/types#harnesschoices), [`IntegrationInfo`](/docs/reference/api/types#integrationinfo), [`IntegrationStatusEntry`](/docs/reference/api/types#integrationstatusentry), [`LimitValue`](/docs/reference/api/types#limitvalue), [`PolicyRule`](/docs/reference/api/types#policyrule), [`UnvalidatedHarness`](/docs/reference/api/types#unvalidatedharness), [`WorkflowNode`](/docs/reference/api/types#workflownode)

### `HarnessChoices` {#harnesschoices}

| Field | Type | Required | Description |
|---|---|---|---|
| `available` | boolean | yes | A start with this harness can go on: it is installed and not logged out. |
| `efforts` | string[] | yes | The effort levels for the default model and for a model without its own levels. Empty: the harness takes no effort level. |
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `hint` | string \| `null` |  | What the person can do when the harness is not available. |
| `installed` | boolean | yes | The program of the harness is on the `PATH` of this machine. |
| `login` | [`LoginState`](/docs/reference/api/types#loginstate) | yes |  |
| `models` | [`ModelChoice`](/docs/reference/api/types#modelchoice)[] | yes | The models, in the order of the menu. The default of the harness is not in the list: a launch without a model gets it. |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L86) · **Used by:** [`AgentChoicesResult`](/docs/reference/api/types#agentchoicesresult)

### `HarnessManifestLoaded` {#harnessmanifestloaded}

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes |  |
| `serial` | integer (uint64) | yes |  |
| `source` | string | yes |  |
| `version` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L115) · **Used by:** [event `harness.manifest_loaded`](/docs/reference/api/events#harness-manifest-loaded)

### `HarnessRefresh` {#harnessrefresh}

Finds the harnesses again and asks each one for its login state now. The call returns when the probes are done, and the result is in the projection `harnesses`.

| Field | Type | Required | Description |
|---|---|---|---|
| `harness` | string \| `null` |  | The id of one manifest, for example `codex`. Without it, every harness. |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L110) · **Used by:** [`harness.refresh`](/docs/reference/api/methods/harness#harness-refresh)

### `HarnessValue` {#harnessvalue}

`harnesses`: a harness that a manifest describes, as the core of this machine found it. The key is the id of the manifest.

| Field | Type | Required | Description |
|---|---|---|---|
| `available` | boolean | yes | A start of an agent with this harness can go on: the harness is installed and not logged out. |
| `checked_at` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  | The time of the last check. |
| `hint` | string \| `null` |  | What the person can do: `[auth] login_hint` of the manifest, or why the program was not found. |
| `id` | string | yes |  |
| `installed` | boolean | yes | The program of `[launch] argv` is on the `PATH` of the login environment. |
| `login` | [`LoginState`](/docs/reference/api/types#loginstate) | yes |  |
| `login_method` | [`LoginMethod`](/docs/reference/api/types#loginmethod) \| `null` |  |  |
| `login_source` | [`LoginSource`](/docs/reference/api/types#loginsource) \| `null` |  |  |
| `name` | string | yes |  |
| `path` | string \| `null` |  | The program that the core found. |
| `validated` | boolean | yes | The version is in a range of `[[capabilities]]` of the manifest. |
| `version` | string \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L706) · **Used by:** [projection `harnesses`](/docs/reference/api/projections#harnesses)

### `HealthFacet` {#healthfacet}

| Field | Type | Required | Description |
|---|---|---|---|
| `provenance` | [`Provenance`](/docs/reference/api/types#provenance) | yes |  |
| `value` | [`AdapterHealth`](/docs/reference/api/types#adapterhealth) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L263) · **Used by:** [`Facets`](/docs/reference/api/types#facets)

### `HealthValue` {#healthvalue}

`health`: what the user needs to know about the core itself. It has one key, `_`.

| Field | Type | Required | Description |
|---|---|---|---|
| `holder_protocols` | integer (uint16)[] | yes |  |
| `renderer` | [`RendererKind`](/docs/reference/api/types#rendererkind) \| `null` |  |  |
| `storage` | [`StorageHealth`](/docs/reference/api/types#storagehealth) | yes |  |
| `unvalidated_harnesses` | [`UnvalidatedHarness`](/docs/reference/api/types#unvalidatedharness)[] | yes |  |
| `warnings` | [`DoctorWarning`](/docs/reference/api/types#doctorwarning)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L921) · **Used by:** [projection `health`](/docs/reference/api/projections#health)

### `HermesAnswer` {#hermesanswer}

| Field | Type | Required | Description |
|---|---|---|---|
| `chat` | [`ChatId`](/docs/reference/api/types#chatid) | yes |  |
| `option` | string | yes | The `id` of one of its options. |
| `request` | string | yes | The `request` of the approval item. |

**Rust:** [`crates/nh-proto/src/api/calls/hermes.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/hermes.rs#L51) · **Used by:** [`hermes.answer`](/docs/reference/api/methods/hermes#hermes-answer)

### `HermesAnswerState` {#hermesanswerstate}

| Value | Meaning |
|---|---|
| `"open"` | The approval waits for an answer. |
| `"answered"` | The answer went to Hermes. |
| `"expired"` | The deadline went by, and Hermes denied the request. |
| `"gone"` | The request ended without an answer of neohar: the turn was cancelled, or `hermes acp` stopped. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1534) · **Used by:** [`HermesItemBody`](/docs/reference/api/types#hermesitembody)

### `HermesChatEvent` {#hermeschatevent}

The payload of `hermes.chat_started` and `hermes.chat_closed`.

| Field | Type | Required | Description |
|---|---|---|---|
| `chat` | [`ChatId`](/docs/reference/api/types#chatid) | yes |  |
| `cwd` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L330) · **Used by:** [event `hermes.chat_started`](/docs/reference/api/events#hermes-chat-started), [event `hermes.chat_closed`](/docs/reference/api/events#hermes-chat-closed)

### `HermesChatRef` {#hermeschatref}

| Field | Type | Required | Description |
|---|---|---|---|
| `chat` | [`ChatId`](/docs/reference/api/types#chatid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/hermes.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/hermes.rs#L40) · **Used by:** [`hermes.cancel`](/docs/reference/api/methods/hermes#hermes-cancel), [`hermes.retry`](/docs/reference/api/methods/hermes#hermes-retry), [`hermes.close`](/docs/reference/api/methods/hermes#hermes-close)

### `HermesChatState` {#hermeschatstate}

The state of a chat with Hermes Agent.

One of these forms:

- `"idle"`

- `"starting"` — The core starts `hermes acp` and opens the session.

- `"working"` — A turn works.

- `"waiting"` — An approval of the turn waits for the person.

- `"disconnected"` — `hermes acp` does not run. `error` tells why. A prompt or `hermes.retry` starts it again.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1446) · **Used by:** [`HermesChatValue`](/docs/reference/api/types#hermeschatvalue)

### `HermesChatValue` {#hermeschatvalue}

`hermes_chats`: one chat with Hermes Agent. The key is the id. The core orders the rows, the newest first.

| Field | Type | Required | Description |
|---|---|---|---|
| `context_text` | string \| `null` |  | The context window as a person reads it, for example "15k of 500k tokens". |
| `cwd` | string | yes | The folder in which Hermes works. |
| `error` | string \| `null` |  |  |
| `guessed` | boolean |  | The state of the turn is a guess: the core found the turn when it resumed the session, and ACP tells the end of a turn only to the client that sent the prompt. Default: `false`. |
| `id` | [`ChatId`](/docs/reference/api/types#chatid) | yes |  |
| `mode` | string \| `null` |  |  |
| `model` | string \| `null` |  |  |
| `models` | [`HermesChoice`](/docs/reference/api/types#hermeschoice)[] |  |  |
| `modes` | [`HermesChoice`](/docs/reference/api/types#hermeschoice)[] |  |  |
| `pending` | integer (uint32) |  | The approvals that wait. Default: `0`. |
| `session` | string \| `null` |  | The session id of Hermes, after `session/new`. |
| `started_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `state` | [`HermesChatState`](/docs/reference/api/types#hermeschatstate) | yes |  |
| `time_text` | string | yes | The start as a person reads it, for example "14:12" or "Oct 31". |
| `title` | string | yes | The title that Hermes gave the session, or the folder name before the first prompt. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1467) · **Used by:** [projection `hermes_chats`](/docs/reference/api/projections#hermes-chats)

### `HermesChoice` {#hermeschoice}

A model or a mode that the session offers.

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes |  |
| `name` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1460) · **Used by:** [`HermesChatValue`](/docs/reference/api/types#hermeschatvalue)

### `HermesItem` {#hermesitem}

`hermes_chat:<chat>`: one item of a chat. The core orders the items as the chat shows them.

| Field | Type | Required | Description |
|---|---|---|---|
| `at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `body` | [`HermesItemBody`](/docs/reference/api/types#hermesitembody) | yes |  |
| `id` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1589) · **Used by:** [projection `hermes_chat`](/docs/reference/api/projections#hermes-chat)

### `HermesItemBody` {#hermesitembody}

One of these forms:

#### `type: "prompt"`

| Field | Type | Required | Description |
|---|---|---|---|
| `markdown` | [`Markdown`](/docs/reference/api/types#markdown) | yes |  |
| `type` | `"prompt"` | yes |  |

#### `type: "assistant"`

| Field | Type | Required | Description |
|---|---|---|---|
| `markdown` | [`Markdown`](/docs/reference/api/types#markdown) | yes |  |
| `streaming` | boolean |  | More text can come. Default: `false`. |
| `type` | `"assistant"` | yes |  |

#### `type: "thought"`

| Field | Type | Required | Description |
|---|---|---|---|
| `markdown` | [`Markdown`](/docs/reference/api/types#markdown) | yes |  |
| `type` | `"thought"` | yes |  |

#### `type: "tool"`

| Field | Type | Required | Description |
|---|---|---|---|
| `diff` | [`Diff`](/docs/reference/api/types#diff) \| `null` |  |  |
| `kind` | string | yes | The kind of ACP: `read`, `edit`, `execute`, `search`, `fetch`, `think`, `other`. |
| `output` | [`Markdown`](/docs/reference/api/types#markdown) \| `null` |  |  |
| `status` | [`HermesToolStatus`](/docs/reference/api/types#hermestoolstatus) | yes |  |
| `title` | string | yes |  |
| `type` | `"tool"` | yes |  |

#### `type: "plan"`

| Field | Type | Required | Description |
|---|---|---|---|
| `entries` | [`HermesPlanEntry`](/docs/reference/api/types#hermesplanentry)[] | yes |  |
| `type` | `"plan"` | yes |  |

#### `type: "approval"`

| Field | Type | Required | Description |
|---|---|---|---|
| `answer` | string \| `null` |  | The label of the option that went to Hermes. |
| `deadline` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  | When Hermes denies the request if nobody answers (`approvals.timeout`). |
| `detail` | [`Markdown`](/docs/reference/api/types#markdown) \| `null` |  |  |
| `diff` | [`Diff`](/docs/reference/api/types#diff) \| `null` |  |  |
| `options` | [`HermesOption`](/docs/reference/api/types#hermesoption)[] | yes |  |
| `request` | string | yes | The id that `hermes.answer` names. |
| `state` | [`HermesAnswerState`](/docs/reference/api/types#hermesanswerstate) | yes |  |
| `title` | string | yes |  |
| `type` | `"approval"` | yes |  |

#### `type: "notice"`

| Field | Type | Required | Description |
|---|---|---|---|
| `text` | string | yes |  |
| `type` | `"notice"` | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1547) · **Used by:** [`HermesItem`](/docs/reference/api/types#hermesitem)

### `HermesOption` {#hermesoption}

An option of an approval, as Hermes gives it.

| Field | Type | Required | Description |
|---|---|---|---|
| `allow` | boolean | yes | The option lets the tool run. |
| `id` | string | yes |  |
| `label` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1525) · **Used by:** [`HermesItemBody`](/docs/reference/api/types#hermesitembody)

### `HermesPlanEntry` {#hermesplanentry}

One step of the plan of a turn (the `todo` tool of Hermes).

| Field | Type | Required | Description |
|---|---|---|---|
| `status` | [`HermesPlanStatus`](/docs/reference/api/types#hermesplanstatus) | yes |  |
| `text` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1519) · **Used by:** [`HermesItemBody`](/docs/reference/api/types#hermesitembody)

### `HermesPlanStatus` {#hermesplanstatus}

Values: `"pending"`, `"in_progress"`, `"completed"`.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1512) · **Used by:** [`HermesPlanEntry`](/docs/reference/api/types#hermesplanentry)

### `HermesPrompt` {#hermesprompt}

| Field | Type | Required | Description |
|---|---|---|---|
| `chat` | [`ChatId`](/docs/reference/api/types#chatid) | yes |  |
| `input_id` | integer (uint64) | yes | A second call with the same id sends nothing. |
| `text` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/hermes.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/hermes.rs#L44) · **Used by:** [`hermes.prompt`](/docs/reference/api/methods/hermes#hermes-prompt)

### `HermesSet` {#hermesset}

| Field | Type | Required | Description |
|---|---|---|---|
| `chat` | [`ChatId`](/docs/reference/api/types#chatid) | yes |  |
| `mode` | string \| `null` |  | The id of a mode of the chat. |
| `model` | string \| `null` |  | The id of a model of the chat. |

**Rust:** [`crates/nh-proto/src/api/calls/hermes.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/hermes.rs#L59) · **Used by:** [`hermes.set`](/docs/reference/api/methods/hermes#hermes-set)

### `HermesStart` {#hermesstart}

| Field | Type | Required | Description |
|---|---|---|---|
| `cwd` | string | yes | The absolute path of the folder in which Hermes works. |
| `prompt` | string \| `null` |  | The first prompt. |

**Rust:** [`crates/nh-proto/src/api/calls/hermes.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/hermes.rs#L28) · **Used by:** [`hermes.start`](/docs/reference/api/methods/hermes#hermes-start)

### `HermesStarted` {#hermesstarted}

| Field | Type | Required | Description |
|---|---|---|---|
| `chat` | [`ChatId`](/docs/reference/api/types#chatid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/hermes.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/hermes.rs#L36) · **Used by:** [`hermes.start`](/docs/reference/api/methods/hermes#hermes-start)

### `HermesStatus` {#hermesstatus}

`hermes.check`: whether this machine can run Hermes.

| Field | Type | Required | Description |
|---|---|---|---|
| `approvals_mode` | string \| `null` |  | `approvals.mode` of the config of Hermes. With `smart` or `off`, shell commands do not ask in the chat. |
| `found` | boolean | yes | `hermes acp --version` answered. |
| `message` | string \| `null` |  | What to do, when Hermes is not found or the check failed. |
| `ok` | boolean | yes | `hermes acp --check` passed. |
| `version` | string \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/hermes.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/hermes.rs#L12) · **Used by:** [`hermes.check`](/docs/reference/api/methods/hermes#hermes-check)

### `HermesToolStatus` {#hermestoolstatus}

Values: `"running"`, `"done"`, `"failed"`.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1504) · **Used by:** [`HermesItemBody`](/docs/reference/api/types#hermesitembody)

### `Hunk` {#hunk}

| Field | Type | Required | Description |
|---|---|---|---|
| `header` | string | yes |  |
| `lines` | [`DiffLine`](/docs/reference/api/types#diffline)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/rich.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/rich.rs#L45) · **Used by:** [`FileDiff`](/docs/reference/api/types#filediff)

## I {#letter-i}

### `InputStreamParams` {#inputstreamparams}

| Field | Type | Required | Description |
|---|---|---|---|
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/streams.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/streams.rs#L55) · **Used by:** [stream `input`](/docs/reference/api/streams#input)

### `IntegrationCapabilitiesParams` {#integrationcapabilitiesparams}

An object with no fields: `{}`.

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L305) · **Used by:** [`integration.capabilities`](/docs/reference/api/methods/integration#integration-capabilities)

### `IntegrationCapabilitiesResult` {#integrationcapabilitiesresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `rows` | [`CapabilityRow`](/docs/reference/api/types#capabilityrow)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L322) · **Used by:** [`integration.capabilities`](/docs/reference/api/methods/integration#integration-capabilities)

### `IntegrationChanges` {#integrationchanges}

| Field | Type | Required | Description |
|---|---|---|---|
| `changes` | [`FileChange`](/docs/reference/api/types#filechange)[] | yes |  |
| `notes` | string[] | yes | What the user must do, for example `/hooks` in Codex. |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L263) · **Used by:** [`integration.install`](/docs/reference/api/methods/integration#integration-install), [`integration.uninstall`](/docs/reference/api/methods/integration#integration-uninstall)

### `IntegrationDoctor` {#integrationdoctor}

Runs the round trip of `nh integration doctor`. It returns a job.

| Field | Type | Required | Description |
|---|---|---|---|
| `target` | [`IntegrationTarget`](/docs/reference/api/types#integrationtarget) \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L300) · **Used by:** [`integration.doctor`](/docs/reference/api/methods/integration#integration-doctor)

### `IntegrationInfo` {#integrationinfo}

| Field | Type | Required | Description |
|---|---|---|---|
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `installed` | boolean | yes |  |
| `transports` | string[] | yes | The transports in use: `hooks`, `transcript`, `screen`, `process`. |
| `version_seen` | string \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L222) · **Used by:** [`IntegrationListResult`](/docs/reference/api/types#integrationlistresult)

### `IntegrationInstall` {#integrationinstall}

| Field | Type | Required | Description |
|---|---|---|---|
| `dry_run` | boolean |  | Default: `false`. |
| `project_root` | string \| `null` |  | The repository for the scope `project`. The CLI gives its working directory. |
| `scope` | [`IntegrationScope`](/docs/reference/api/types#integrationscope) |  | Default: `"user"`. |
| `target` | [`IntegrationTarget`](/docs/reference/api/types#integrationtarget) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L235) · **Used by:** [`integration.install`](/docs/reference/api/methods/integration#integration-install)

### `IntegrationList` {#integrationlist}

An object with no fields: `{}`.

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L220) · **Used by:** [`integration.list`](/docs/reference/api/methods/integration#integration-list)

### `IntegrationListResult` {#integrationlistresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `integrations` | [`IntegrationInfo`](/docs/reference/api/types#integrationinfo)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L231) · **Used by:** [`integration.list`](/docs/reference/api/methods/integration#integration-list)

### `IntegrationScope` {#integrationscope}

Values: `"user"`, `"project"`.

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L214) · **Used by:** [`IntegrationInstall`](/docs/reference/api/types#integrationinstall), [`IntegrationUninstall`](/docs/reference/api/types#integrationuninstall)

### `IntegrationStatusEntry` {#integrationstatusentry}

| Field | Type | Required | Description |
|---|---|---|---|
| `files` | string[] | yes |  |
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `hooks_trusted` | boolean \| `null` |  | Codex only: every hook is trusted. |
| `installed` | boolean | yes |  |
| `last_signal_at` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  |  |
| `version_validated` | boolean | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L274) · **Used by:** [`IntegrationStatusResult`](/docs/reference/api/types#integrationstatusresult)

### `IntegrationStatusParams` {#integrationstatusparams}

| Field | Type | Required | Description |
|---|---|---|---|
| `target` | [`IntegrationTarget`](/docs/reference/api/types#integrationtarget) \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L269) · **Used by:** [`integration.status`](/docs/reference/api/methods/integration#integration-status)

### `IntegrationStatusResult` {#integrationstatusresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `status` | [`IntegrationStatusEntry`](/docs/reference/api/types#integrationstatusentry)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L286) · **Used by:** [`integration.status`](/docs/reference/api/methods/integration#integration-status)

### `IntegrationTarget` {#integrationtarget}

Which integration. `All` means both harnesses.

Values: `"claude"`, `"codex"`, `"all"`.

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L206) · **Used by:** [`IntegrationDoctor`](/docs/reference/api/types#integrationdoctor), [`IntegrationInstall`](/docs/reference/api/types#integrationinstall), [`IntegrationStatusParams`](/docs/reference/api/types#integrationstatusparams), [`IntegrationUninstall`](/docs/reference/api/types#integrationuninstall)

### `IntegrationUninstall` {#integrationuninstall}

| Field | Type | Required | Description |
|---|---|---|---|
| `project_root` | string \| `null` |  | The repository for the scope `project`. The CLI gives its working directory. |
| `scope` | [`IntegrationScope`](/docs/reference/api/types#integrationscope) |  | Default: `"user"`. |
| `target` | [`IntegrationTarget`](/docs/reference/api/types#integrationtarget) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L290) · **Used by:** [`integration.uninstall`](/docs/reference/api/methods/integration#integration-uninstall)

### `InteractionAnswer` {#interactionanswer}

`via` goes into the delivery record. A second answer gets `already_answered`.

| Field | Type | Required | Description |
|---|---|---|---|
| `decision` | [`Decision`](/docs/reference/api/types#decision) | yes |  |
| `interaction` | [`RefInteractionId`](/docs/reference/api/types#refinteractionid) | yes |  |
| `via` | [`Via`](/docs/reference/api/types#via) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L283) · **Used by:** [`interaction.answer`](/docs/reference/api/methods/interaction#interaction-answer)

### `InteractionAnswered` {#interactionanswered}

| Field | Type | Required | Description |
|---|---|---|---|
| `rev` | integer (uint32) | yes | The revision of the decision. |
| `state` | [`InteractionState`](/docs/reference/api/types#interactionstate) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L289) · **Used by:** [`interaction.answer`](/docs/reference/api/methods/interaction#interaction-answer)

### `InteractionCancelled` {#interactioncancelled}

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `reason` | string | yes | For example `process_exited`. |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L162) · **Used by:** [event `interaction.cancelled`](/docs/reference/api/events#interaction-cancelled)

### `InteractionDecided` {#interactiondecided}

| Field | Type | Required | Description |
|---|---|---|---|
| `by` | string | yes |  |
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `rev` | integer (uint32) | yes |  |
| `via` | [`Via`](/docs/reference/api/types#via) | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L137) · **Used by:** [event `interaction.decided`](/docs/reference/api/events#interaction-decided)

### `InteractionDelivery` {#interactiondelivery}

The payload of `interaction.delivery_started`, `interaction.delivered` and `interaction.delivery_unknown`.

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `rev` | integer (uint32) | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L146) · **Used by:** [event `interaction.delivery_started`](/docs/reference/api/events#interaction-delivery-started), [event `interaction.delivered`](/docs/reference/api/events#interaction-delivered), [event `interaction.delivery_unknown`](/docs/reference/api/events#interaction-delivery-unknown)

### `InteractionDeliveryFailed` {#interactiondeliveryfailed}

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `reason` | string | yes |  |
| `rev` | integer (uint32) | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L151) · **Used by:** [event `interaction.delivery_failed`](/docs/reference/api/events#interaction-delivery-failed)

### `InteractionDismiss` {#interactiondismiss}

Closes an interaction without an answer (a notice, or a card the user does not want).

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`RefInteractionId`](/docs/reference/api/types#refinteractionid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L296) · **Used by:** [`interaction.dismiss`](/docs/reference/api/methods/interaction#interaction-dismiss)

### `InteractionEnded` {#interactionended}

The payload of `interaction.resolved_elsewhere` and `interaction.dismissed`.

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L158) · **Used by:** [event `interaction.resolved_elsewhere`](/docs/reference/api/events#interaction-resolved-elsewhere), [event `interaction.dismissed`](/docs/reference/api/events#interaction-dismissed)

### `InteractionId` {#interactionid}

The ULID of a interaction, as 26 characters.

Type: string. Pattern: `^[0-9A-HJKMNP-TV-Z]{26}$`.

**Used by:** [`AdapterDeliveryAck`](/docs/reference/api/types#adapterdeliveryack), [`AdapterGateResume`](/docs/reference/api/types#adaptergateresume), [`AttentionValue`](/docs/reference/api/types#attentionvalue), [`Facets`](/docs/reference/api/types#facets), [`GateReply`](/docs/reference/api/types#gatereply), [`GateResult`](/docs/reference/api/types#gateresult), [`InteractionCancelled`](/docs/reference/api/types#interactioncancelled), [`InteractionDecided`](/docs/reference/api/types#interactiondecided), [`InteractionDelivery`](/docs/reference/api/types#interactiondelivery), [`InteractionDeliveryFailed`](/docs/reference/api/types#interactiondeliveryfailed), [`InteractionEnded`](/docs/reference/api/types#interactionended), [`InteractionNextResult`](/docs/reference/api/types#interactionnextresult), [`InteractionOpened`](/docs/reference/api/types#interactionopened), [`InteractionRevealed`](/docs/reference/api/types#interactionrevealed), [`InteractionValue`](/docs/reference/api/types#interactionvalue), [`NotificationValue`](/docs/reference/api/types#notificationvalue), [`Reveal`](/docs/reference/api/types#reveal)

### `InteractionKind` {#interactionkind}

Values: `"approval"`, `"question"`, `"plan_review"`, `"notice"`.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L382) · **Used by:** [`Capabilities`](/docs/reference/api/types#capabilities), [`InteractionOpened`](/docs/reference/api/types#interactionopened), [`InteractionValue`](/docs/reference/api/types#interactionvalue)

### `InteractionList` {#interactionlist}

| Field | Type | Required | Description |
|---|---|---|---|
| `limit` | integer (uint32) \| `null` |  |  |
| `open_only` | boolean |  | Only the open ones. It is the default. Default: `true`. |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) \| `null` |  |  |
| `run` | [`RefRunId`](/docs/reference/api/types#refrunid) \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L206) · **Used by:** [`interaction.list`](/docs/reference/api/methods/interaction#interaction-list)

### `InteractionListResult` {#interactionlistresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `interactions` | [`InteractionValue`](/docs/reference/api/types#interactionvalue)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L218) · **Used by:** [`interaction.list`](/docs/reference/api/methods/interaction#interaction-list)

### `InteractionNext` {#interactionnext}

The most urgent open interaction that a client can answer.

| Field | Type | Required | Description |
|---|---|---|---|
| `after` | [`RefInteractionId`](/docs/reference/api/types#refinteractionid) \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L301) · **Used by:** [`interaction.next`](/docs/reference/api/methods/interaction#interaction-next)

### `InteractionNextResult` {#interactionnextresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string \| `null` |  |  |
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L306) · **Used by:** [`interaction.next`](/docs/reference/api/methods/interaction#interaction-next)

### `InteractionOpened` {#interactionopened}

| Field | Type | Required | Description |
|---|---|---|---|
| `by_policy` | boolean |  | Policy decided it at once. Default: `false`. |
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `kind` | [`InteractionKind`](/docs/reference/api/types#interactionkind) | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) \| `null` |  |  |
| `risk` | [`Risk`](/docs/reference/api/types#risk) | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L124) · **Used by:** [event `interaction.opened`](/docs/reference/api/events#interaction-opened)

### `InteractionPayload` {#interactionpayload}

One of these forms:

#### `kind: "approval"`

| Field | Type | Required | Description |
|---|---|---|---|
| `kind` | `"approval"` | yes |  |

#### `kind: "question"`

| Field | Type | Required | Description |
|---|---|---|---|
| `kind` | `"question"` | yes |  |

#### `kind: "plan_review"`

| Field | Type | Required | Description |
|---|---|---|---|
| `kind` | `"plan_review"` | yes |  |

#### `kind: "notice"`

| Field | Type | Required | Description |
|---|---|---|---|
| `kind` | `"notice"` | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L494) · **Used by:** [`InteractionValue`](/docs/reference/api/types#interactionvalue)

### `InteractionReveal` {#interactionreveal}

Brings an interaction to the person: the app comes to the front, and the window shows the inbox with the card focused. It does not answer and does not focus the pane. Without `interaction`, it takes the result of `interaction.next`.

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`RefInteractionId`](/docs/reference/api/types#refinteractionid) \| `null` |  |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  | The window that shows the card. The default is the window with the OS focus, else the first one. |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L316) · **Used by:** [`interaction.reveal`](/docs/reference/api/methods/interaction#interaction-reveal)

### `InteractionRevealed` {#interactionrevealed}

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) \| `null` |  | The interaction that the window shows, or none when nothing waits. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L325) · **Used by:** [`interaction.reveal`](/docs/reference/api/methods/interaction#interaction-reveal)

### `InteractionShow` {#interactionshow}

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`RefInteractionId`](/docs/reference/api/types#refinteractionid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L222) · **Used by:** [`interaction.show`](/docs/reference/api/methods/interaction#interaction-show)

### `InteractionShown` {#interactionshown}

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionValue`](/docs/reference/api/types#interactionvalue) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L226) · **Used by:** [`interaction.show`](/docs/reference/api/methods/interaction#interaction-show)

### `InteractionState` {#interactionstate}

The state of the delivery record, plus how an interaction can end otherwise.

Values: `"open"`, `"decision_recorded"`, `"delivering"`, `"delivered"`, `"delivery_unknown"`, `"failed"`, `"superseded"`, `"resolved_elsewhere"`, `"dismissed"`, `"cancelled"`.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L392) · **Used by:** [`DebugAnswered`](/docs/reference/api/types#debuganswered), [`InteractionAnswered`](/docs/reference/api/types#interactionanswered), [`InteractionValue`](/docs/reference/api/types#interactionvalue)

### `InteractionValue` {#interactionvalue}

`interactions`: an approval, a question, a plan review or a notice. The core renders the payload once, when the interaction opens.

| Field | Type | Required | Description |
|---|---|---|---|
| `always_rule` | string \| `null` |  | The rule that "Allow always" adds, such as `pnpm test:*` or `src/app/**`. Only an approval that a rule may cover, with an answer through the hook, has it. Without it the core refuses "always". |
| `answerable` | boolean | yes | A client can answer it now. A run that is only suggested is not answerable. |
| `channels` | [`AnswerChannel`](/docs/reference/api/types#answerchannel)[] | yes | The channels that this kind has for this run. |
| `deadline` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  |  |
| `delivery` | [`DeliveryInfo`](/docs/reference/api/types#deliveryinfo) | yes |  |
| `handle` | string | yes | The short handle, for example `i42`. |
| `id` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `kind` | [`InteractionKind`](/docs/reference/api/types#interactionkind) | yes |  |
| `mode` | [`GateMode`](/docs/reference/api/types#gatemode) | yes |  |
| `opened_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) \| `null` |  |  |
| `payload` | [`InteractionPayload`](/docs/reference/api/types#interactionpayload) | yes |  |
| `risk` | [`Risk`](/docs/reference/api/types#risk) | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) \| `null` |  |  |
| `state` | [`InteractionState`](/docs/reference/api/types#interactionstate) | yes |  |
| `title` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L503) · **Used by:** [projection `interactions`](/docs/reference/api/projections#interactions), [`InteractionListResult`](/docs/reference/api/types#interactionlistresult), [`InteractionShown`](/docs/reference/api/types#interactionshown)

## J {#letter-j}

### `JobId` {#jobid}

The ULID of a job, as 26 characters.

Type: string. Pattern: `^[0-9A-HJKMNP-TV-Z]{26}$`.

**Used by:** [`GitJob`](/docs/reference/api/types#gitjob), [`JobStarted`](/docs/reference/api/types#jobstarted), [`JobValue`](/docs/reference/api/types#jobvalue), [`JobsCancel`](/docs/reference/api/types#jobscancel)

### `JobStarted` {#jobstarted}

The result of a command that runs as a job. Progress is in the `jobs` projection.

| Field | Type | Required | Description |
|---|---|---|---|
| `job` | [`JobId`](/docs/reference/api/types#jobid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/common.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/common.rs#L37) · **Used by:** [`agent.start`](/docs/reference/api/methods/agent#agent-start), [`agent.resume`](/docs/reference/api/methods/agent#agent-resume), [`git.fetch`](/docs/reference/api/methods/git#git-fetch), [`git.pull`](/docs/reference/api/methods/git#git-pull), [`workflow.continue`](/docs/reference/api/methods/workflow#workflow-continue), [`integration.doctor`](/docs/reference/api/methods/integration#integration-doctor)

### `JobStatus` {#jobstatus}

Values: `"running"`, `"done"`, `"failed"`, `"cancelled"`.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L819) · **Used by:** [`JobValue`](/docs/reference/api/types#jobvalue)

### `JobValue` {#jobvalue}

`jobs`: the progress of a long command.

| Field | Type | Required | Description |
|---|---|---|---|
| `error` | [`CallError`](/docs/reference/api/types#callerror) \| `null` |  |  |
| `finished_at` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  |  |
| `id` | [`JobId`](/docs/reference/api/types#jobid) | yes |  |
| `message` | string \| `null` |  |  |
| `progress` | number (float) \| `null` |  | From 0 to 1, when the job knows. |
| `result` | any |  | What the job made, when it is done (for example `{"run": "…"}`). |
| `started_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `status` | [`JobStatus`](/docs/reference/api/types#jobstatus) | yes |  |
| `title` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L827) · **Used by:** [projection `jobs`](/docs/reference/api/projections#jobs)

### `JobsCancel` {#jobscancel}

| Field | Type | Required | Description |
|---|---|---|---|
| `job` | [`JobId`](/docs/reference/api/types#jobid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L460) · **Used by:** [`jobs.cancel`](/docs/reference/api/methods/jobs#jobs-cancel)

### `JsonRpc` {#jsonrpc}

Type: `"2.0"`.

**Rust:** [`crates/nh-proto/src/api/envelope.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/envelope.rs#L16) · **Used by:** [`Notification`](/docs/reference/api/types#notification), [`Request`](/docs/reference/api/types#request), [`Response`](/docs/reference/api/types#response)

## K {#letter-k}

### `KeyKind` {#keykind}

What the key did. A client sends `release` only when the pane's frame has the kitty "report event types" mode bit.

Values: `"press"`, `"repeat"`, `"release"`.

**Rust:** [`crates/nh-proto/src/input.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/input.rs#L95) · **Used by:** [`LogicalKey`](/docs/reference/api/types#logicalkey)

## L {#letter-l}

### `LaunchMode` {#launchmode}

Values: `"tui"`, `"headless"`.

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L309) · **Used by:** [`CapabilityRow`](/docs/reference/api/types#capabilityrow)

### `LayoutChanged` {#layoutchanged}

The layout of a tab changed (a split, a ratio, a float or a zoom).

| Field | Type | Required | Description |
|---|---|---|---|
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L232) · **Used by:** [event `layout.changed`](/docs/reference/api/events#layout-changed)

### `LayoutEqualize` {#layoutequalize}

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | integer (uint32)[] |  | Default: `[]`. |
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L252) · **Used by:** [`layout.equalize`](/docs/reference/api/methods/layout#layout-equalize)

### `LayoutNode` {#layoutnode}

The layout of a tab: panes in nested splits. The UI renders it with CSS.

One of these forms:

#### `type: "pane"`

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `type` | `"pane"` | yes |  |

#### `type: "split"`

| Field | Type | Required | Description |
|---|---|---|---|
| `axis` | [`Axis`](/docs/reference/api/types#axis) | yes |  |
| `children` | [`LayoutNode`](/docs/reference/api/types#layoutnode)[] | yes |  |
| `ratios` | number (float)[] | yes | One share for each child. They add up to 1. |
| `type` | `"split"` | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L104) · **Used by:** [`TabValue`](/docs/reference/api/types#tabvalue)

### `LayoutSetRatios` {#layoutsetratios}

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | integer (uint32)[] | yes | The split node, as the indices of the children from the root. `[]` is the root. |
| `ratios` | number (float)[] | yes |  |
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L245) · **Used by:** [`layout.set_ratios`](/docs/reference/api/methods/layout#layout-set-ratios)

### `LayoutZoom` {#layoutzoom}

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) \| `null` |  | The pane to zoom. Without it the zoom ends. |
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L258) · **Used by:** [`layout.zoom`](/docs/reference/api/methods/layout#layout-zoom)

### `LimitValue` {#limitvalue}

`limits`: the rate limits of an account.

| Field | Type | Required | Description |
|---|---|---|---|
| `account` | string | yes |  |
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `usage_today` | [`Usage`](/docs/reference/api/types#usage) | yes |  |
| `windows` | [`RateLimitWindow`](/docs/reference/api/types#ratelimitwindow)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L810) · **Used by:** [projection `limits`](/docs/reference/api/projections#limits)

### `LineKind` {#linekind}

Values: `"ctx"`, `"add"`, `"del"`.

**Rust:** [`crates/nh-proto/src/api/rich.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/rich.rs#L25) · **Used by:** [`DiffLine`](/docs/reference/api/types#diffline)

### `LineRange` {#linerange}

A line range of the archive of a pane, counted from the first line that the archive holds. `to` is not part of the range.

| Field | Type | Required | Description |
|---|---|---|---|
| `from` | integer (uint64) | yes |  |
| `to` | integer (uint64) | yes |  |

**Rust:** [`crates/nh-proto/src/api/common.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/common.rs#L164) · **Used by:** [`PaneRead`](/docs/reference/api/types#paneread)

### `LinkValue` {#linkvalue}

`link`: the link of a window to its machine. The host of the client serves it, to each window. It has one key, `_`. For a window of the local core the state is `local`.

| Field | Type | Required | Description |
|---|---|---|---|
| `changed_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes | When the state last changed. |
| `machine` | string | yes | The label of the machine. |
| `reason` | string \| `null` |  | Why the link is not connected. |
| `rtt_ms` | number (float) \| `null` |  |  |
| `state` | [`MachineState`](/docs/reference/api/types#machinestate) | yes |  |
| `text` | string | yes | What the status bar shows: `local`, the label of a link that is up, or the label with the state for a link that is not. The host makes it, and the page prints it. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L904) · **Used by:** [projection `link`](/docs/reference/api/projections#link)

### `Liveness` {#liveness}

One of these forms:

#### `state: "starting"`

| Field | Type | Required | Description |
|---|---|---|---|
| `state` | `"starting"` | yes |  |

#### `state: "alive"`

| Field | Type | Required | Description |
|---|---|---|---|
| `state` | `"alive"` | yes |  |

#### `state: "exited"`

| Field | Type | Required | Description |
|---|---|---|---|
| `code` | integer (int32) \| `null` |  |  |
| `state` | `"exited"` | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L214) · **Used by:** [`LivenessFacet`](/docs/reference/api/types#livenessfacet)

### `LivenessFacet` {#livenessfacet}

| Field | Type | Required | Description |
|---|---|---|---|
| `provenance` | [`Provenance`](/docs/reference/api/types#provenance) | yes |  |
| `value` | [`Liveness`](/docs/reference/api/types#liveness) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L253) · **Used by:** [`Facets`](/docs/reference/api/types#facets)

### `LogicalInput` {#logicalinput}

The `event` of `pane.input`.

One of these forms:

#### `type: "key"`

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `"key"` | yes |  |

#### `type: "mouse"`

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `"mouse"` | yes |  |

#### `type: "text"`

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `"text"` | yes |  |

**Rust:** [`crates/nh-proto/src/input.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/input.rs#L180) · **Used by:** [`PaneInput`](/docs/reference/api/types#paneinput)

### `LogicalKey` {#logicalkey}

A key event. The fields are those of the DOM `KeyboardEvent`, after the client has dealt with composition, dead keys, `AltGraph` and the Option key of macOS.

| Field | Type | Required | Description |
|---|---|---|---|
| `code` | string | yes | The physical key (`KeyboardEvent.code`), for example `KeyC` or `Numpad1`. |
| `key` | string | yes | The key in the layout of the user (`KeyboardEvent.key`), for example `c`, `C`, `Enter`. |
| `kind` | [`KeyKind`](/docs/reference/api/types#keykind) |  | Default: `"press"`. |
| `location` | integer (uint8) |  | `KeyboardEvent.location`: 0 standard, 1 left, 2 right, 3 numeric keypad. Default: `0`. |
| `mods` | [`Mods`](/docs/reference/api/types#mods) |  | Default: `[]`. |
| `text` | string \| `null` |  | The text that the key produces, when the client knows it. The kitty protocol sends it when the application asks for the associated text. |

**Rust:** [`crates/nh-proto/src/input.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/input.rs#L106) · **Used by:** [`LogicalInput`](/docs/reference/api/types#logicalinput)

### `LogicalMouse` {#logicalmouse}

A mouse event at a cell of the grid.

| Field | Type | Required | Description |
|---|---|---|---|
| `button` | [`MouseButton`](/docs/reference/api/types#mousebutton) |  | Default: `"none"`. |
| `col` | integer (uint16) | yes | The column of the cell, counted from 0 at the left. |
| `kind` | [`MouseKind`](/docs/reference/api/types#mousekind) | yes |  |
| `mods` | [`Mods`](/docs/reference/api/types#mods) |  | Default: `[]`. |
| `row` | integer (uint16) | yes | The row of the cell, counted from 0 at the top. |
| `wheel_x` | integer (int16) |  | Horizontal wheel notches. Positive is to the right, as in `WheelEvent.deltaX`. Default: `0`. Minimum: -32768. |
| `wheel_y` | integer (int16) |  | Vertical wheel notches. Positive is down, as in `WheelEvent.deltaY`. Default: `0`. Minimum: -32768. |

**Rust:** [`crates/nh-proto/src/input.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/input.rs#L151) · **Used by:** [`LogicalInput`](/docs/reference/api/types#logicalinput)

### `LogicalText` {#logicaltext}

Text that is not a key press: the result of an input method or a dead key.

| Field | Type | Required | Description |
|---|---|---|---|
| `text` | string | yes |  |

**Rust:** [`crates/nh-proto/src/input.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/input.rs#L172) · **Used by:** [`LogicalInput`](/docs/reference/api/types#logicalinput)

### `LoginMethod` {#loginmethod}

How a harness is logged in, when its probe tells it. The core keeps no account name, address or token.

One of these forms:

- `"api_key"`

- `"subscription"` — A subscription account: Claude.ai for Claude Code, ChatGPT for Codex.

- `"token"` — A token that the person gave the harness, for example `CLAUDE_CODE_OAUTH_TOKEN`.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L696) · **Used by:** [`HarnessValue`](/docs/reference/api/types#harnessvalue)

### `LoginSource` {#loginsource}

What gave the login state: the probe of the manifest, or a run that showed it.

Values: `"probe"`, `"run"`.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L687) · **Used by:** [`HarnessValue`](/docs/reference/api/types#harnessvalue)

### `LoginState` {#loginstate}

The login state of a harness on this machine. `unknown` means that the probe failed, timed out, or gave an output that no rule of the manifest matches.

Values: `"logged_in"`, `"logged_out"`, `"unknown"`.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L678) · **Used by:** [`HarnessChoices`](/docs/reference/api/types#harnesschoices), [`HarnessValue`](/docs/reference/api/types#harnessvalue), [`MachineHarness`](/docs/reference/api/types#machineharness)

## M {#letter-m}

### `MachineHarness` {#machineharness}

The login state of one harness on a machine, as the row of that machine says it.

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes | The id of the manifest: `claude` or `codex`. |
| `installed` | boolean | yes |  |
| `login` | [`LoginState`](/docs/reference/api/types#loginstate) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L756) · **Used by:** [`MachineValue`](/docs/reference/api/types#machinevalue)

### `MachineInfo` {#machineinfo}

`machine.info`: the machine of the core.

| Field | Type | Required | Description |
|---|---|---|---|
| `arch` | string | yes |  |
| `hostname` | string \| `null` |  |  |
| `label` | string | yes | The label of the machine. The local machine is `local`. |
| `os` | string | yes |  |
| `uuid` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L64) · **Used by:** [`machine.info`](/docs/reference/api/methods/machine#machine-info)

### `MachineList` {#machinelist}

`machine.list`: the rows of the projection `machines`, in its order.

| Field | Type | Required | Description |
|---|---|---|---|
| `machines` | [`MachineValue`](/docs/reference/api/types#machinevalue)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L441) · **Used by:** [`machine.list`](/docs/reference/api/methods/machine#machine-list)

### `MachineState` {#machinestate}

Values: `"local"`, `"connecting"`, `"connected"`, `"disconnected"`.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L748) · **Used by:** [`LinkValue`](/docs/reference/api/types#linkvalue), [`MachineValue`](/docs/reference/api/types#machinevalue)

### `MachineTarget` {#machinetarget}

`machine.connect`, `machine.disconnect` and `machine.open_window`: the label of a machine of `config.toml`.

| Field | Type | Required | Description |
|---|---|---|---|
| `machine` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L436) · **Used by:** [`machine.connect`](/docs/reference/api/methods/machine#machine-connect), [`machine.disconnect`](/docs/reference/api/methods/machine#machine-disconnect), [`machine.open_window`](/docs/reference/api/methods/machine#machine-open-window)

### `MachineUpdate` {#machineupdate}

What the client should do about the version of the core of a machine.

| Value | Meaning |
|---|---|
| `"update_available"` | The core is older than the app. The link works, and the app offers an update. |
| `"update_app"` | The core is newer than the app within the major version. The link works, and the app asks the person to update the app. |
| `"version_mismatch"` | The core is of another major version of the API. The link does not work. |
| `"install_needed"` | The machine has no `nh`, or a bridge that does not know `link/1`. The app offers the install. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L773) · **Used by:** [`MachineValue`](/docs/reference/api/types#machinevalue)

### `MachineValue` {#machinevalue}

`machines`: the machine of the person and one row for each `[machines.<label>]`. The key is the label. The core orders the rows: `local`, then the order of `config.toml`.

| Field | Type | Required | Description |
|---|---|---|---|
| `forwards` | [`PortForward`](/docs/reference/api/types#portforward)[] |  | The ports forwarded to the machine. Default: `[]`. |
| `harnesses` | [`MachineHarness`](/docs/reference/api/types#machineharness)[] |  | The summary of the harnesses of the machine, copied from its `harnesses` projection. Default: `[]`. |
| `label` | string | yes |  |
| `latency_ms` | number (float) \| `null` |  |  |
| `reason` | string \| `null` |  | Why the link is not connected: the reason of the last disconnect or the last failure. |
| `state` | [`MachineState`](/docs/reference/api/types#machinestate) | yes |  |
| `update` | [`MachineUpdate`](/docs/reference/api/types#machineupdate) \| `null` |  |  |
| `version` | string \| `null` |  | The version of the core of the machine, from `Welcome`. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L788) · **Used by:** [projection `machines`](/docs/reference/api/projections#machines), [`MachineList`](/docs/reference/api/types#machinelist)

### `Markdown` {#markdown}

Sanitized HTML (`ammonia`). The UI inserts it only through the `RichHtml` component.

| Field | Type | Required | Description |
|---|---|---|---|
| `html` | string | yes |  |
| `truncated` | boolean |  | The text was cut at the cap (256 KB for a plan). The UI offers "open full". Default: `false`. |

**Rust:** [`crates/nh-proto/src/api/rich.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/rich.rs#L71) · **Used by:** [`rich.markdown`](/docs/reference/api/methods/rich#rich-markdown), [`FsReadResult`](/docs/reference/api/types#fsreadresult), [`HermesItemBody`](/docs/reference/api/types#hermesitembody), [`PlanPayload`](/docs/reference/api/types#planpayload), [`WorkflowReport`](/docs/reference/api/types#workflowreport)

### `ModName` {#modname}

One modifier, in the order of the list form of [`Mods`].

Values: `"shift"`, `"ctrl"`, `"alt"`, `"meta"`, `"caps_lock"`, `"num_lock"`.

**Used by:** [`Mods`](/docs/reference/api/types#mods)

### `ModelChoice` {#modelchoice}

| Field | Type | Required | Description |
|---|---|---|---|
| `efforts` | string[] |  | The effort levels of this model. Empty: the levels of the harness. |
| `id` | string | yes | What goes on the command line. |
| `label` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L77) · **Used by:** [`HarnessChoices`](/docs/reference/api/types#harnesschoices)

### `Mods` {#mods}

The modifiers that were active when an event happened.

On the wire this is a list of names (`["ctrl", "shift"]`), in the order of the fields, so a reader sees the chord. `meta` is the Command key on macOS and the Windows key elsewhere; the kitty keyboard protocol calls it `super`. `caps_lock` and `num_lock` are the lock states, not key presses.

Type: [`ModName`](/docs/reference/api/types#modname)[].

**Rust:** [`crates/nh-proto/src/input.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/input.rs#L36) · **Used by:** [`LogicalKey`](/docs/reference/api/types#logicalkey), [`LogicalMouse`](/docs/reference/api/types#logicalmouse)

### `MouseButton` {#mousebutton}

A mouse button.

Values: `"none"`, `"left"`, `"middle"`, `"right"`.

**Rust:** [`crates/nh-proto/src/input.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/input.rs#L140) · **Used by:** [`LogicalMouse`](/docs/reference/api/types#logicalmouse)

### `MouseKind` {#mousekind}

What a mouse event is.

One of these forms:

- `"down"` | `"up"` | `"wheel"`

- `"move"` — The pointer moved. `button` is the button that is held, if any.

**Rust:** [`crates/nh-proto/src/input.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/input.rs#L128) · **Used by:** [`LogicalMouse`](/docs/reference/api/types#logicalmouse)

### `MouseMode` {#mousemode}

The mouse protocol that the program in the pane asked for.

Values: `"none"`, `"x10"`, `"normal"`, `"button_event"`, `"any_event"`.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L132) · **Used by:** [`PaneValue`](/docs/reference/api/types#panevalue)

## N {#letter-n}

### `NodeState` {#nodestate}

The state of one node in an execution.

| Value | Meaning |
|---|---|
| `"waiting"` | An input is not done. |
| `"queued"` | The inputs are done, and `workflows.max_agents` agents work. |
| `"working"` | The agent of the node works. |
| `"needs_you"` | The agent of the node has an open interaction. |
| `"done"` | The report of the node exists. |
| `"failed"` | The node failed. `reason` tells why. |
| `"skipped"` | The person skipped the node. |
| `"cancelled"` | The person cancelled the execution. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1341) · **Used by:** [`ExecutionNodeValue`](/docs/reference/api/types#executionnodevalue), [`WorkflowNodeEvent`](/docs/reference/api/types#workflownodeevent)

### `NoticePayload` {#noticepayload}

| Field | Type | Required | Description |
|---|---|---|---|
| `resets_at` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  |  |
| `text` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L487) · **Used by:** [`InteractionPayload`](/docs/reference/api/types#interactionpayload)

### `Notification` {#notification}

A message of the server without an id: `events.event`, `projection.message` and the like.

| Field | Type | Required | Description |
|---|---|---|---|
| `jsonrpc` | [`JsonRpc`](/docs/reference/api/types#jsonrpc) | yes |  |
| `method` | string | yes |  |
| `params` | any |  | Default: `null`. |

**Rust:** [`crates/nh-proto/src/api/envelope.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/envelope.rs#L100)

### `NotificationId` {#notificationid}

The ULID of a notification, as 26 characters.

Type: string. Pattern: `^[0-9A-HJKMNP-TV-Z]{26}$`.

**Used by:** [`NotificationValue`](/docs/reference/api/types#notificationvalue)

### `NotificationValue` {#notificationvalue}

`notifications`.

| Field | Type | Required | Description |
|---|---|---|---|
| `at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `id` | [`NotificationId`](/docs/reference/api/types#notificationid) | yes |  |
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) \| `null` |  |  |
| `seen` | boolean | yes |  |
| `text` | string | yes |  |
| `urgency` | [`Urgency`](/docs/reference/api/types#urgency) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L736) · **Used by:** [projection `notifications`](/docs/reference/api/projections#notifications)

## P {#letter-p}

### `PairingState` {#pairingstate}

The state of the offer of a pairing.

| Value | Meaning |
|---|---|
| `"none"` | No offer. |
| `"waiting"` | An offer waits for a phone. |
| `"paired"` | A phone paired with the offer. |
| `"expired"` | The offer ended: its time ran out, or five codes were wrong. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1200) · **Used by:** [`PhonePairingValue`](/docs/reference/api/types#phonepairingvalue)

### `PaletteCommand` {#palettecommand}

| Field | Type | Required | Description |
|---|---|---|---|
| `category` | string | yes |  |
| `disabled_reason` | string \| `null` |  | Why a command is disabled, for the hint. |
| `enabled` | boolean | yes |  |
| `id` | string | yes |  |
| `shortcut` | string \| `null` |  |  |
| `title` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L846) · **Used by:** [`PaletteMatch`](/docs/reference/api/types#palettematch), [`PaletteValue`](/docs/reference/api/types#palettevalue)

### `PaletteExecute` {#paletteexecute}

| Field | Type | Required | Description |
|---|---|---|---|
| `args` | any |  |  |
| `command` | string | yes | The id of the command (`PaletteCommand.id`). |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  | The window that runs the command. A command on the tab or the pane in focus needs it. |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L398) · **Used by:** [`palette.execute`](/docs/reference/api/methods/palette#palette-execute)

### `PaletteExecuted` {#paletteexecuted}

What `palette.execute` did. Some commands belong to the page and not to the core, for example the inbox. The core then names the action, and the page does it.

| Field | Type | Required | Description |
|---|---|---|---|
| `ui` | string \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L410) · **Used by:** [`palette.execute`](/docs/reference/api/methods/palette#palette-execute)

### `PaletteMatch` {#palettematch}

| Field | Type | Required | Description |
|---|---|---|---|
| `command` | [`PaletteCommand`](/docs/reference/api/types#palettecommand) | yes |  |
| `ranges` | any[][] | yes | The matched ranges of the title, in UTF-16 code units. |
| `score` | number (float) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L387) · **Used by:** [`PaletteResults`](/docs/reference/api/types#paletteresults)

### `PaletteQuery` {#palettequery}

| Field | Type | Required | Description |
|---|---|---|---|
| `limit` | integer (uint32) \| `null` |  |  |
| `query` | string | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  | The window that asks. A later version ranks the commands of that window first. |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L378) · **Used by:** [`palette.query`](/docs/reference/api/methods/palette#palette-query)

### `PaletteResults` {#paletteresults}

| Field | Type | Required | Description |
|---|---|---|---|
| `items` | [`PaletteMatch`](/docs/reference/api/types#palettematch)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L394) · **Used by:** [`palette.query`](/docs/reference/api/methods/palette#palette-query)

### `PaletteValue` {#palettevalue}

`palette`: the command list with the enablement of the moment. It has one key, `_`.

| Field | Type | Required | Description |
|---|---|---|---|
| `commands` | [`PaletteCommand`](/docs/reference/api/types#palettecommand)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L859) · **Used by:** [projection `palette`](/docs/reference/api/projections#palette)

### `PaneAck` {#paneack}

One painted frame. `pane` is the render id that the frames carry.

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | integer (uint32) | yes |  |
| `rev` | integer (uint64) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L464) · **Used by:** [`RenderAck`](/docs/reference/api/types#renderack)

### `PaneBell` {#panebell}

The program in the pane rang the bell. The core sends at most one event in 2 seconds for a pane.

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L193) · **Used by:** [event `pane.bell`](/docs/reference/api/events#pane-bell)

### `PaneClaim` {#paneclaim}

The window takes the geometry lease of the pane as an input does, with the same debounce, and sends no byte. A phone calls it when its keyboard opens.

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L46) · **Used by:** [`pane.claim`](/docs/reference/api/methods/pane#pane-claim)

### `PaneClipboardAllow` {#paneclipboardallow}

The answer to an OSC 52 clipboard write.

| Field | Type | Required | Description |
|---|---|---|---|
| `allow` | boolean | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `remember` | boolean |  | Later writes of this pane follow the answer without asking. Default: `false`. |
| `return_text` | boolean |  | Return the text, and write no clipboard of a host: a phone writes it in the tap. Default: `false`. |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L235) · **Used by:** [`pane.clipboard_allow`](/docs/reference/api/methods/pane#pane-clipboard-allow)

### `PaneClose` {#paneclose}

`confirm_required` when the child is alive and `force` is false. `details` has `{process, is_agent}`.

| Field | Type | Required | Description |
|---|---|---|---|
| `force` | boolean |  | Default: `false`. |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L32) · **Used by:** [`pane.close`](/docs/reference/api/methods/pane#pane-close)

### `PaneCopy` {#panecopy}

Writes the selection to the clipboard through the host.

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `return_text` | boolean |  | Also return the text, for the CLI and for a phone. A phone writes its own clipboard in the tap, so the core writes none for it. Default: `false`. |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L195) · **Used by:** [`pane.copy`](/docs/reference/api/methods/pane#pane-copy)

### `PaneCreated` {#panecreated}

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L25) · **Used by:** [`pane.split`](/docs/reference/api/methods/pane#pane-split)

### `PaneDropBlobs` {#panedropblobs}

Files dropped on a pane of a remote window. The client has put each file in the blob store over its link. The core writes each blob to a file of the machine of the pane, then quotes the paths and pastes them as `pane.drop_files` does.

| Field | Type | Required | Description |
|---|---|---|---|
| `blobs` | [`DroppedBlob`](/docs/reference/api/types#droppedblob)[] | yes |  |
| `input_id` | integer (uint64) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L129) · **Used by:** [`pane.drop_blobs`](/docs/reference/api/methods/pane#pane-drop-blobs)

### `PaneDropFiles` {#panedropfiles}

The core quotes the paths and pastes them.

| Field | Type | Required | Description |
|---|---|---|---|
| `input_id` | integer (uint64) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `paths` | string[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L136) · **Used by:** [`pane.drop_files`](/docs/reference/api/methods/pane#pane-drop-files)

### `PaneEvent` {#paneevent}

The payload of the `pane.*` events that name a pane and its place.

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L224) · **Used by:** [event `pane.created`](/docs/reference/api/events#pane-created), [event `pane.closed`](/docs/reference/api/events#pane-closed), [event `pane.moved`](/docs/reference/api/events#pane-moved), [event `pane.floated`](/docs/reference/api/events#pane-floated), [event `pane.unfloated`](/docs/reference/api/events#pane-unfloated), [event `pane.restarted`](/docs/reference/api/events#pane-restarted)

### `PaneExited` {#paneexited}

| Field | Type | Required | Description |
|---|---|---|---|
| `code` | integer (int32) \| `null` |  |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L185) · **Used by:** [event `pane.exited`](/docs/reference/api/events#pane-exited)

### `PaneFloat` {#panefloat}

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `rect` | [`Rect`](/docs/reference/api/types#rect) \| `null` |  | Where to put it. Without it the core picks a rectangle. |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L61) · **Used by:** [`pane.float`](/docs/reference/api/methods/pane#pane-float)

### `PaneFocus` {#panefocus}

Keyboard focus moved to a pane in a window.

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L39) · **Used by:** [`pane.focus`](/docs/reference/api/methods/pane#pane-focus)

### `PaneId` {#paneid}

The ULID of a pane, as 26 characters.

Type: string. Pattern: `^[0-9A-HJKMNP-TV-Z]{26}$`.

**Used by:** [`AgentIdentified`](/docs/reference/api/types#agentidentified), [`AgentValue`](/docs/reference/api/types#agentvalue), [`DebugScreen`](/docs/reference/api/types#debugscreen), [`DebugScreenResult`](/docs/reference/api/types#debugscreenresult), [`DebugSpawnedPane`](/docs/reference/api/types#debugspawnedpane), [`ExecutionNodeValue`](/docs/reference/api/types#executionnodevalue), [`FloatingPane`](/docs/reference/api/types#floatingpane), [`GitAgent`](/docs/reference/api/types#gitagent), [`InteractionOpened`](/docs/reference/api/types#interactionopened), [`InteractionValue`](/docs/reference/api/types#interactionvalue), [`LayoutNode`](/docs/reference/api/types#layoutnode), [`PaneBell`](/docs/reference/api/types#panebell), [`PaneCreated`](/docs/reference/api/types#panecreated), [`PaneEvent`](/docs/reference/api/types#paneevent), [`PaneExited`](/docs/reference/api/types#paneexited), [`PaneInputUnconfirmed`](/docs/reference/api/types#paneinputunconfirmed), [`PaneValue`](/docs/reference/api/types#panevalue), [`TabCreated`](/docs/reference/api/types#tabcreated), [`TabValue`](/docs/reference/api/types#tabvalue), [`WindowValue`](/docs/reference/api/types#windowvalue), [`WorkspaceCreated`](/docs/reference/api/types#workspacecreated)

### `PaneInput` {#paneinput}

One logical event, at once. `input_id` is unique for the core epoch. A holder drops a repeated id.

| Field | Type | Required | Description |
|---|---|---|---|
| `event` | [`LogicalInput`](/docs/reference/api/types#logicalinput) | yes |  |
| `input_id` | integer (uint64) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L100) · **Used by:** [`pane.input`](/docs/reference/api/methods/pane#pane-input)

### `PaneInputUnconfirmed` {#paneinputunconfirmed}

An input was in flight when the holder or the core went away.

| Field | Type | Required | Description |
|---|---|---|---|
| `input_id` | integer (uint64) | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `reason` | [`UnconfirmedReason`](/docs/reference/api/types#unconfirmedreason) | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L179) · **Used by:** [event `pane.input_unconfirmed`](/docs/reference/api/events#pane-input-unconfirmed)

### `PaneInsertPaths` {#paneinsertpaths}

Paths of the machine of the core, from the Files tab. The core quotes and pastes them as `pane.drop_files` does, and the host never uploads them.

| Field | Type | Required | Description |
|---|---|---|---|
| `input_id` | integer (uint64) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `paths` | string[] | yes | 1 to 20 absolute paths. |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L144) · **Used by:** [`pane.insert_paths`](/docs/reference/api/methods/pane#pane-insert-paths)

### `PaneList` {#panelist}

| Field | Type | Required | Description |
|---|---|---|---|
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) \| `null` |  |  |
| `workspace` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L246) · **Used by:** [`pane.list`](/docs/reference/api/methods/pane#pane-list)

### `PaneListResult` {#panelistresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `panes` | [`PaneValue`](/docs/reference/api/types#panevalue)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L253) · **Used by:** [`pane.list`](/docs/reference/api/methods/pane#pane-list)

### `PaneLiveness` {#paneliveness}

One of these forms:

#### `state: "starting"`

| Field | Type | Required | Description |
|---|---|---|---|
| `state` | `"starting"` | yes |  |

#### `state: "alive"`

| Field | Type | Required | Description |
|---|---|---|---|
| `state` | `"alive"` | yes |  |

#### `state: "exited"`

| Field | Type | Required | Description |
|---|---|---|---|
| `code` | integer (int32) \| `null` |  |  |
| `state` | `"exited"` | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L120) · **Used by:** [`PaneValue`](/docs/reference/api/types#panevalue)

### `PaneMove` {#panemove}

Moves a pane to another tab. Without `beside` the pane becomes a split of the root.

| Field | Type | Required | Description |
|---|---|---|---|
| `beside` | [`RefPaneId`](/docs/reference/api/types#refpaneid) \| `null` |  |  |
| `direction` | [`SplitDirection`](/docs/reference/api/types#splitdirection) \| `null` |  |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L52) · **Used by:** [`pane.move`](/docs/reference/api/methods/pane#pane-move)

### `PanePaste` {#panepaste}

| Field | Type | Required | Description |
|---|---|---|---|
| `input_id` | integer (uint64) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `text` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L106) · **Used by:** [`pane.paste`](/docs/reference/api/methods/pane#pane-paste)

### `PanePeekClose` {#panepeekclose}

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L230) · **Used by:** [`pane.peek_close`](/docs/reference/api/methods/pane#pane-peek-close)

### `PanePeekOpen` {#panepeekopen}

Starts low-rate frames for a pane that is not in the grid.

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L226) · **Used by:** [`pane.peek_open`](/docs/reference/api/methods/pane#pane-peek-open)

### `PanePopout` {#panepopout}

Moves a pane into a window of its own.

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L79) · **Used by:** [`pane.popout`](/docs/reference/api/methods/pane#pane-popout)

### `PaneRead` {#paneread}

Reads text from a pane. Without `lines` it reads the visible screen.

| Field | Type | Required | Description |
|---|---|---|---|
| `lines` | [`LineRange`](/docs/reference/api/types#linerange) \| `null` |  |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `visible` | boolean |  | Read the visible screen. It is the default when `lines` is absent. Default: `false`. |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L210) · **Used by:** [`pane.read`](/docs/reference/api/methods/pane#pane-read)

### `PaneResize` {#paneresize}

From the `ResizeObserver`. The geometry lease decides whether it resizes the PTY.

| Field | Type | Required | Description |
|---|---|---|---|
| `cols` | integer (uint16) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `rows` | integer (uint16) | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L91) · **Used by:** [`pane.resize`](/docs/reference/api/methods/pane#pane-resize)

### `PaneRestart` {#panerestart}

Starts the pane again after its child exited.

| Field | Type | Required | Description |
|---|---|---|---|
| `command` | [`ProcessSpec`](/docs/reference/api/types#processspec) \| `null` |  |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L84) · **Used by:** [`pane.restart`](/docs/reference/api/methods/pane#pane-restart)

### `PaneScroll` {#panescroll}

A viewport change. Frames follow.

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

Plus exactly one of these forms:

#### Form 1

The absolute line at the top of the viewport.

| Field | Type | Required | Description |
|---|---|---|---|
| `top_line` | integer (uint64) | yes |  |

#### Form 2

Lines up (negative) or down (positive).

| Field | Type | Required | Description |
|---|---|---|---|
| `delta` | integer (int64) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L160) · **Used by:** [`pane.scroll`](/docs/reference/api/methods/pane#pane-scroll)

### `PaneSelect` {#paneselect}

The selection lives in the core.

| Field | Type | Required | Description |
|---|---|---|---|
| `anchor` | [`SelectPoint`](/docs/reference/api/types#selectpoint) | yes |  |
| `head` | [`SelectPoint`](/docs/reference/api/types#selectpoint) | yes |  |
| `mode` | [`SelectMode`](/docs/reference/api/types#selectmode) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L183) · **Used by:** [`pane.select`](/docs/reference/api/methods/pane#pane-select)

### `PaneSelectClear` {#paneselectclear}

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L190) · **Used by:** [`pane.select_clear`](/docs/reference/api/methods/pane#pane-select-clear)

### `PaneSendBytes` {#panesendbytes}

Raw bytes, as the explicit exception to logical input.

| Field | Type | Required | Description |
|---|---|---|---|
| `bytes` | [`Bytes`](/docs/reference/api/types#bytes) | yes |  |
| `input_id` | integer (uint64) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L113) · **Used by:** [`pane.send_bytes`](/docs/reference/api/methods/pane#pane-send-bytes)

### `PaneSetFloatRect` {#panesetfloatrect}

At most once for each animation frame while the user drags.

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `rect` | [`Rect`](/docs/reference/api/types#rect) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L73) · **Used by:** [`pane.set_float_rect`](/docs/reference/api/methods/pane#pane-set-float-rect)

### `PaneShow` {#paneshow}

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L257) · **Used by:** [`pane.show`](/docs/reference/api/methods/pane#pane-show)

### `PaneShown` {#paneshown}

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`PaneValue`](/docs/reference/api/types#panevalue) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L261) · **Used by:** [`pane.show`](/docs/reference/api/methods/pane#pane-show)

### `PaneSplit` {#panesplit}

| Field | Type | Required | Description |
|---|---|---|---|
| `command` | [`ProcessSpec`](/docs/reference/api/types#processspec) \| `null` |  |  |
| `direction` | [`SplitDirection`](/docs/reference/api/types#splitdirection) | yes | Which side of `pane` the new pane goes on. |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes | The pane to split. |
| `ratio` | number (float) \| `null` |  | The share of the new pane, from 0 to 1. Without it the two panes share the room. |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L13) · **Used by:** [`pane.split`](/docs/reference/api/methods/pane#pane-split)

### `PaneText` {#panetext}

| Field | Type | Required | Description |
|---|---|---|---|
| `from_line` | integer (uint64) | yes | The absolute line of the first line of `text`. |
| `text` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L219) · **Used by:** [`pane.read`](/docs/reference/api/methods/pane#pane-read)

### `PaneUnfloat` {#paneunfloat}

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L68) · **Used by:** [`pane.unfloat`](/docs/reference/api/methods/pane#pane-unfloat)

### `PaneValue` {#panevalue}

`panes`: one pane.

| Field | Type | Required | Description |
|---|---|---|---|
| `alt_screen` | boolean |  | Default: `false`. |
| `bracketed_paste` | boolean |  | Default: `false`. |
| `clipboard_dropped` | [`ClipboardDropped`](/docs/reference/api/types#clipboarddropped) \| `null` |  | The last OSC 52 write that no client got. The next write that a client gets clears it. |
| `clipboard_request` | [`ClipboardRequest`](/docs/reference/api/types#clipboardrequest) \| `null` |  |  |
| `cols` | integer (uint16) | yes |  |
| `cwd` | string \| `null` |  |  |
| `fg_process` | string \| `null` |  | The foreground process. |
| `geometry_owner` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  | The window whose size the PTY follows. |
| `handle` | string | yes | The short handle, for example `w3:p5`. |
| `id` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `input_unconfirmed` | integer (uint64)[] |  | Inputs that were in flight when something went away (`pane.input_unconfirmed`). |
| `liveness` | [`PaneLiveness`](/docs/reference/api/types#paneliveness) | yes |  |
| `mouse_mode` | [`MouseMode`](/docs/reference/api/types#mousemode) | yes |  |
| `render_id` | integer (uint32) | yes | The number that frames carry for this pane. |
| `restoring` | boolean |  | The pane shows the restored screen and the holder has not caught up. Default: `false`. |
| `rows` | integer (uint16) | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) \| `null` |  | The run of an agent in the pane. |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |
| `title` | string | yes |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L159) · **Used by:** [projection `panes`](/docs/reference/api/projections#panes), [`PaneListResult`](/docs/reference/api/types#panelistresult), [`PaneShown`](/docs/reference/api/types#paneshown)

### `PatchOp` {#patchop}

One change of a patch.

One of these forms:

#### `op: "set"`

Sets the value of a key. The key is new or it exists.

| Field | Type | Required | Description |
|---|---|---|---|
| `key` | string | yes |  |
| `op` | `"set"` | yes |  |
| `value` | any | yes |  |

#### `op: "remove"`

| Field | Type | Required | Description |
|---|---|---|---|
| `key` | string | yes |  |
| `op` | `"remove"` | yes |  |

#### `op: "order"`

Sets the order of all keys.

| Field | Type | Required | Description |
|---|---|---|---|
| `keys` | string[] | yes |  |
| `op` | `"order"` | yes |  |

**Rust:** [`crates/nh-proto/src/api/projections.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/projections.rs#L263) · **Used by:** [`ProjectionMessage`](/docs/reference/api/types#projectionmessage)

### `PeekParams` {#peekparams}

Opens the `peek` stream of a pane. A keyframe holds the bottom `max_rows` rows and the left `max_cols` columns.

| Field | Type | Required | Description |
|---|---|---|---|
| `max_cols` | integer (uint16) | yes |  |
| `max_hz` | number (float) |  | Default: `4`. |
| `max_rows` | integer (uint16) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/streams.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/streams.rs#L41) · **Used by:** [stream `peek`](/docs/reference/api/streams#peek)

### `PhoneEvent` {#phoneevent}

The payload of `phone.paired`, `phone.renamed` and `phone.removed`. The event never holds a code or a token.

| Field | Type | Required | Description |
|---|---|---|---|
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) | yes |  |
| `name` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L245) · **Used by:** [event `phone.paired`](/docs/reference/api/events#phone-paired), [event `phone.renamed`](/docs/reference/api/events#phone-renamed), [event `phone.removed`](/docs/reference/api/events#phone-removed)

### `PhonePairBegin` {#phonepairbegin}

`phone.pair_begin`: a new offer of a pairing. It ends the offer before it.

An object with no fields: `{}`.

**Rust:** [`crates/nh-proto/src/api/calls/phone.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/phone.rs#L13) · **Used by:** [`phone.pair_begin`](/docs/reference/api/methods/phone#phone-pair-begin)

### `PhonePairStarted` {#phonepairstarted}

The offer, also in the projection `phone_pairing`. The result has data so that the CLI can print the URL and the short code.

| Field | Type | Required | Description |
|---|---|---|---|
| `code` | string | yes |  |
| `expires_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `url` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/phone.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/phone.rs#L17) · **Used by:** [`phone.pair_begin`](/docs/reference/api/methods/phone#phone-pair-begin)

### `PhonePairingValue` {#phonepairingvalue}

`phone_pairing`: the offer of a pairing. It has one key, `_`. A phone cannot follow it, because it holds the codes.

| Field | Type | Required | Description |
|---|---|---|---|
| `code` | string \| `null` |  | The short code that a person types: 8 Crockford base32 characters, as `ABCD-EFGH`. |
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) \| `null` |  | The phone that paired with the offer. |
| `expires_at` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  | When the offer ends. |
| `note` | string \| `null` |  | The time that the offer is good for, as a person reads it, for example "The code is good for 10 minutes and for one phone.". The page shows it as it is. |
| `qr` | string \| `null` |  | The QR code of `url` as an SVG `data:` URL. The page shows it in an image. |
| `state` | [`PairingState`](/docs/reference/api/types#pairingstate) | yes |  |
| `url` | string \| `null` |  | The URL of the QR code: the public origin, `/pair?code=` and the long code. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1213) · **Used by:** [projection `phone_pairing`](/docs/reference/api/projections#phone-pairing)

### `PhonePushFailed` {#phonepushfailed}

The payload of `phone.push_failed`: a push of a run did not reach the push service of a phone.

| Field | Type | Required | Description |
|---|---|---|---|
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) | yes |  |
| `status` | integer (uint16) \| `null` |  | The last status of the push service. `None` when no answer came. |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L260) · **Used by:** [event `phone.push_failed`](/docs/reference/api/events#phone-push-failed)

### `PhonePushSubscribe` {#phonepushsubscribe}

`phone.push_subscribe`: the push subscription of the phone that calls. Only a phone calls it, for itself. A new subscription replaces the old one.

| Field | Type | Required | Description |
|---|---|---|---|
| `auth` | string | yes | The authentication secret of the browser (16 bytes), base64url. |
| `endpoint` | string | yes | The endpoint of the push service. It must be on the allowlist of push services. |
| `p256dh` | string | yes | The public key of the browser (P-256, uncompressed), base64url. |

**Rust:** [`crates/nh-proto/src/api/calls/phone.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/phone.rs#L38) · **Used by:** [`phone.push_subscribe`](/docs/reference/api/methods/phone#phone-push-subscribe)

### `PhonePushTest` {#phonepushtest}

`phone.push_test`: sends the push "Test notification" to a phone.

| Field | Type | Required | Description |
|---|---|---|---|
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/phone.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/phone.rs#L48) · **Used by:** [`phone.push_test`](/docs/reference/api/methods/phone#phone-push-test)

### `PhonePushed` {#phonepushed}

The payload of `phone.pushed`: the push service took a push of a run for a phone. The event holds the reason, never the text of the push.

| Field | Type | Required | Description |
|---|---|---|---|
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) | yes |  |
| `reason` | [`AttentionReason`](/docs/reference/api/types#attentionreason) | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L252) · **Used by:** [event `phone.pushed`](/docs/reference/api/events#phone-pushed)

### `PhoneRemove` {#phoneremove}

`phone.remove`: removes a paired phone, closes its connections and its streams, and removes its phone windows.

| Field | Type | Required | Description |
|---|---|---|---|
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/phone.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/phone.rs#L25) · **Used by:** [`phone.remove`](/docs/reference/api/methods/phone#phone-remove)

### `PhoneRename` {#phonerename}

`phone.rename`: the name of a paired phone.

| Field | Type | Required | Description |
|---|---|---|---|
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) | yes |  |
| `name` | string | yes | 1 to 64 characters. |

**Rust:** [`crates/nh-proto/src/api/calls/phone.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/phone.rs#L30) · **Used by:** [`phone.rename`](/docs/reference/api/methods/phone#phone-rename)

### `PhoneReplied` {#phonereplied}

The payload of `phone.replied`: a phone sent `agent.reply` to a run. The event does not hold the text.

| Field | Type | Required | Description |
|---|---|---|---|
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L269) · **Used by:** [event `phone.replied`](/docs/reference/api/events#phone-replied)

### `PhoneStatusValue` {#phonestatusvalue}

`phone_status`: the listener of the phone. It has one key, `_`.

| Field | Type | Required | Description |
|---|---|---|---|
| `hint` | string \| `null` |  | What the person can do when the listener cannot serve a phone. `phone.pair_begin` refuses while there is a hint. |
| `listening` | boolean | yes | Whether the listener serves its port. |
| `origin` | string \| `null` |  | The HTTPS origin that a phone uses, for example `https://studio.example.ts.net:8443`. |
| `port` | integer (uint16) \| `null` |  | The port of `127.0.0.1`, when the listener has one. |
| `summary` | string |  | One line about the listener, for the settings window and the doctor, for example "Published with Tailscale at https://studio.example.ts.net:8443.". Default: `""`. |
| `tailscale` | [`TailscaleStatus`](/docs/reference/api/types#tailscalestatus) \| `null` |  | Tailscale. `null` when the listener does not use it: the listener is off, `phone.url` names the origin, or the run is a development run. |
| `vapid_key` | string \| `null` |  | The public VAPID key of this core: the uncompressed P-256 point, base64url. The phone app subscribes to push with it. `null` while the listener does not run. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1237) · **Used by:** [`phone.publish`](/docs/reference/api/methods/phone#phone-publish), [`phone.unpublish`](/docs/reference/api/methods/phone#phone-unpublish), [projection `phone_status`](/docs/reference/api/projections#phone-status)

### `PhoneValue` {#phonevalue}

`phones`: a paired phone. The key is the device id. The core orders the rows, the phone paired last first.

| Field | Type | Required | Description |
|---|---|---|---|
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) | yes |  |
| `name` | string | yes | The name that the phone gave when it paired, or that `phone.rename` set. |
| `paired_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `push` | boolean |  | Whether the phone has a push subscription. Default: `false`. |
| `seen_at` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  | The time of the last connection of the phone, or `null` before the first one. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1184) · **Used by:** [projection `phones`](/docs/reference/api/projections#phones)

### `PhoneVisible` {#phonevisible}

`phone.visible`: the phone app of the phone that calls is open and visible, or not. The app calls it on each `visibilitychange`. A visible phone gets no push. The core keeps it in memory for the connection, and the end of the connection makes it false.

| Field | Type | Required | Description |
|---|---|---|---|
| `visible` | boolean | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/phone.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/phone.rs#L55) · **Used by:** [`phone.visible`](/docs/reference/api/methods/phone#phone-visible)

### `PhoneWindowResult` {#phonewindowresult}

The result of `phone.window`: the phone window of the device that calls, on this core. The core makes the row when the device has none.

| Field | Type | Required | Description |
|---|---|---|---|
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/phone.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/phone.rs#L61) · **Used by:** [`phone.window`](/docs/reference/api/methods/phone#phone-window)

### `Placement` {#placement}

Where an agent starts.

One of these forms:

#### `type: "new_tab"`

A new tab of the workspace.

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `"new_tab"` | yes |  |
| `workspace` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) | yes |  |

#### `type: "split"`

A new pane beside an existing one.

| Field | Type | Required | Description |
|---|---|---|---|
| `direction` | [`SplitDirection`](/docs/reference/api/types#splitdirection) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `type` | `"split"` | yes |  |

#### `type: "pane"`

An existing pane whose shell is idle. The core types the command there.

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `type` | `"pane"` | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L35) · **Used by:** [`AgentResume`](/docs/reference/api/types#agentresume), [`AgentStart`](/docs/reference/api/types#agentstart)

### `PlanDecision` {#plandecision}

| Field | Type | Required | Description |
|---|---|---|---|
| `approve` | boolean | yes |  |
| `feedback` | string \| `null` |  | With `approve: false`, the feedback becomes the deny message. |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L268) · **Used by:** [`Decision`](/docs/reference/api/types#decision)

### `PlanPayload` {#planpayload}

| Field | Type | Required | Description |
|---|---|---|---|
| `plan` | [`Markdown`](/docs/reference/api/types#markdown) | yes |  |
| `plan_path` | string \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L481) · **Used by:** [`InteractionPayload`](/docs/reference/api/types#interactionpayload)

### `PolicyAdd` {#policyadd}

| Field | Type | Required | Description |
|---|---|---|---|
| `rule` | [`PolicyRule`](/docs/reference/api/types#policyrule) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L173) · **Used by:** [`policy.add`](/docs/reference/api/methods/policy#policy-add)

### `PolicyAdded` {#policyadded}

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L177) · **Used by:** [`policy.add`](/docs/reference/api/methods/policy#policy-add)

### `PolicyDecision` {#policydecision}

Values: `"allow"`, `"deny"`.

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L123) · **Used by:** [`PolicyRule`](/docs/reference/api/types#policyrule)

### `PolicyEvent` {#policyevent}

The payload of `policy.added` and `policy.removed`.

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes |  |
| `scope` | [`PolicyScope`](/docs/reference/api/types#policyscope) | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L316) · **Used by:** [event `policy.added`](/docs/reference/api/events#policy-added), [event `policy.removed`](/docs/reference/api/events#policy-removed)

### `PolicyList` {#policylist}

An object with no fields: `{}`.

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L167) · **Used by:** [`policy.list`](/docs/reference/api/methods/policy#policy-list)

### `PolicyListResult` {#policylistresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `rules` | [`PolicyRuleValue`](/docs/reference/api/types#policyrulevalue)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L169) · **Used by:** [`policy.list`](/docs/reference/api/methods/policy#policy-list)

### `PolicyRemove` {#policyremove}

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L181) · **Used by:** [`policy.remove`](/docs/reference/api/methods/policy#policy-remove)

### `PolicyRule` {#policyrule}

A rule.

| Field | Type | Required | Description |
|---|---|---|---|
| `command` | string \| `null` |  | For `Bash`: a prefix glob on the normalized command. `*` only at the end. |
| `decision` | [`PolicyDecision`](/docs/reference/api/types#policydecision) | yes |  |
| `harness` | [`Harness`](/docs/reference/api/types#harness)[] |  | Default: any harness. |
| `id` | string \| `null` |  | Made by the core if absent. |
| `paths` | string[] |  | For file tools: globs relative to the workspace root. |
| `reason` | string \| `null` |  |  |
| `tool` | string | yes | The exact name of the tool, or `*`. |
| `workspace` | string \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L129) · **Used by:** [`PolicyAdd`](/docs/reference/api/types#policyadd), [`PolicyRuleValue`](/docs/reference/api/types#policyrulevalue), [`PolicySuggestion`](/docs/reference/api/types#policysuggestion)

### `PolicyRuleValue` {#policyrulevalue}

| Field | Type | Required | Description |
|---|---|---|---|
| `created_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `id` | string | yes |  |
| `rule` | [`PolicyRule`](/docs/reference/api/types#policyrule) | yes |  |
| `scope` | [`PolicyScope`](/docs/reference/api/types#policyscope) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L160) · **Used by:** [`PolicyListResult`](/docs/reference/api/types#policylistresult)

### `PolicyScope` {#policyscope}

| Value | Meaning |
|---|---|
| `"config"` | From the config file. |
| `"session"` | From "Allow always" in a card. |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L153) · **Used by:** [`PolicyEvent`](/docs/reference/api/types#policyevent), [`PolicyRuleValue`](/docs/reference/api/types#policyrulevalue)

### `PolicySuggest` {#policysuggest}

An object with no fields: `{}`.

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L185) · **Used by:** [`policy.suggest`](/docs/reference/api/methods/policy#policy-suggest)

### `PolicySuggestResult` {#policysuggestresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `suggestions` | [`PolicySuggestion`](/docs/reference/api/types#policysuggestion)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L197) · **Used by:** [`policy.suggest`](/docs/reference/api/methods/policy#policy-suggest)

### `PolicySuggestion` {#policysuggestion}

A rule that the user could add: approved at least 5 times, never denied, in 30 days.

| Field | Type | Required | Description |
|---|---|---|---|
| `approvals` | integer (uint32) | yes |  |
| `fingerprint` | string | yes |  |
| `last_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `rule` | [`PolicyRule`](/docs/reference/api/types#policyrule) | yes |  |
| `toml` | string | yes | The rule as a `[[policy.rule]]` block, ready to paste into `config.toml`. |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L188) · **Used by:** [`PolicySuggestResult`](/docs/reference/api/types#policysuggestresult)

### `PortForward` {#portforward}

A port that the client forwards to a machine.

| Field | Type | Required | Description |
|---|---|---|---|
| `local_port` | integer (uint16) | yes |  |
| `remote_port` | integer (uint16) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L765) · **Used by:** [`MachineValue`](/docs/reference/api/types#machinevalue)

### `ProcessSpec` {#processspec}

What to run in a new pane. Without it the pane runs the login shell of the user.

| Field | Type | Required | Description |
|---|---|---|---|
| `argv` | string[] | yes | The program and its arguments. |
| `cwd` | string \| `null` |  |  |
| `env` | map of string to string |  | Extra variables for the environment of the process. |

**Rust:** [`crates/nh-proto/src/api/common.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/common.rs#L81) · **Used by:** [`PaneRestart`](/docs/reference/api/types#panerestart), [`PaneSplit`](/docs/reference/api/types#panesplit), [`TabCreate`](/docs/reference/api/types#tabcreate), [`WorkspaceCreate`](/docs/reference/api/types#workspacecreate)

### `Projection` {#projection}

The name of a projection. timeline:&lt;run>, pane_text:&lt;pane>, agents_for:&lt;window>, changes_for:&lt;window>, changes_head:&lt;window>, git_head:&lt;window>, git_worktrees:&lt;window>, git_branches:&lt;window>, git_changes:&lt;window> and hermes_chat:&lt;chat> take an id.

Type: string. Pattern: `^(windows|tree|panes|agents|attention|interactions|notifications|machines|harnesses|limits|jobs|palette|settings|phones|phone_pairing|phone_status|git_repos|workflows|workflow_executions|hermes_chats|health|link|timeline:[0-9A-HJKMNP-TV-Z]{26}|pane_text:[0-9A-HJKMNP-TV-Z]{26}|agents_for:[0-9A-HJKMNP-TV-Z]{26}|changes_for:[0-9A-HJKMNP-TV-Z]{26}|changes_head:[0-9A-HJKMNP-TV-Z]{26}|git_head:[0-9A-HJKMNP-TV-Z]{26}|git_worktrees:[0-9A-HJKMNP-TV-Z]{26}|git_branches:[0-9A-HJKMNP-TV-Z]{26}|git_changes:[0-9A-HJKMNP-TV-Z]{26}|hermes_chat:[0-9A-HJKMNP-TV-Z]{26})$`.

**Rust:** [`crates/nh-proto/src/api/projections.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/projections.rs#L29) · **Used by:** [`ProjectionSubscribe`](/docs/reference/api/types#projectionsubscribe), [`ProjectionVersions`](/docs/reference/api/types#projectionversions)

### `ProjectionData` {#projectiondata}

The state of a projection: the values by key, and the order of the keys.

| Field | Type | Required | Description |
|---|---|---|---|
| `items` | map of string to any | yes |  |
| `order` | string[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/projections.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/projections.rs#L256) · **Used by:** [`ProjectionMessage`](/docs/reference/api/types#projectionmessage)

### `ProjectionMessage` {#projectionmessage}

What a subscription delivers: a snapshot once, then patches. `resync` means that the client is older than the core retains, and it subscribes again.

One of these forms:

#### `type: "snapshot"`

| Field | Type | Required | Description |
|---|---|---|---|
| `data` | [`ProjectionData`](/docs/reference/api/types#projectiondata) | yes |  |
| `type` | `"snapshot"` | yes |  |
| `version` | integer (uint64) | yes |  |

#### `type: "patch"`

| Field | Type | Required | Description |
|---|---|---|---|
| `ops` | [`PatchOp`](/docs/reference/api/types#patchop)[] | yes |  |
| `type` | `"patch"` | yes |  |
| `version` | integer (uint64) | yes |  |

#### `type: "resync"`

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `"resync"` | yes |  |

**Rust:** [`crates/nh-proto/src/api/projections.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/projections.rs#L274) · **Used by:** [`ProjectionNotification`](/docs/reference/api/types#projectionnotification)

### `ProjectionNotification` {#projectionnotification}

The params of the notification `projection.message`.

| Field | Type | Required | Description |
|---|---|---|---|
| `message` | [`ProjectionMessage`](/docs/reference/api/types#projectionmessage) | yes |  |
| `subscription` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L74)

### `ProjectionSubscribe` {#projectionsubscribe}

`projection.subscribe`. Messages arrive as `projection.message` notifications.

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | [`Projection`](/docs/reference/api/types#projection) | yes |  |
| `since` | integer (uint64) \| `null` |  | The version the client has. The core sends only patches if it still retains them. |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L58) · **Used by:** [`projection.subscribe`](/docs/reference/api/methods/projection#projection-subscribe)

### `ProjectionSubscribed` {#projectionsubscribed}

| Field | Type | Required | Description |
|---|---|---|---|
| `subscription` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L65) · **Used by:** [`projection.subscribe`](/docs/reference/api/methods/projection#projection-subscribe)

### `ProjectionUnsubscribe` {#projectionunsubscribe}

| Field | Type | Required | Description |
|---|---|---|---|
| `subscription` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L69) · **Used by:** [`projection.unsubscribe`](/docs/reference/api/methods/projection#projection-unsubscribe)

### `ProjectionVersions` {#projectionversions}

`projection.versions {names}`: the version of each projection named. A client compares them with the versions of its subscriptions, because a subscription can stop without a gap in its versions: a channel of the host can lose one message and hold the ones after it, and the core drops a sink whose send fails.

| Field | Type | Required | Description |
|---|---|---|---|
| `names` | [`Projection`](/docs/reference/api/types#projection)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L83) · **Used by:** [`projection.versions`](/docs/reference/api/methods/projection#projection-versions)

### `ProjectionVersionsResult` {#projectionversionsresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `versions` | map of string to integer (uint64) | yes | The version by name. A projection that the core has not made is left out. |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L87) · **Used by:** [`projection.versions`](/docs/reference/api/methods/projection#projection-versions)

### `Protocols` {#protocols}

The versions of the wire formats of this core.

| Field | Type | Required | Description |
|---|---|---|---|
| `frame` | integer (uint16) | yes | `nh_proto::version::FRAME_PROTO`. |
| `holder` | integer (uint16) | yes | `nh_proto::version::HOLDER_PROTO`. |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L19) · **Used by:** [`SessionInfo`](/docs/reference/api/types#sessioninfo)

### `Provenance` {#provenance}

Where a facet came from.

| Field | Type | Required | Description |
|---|---|---|---|
| `at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `confidence` | number (float) | yes | From 0 to 1. |
| `source` | [`Source`](/docs/reference/api/types#source) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L206) · **Used by:** [`ExecutionFacet`](/docs/reference/api/types#executionfacet), [`HealthFacet`](/docs/reference/api/types#healthfacet), [`LivenessFacet`](/docs/reference/api/types#livenessfacet)

## Q {#letter-q}

### `Question` {#question}

| Field | Type | Required | Description |
|---|---|---|---|
| `header` | string \| `null` |  |  |
| `multi_select` | boolean |  | Default: `false`. |
| `options` | [`QuestionOption`](/docs/reference/api/types#questionoption)[] | yes |  |
| `question` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L465) · **Used by:** [`QuestionPayload`](/docs/reference/api/types#questionpayload)

### `QuestionAnswer` {#questionanswer}

| Field | Type | Required | Description |
|---|---|---|---|
| `question` | string | yes | The text of the question. |
| `selected` | string[] | yes | The labels of the chosen options. |
| `text` | string \| `null` |  | Free text, when the user typed an answer. |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L254) · **Used by:** [`QuestionDecision`](/docs/reference/api/types#questiondecision)

### `QuestionDecision` {#questiondecision}

| Field | Type | Required | Description |
|---|---|---|---|
| `answers` | [`QuestionAnswer`](/docs/reference/api/types#questionanswer)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L264) · **Used by:** [`Decision`](/docs/reference/api/types#decision)

### `QuestionOption` {#questionoption}

| Field | Type | Required | Description |
|---|---|---|---|
| `description` | string \| `null` |  |  |
| `label` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L459) · **Used by:** [`Question`](/docs/reference/api/types#question)

### `QuestionPayload` {#questionpayload}

| Field | Type | Required | Description |
|---|---|---|---|
| `context` | string \| `null` |  | The last message of the agent, as context. |
| `questions` | [`Question`](/docs/reference/api/types#question)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L474) · **Used by:** [`InteractionPayload`](/docs/reference/api/types#interactionpayload)

## R {#letter-r}

### `RateLimitWindow` {#ratelimitwindow}

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `resets_at` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  |  |
| `used_percent` | number (float) | yes | From 0 to 100. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L302) · **Used by:** [`AgentValue`](/docs/reference/api/types#agentvalue), [`LimitValue`](/docs/reference/api/types#limitvalue)

### `ReadState` {#readstate}

The read state of the run for one client. The core keeps it for each subscription.

Values: `"seen"`, `"unseen"`.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L248) · **Used by:** [`Facets`](/docs/reference/api/types#facets)

### `Rect` {#rect}

A rectangle as fractions of the area of the tab, from 0 to 1. A floating pane has one.

| Field | Type | Required | Description |
|---|---|---|---|
| `h` | number (float) | yes |  |
| `w` | number (float) | yes |  |
| `x` | number (float) | yes |  |
| `y` | number (float) | yes |  |

**Rust:** [`crates/nh-proto/src/api/common.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/common.rs#L43) · **Used by:** [`FloatingPane`](/docs/reference/api/types#floatingpane), [`PaneFloat`](/docs/reference/api/types#panefloat), [`PaneSetFloatRect`](/docs/reference/api/types#panesetfloatrect)

### `RefGroupId` {#refgroupid}

A reference to a group: its ULID.

Type: string.

**Used by:** [`GroupClose`](/docs/reference/api/types#groupclose), [`GroupMove`](/docs/reference/api/types#groupmove), [`GroupRename`](/docs/reference/api/types#grouprename), [`GroupShow`](/docs/reference/api/types#groupshow), [`WorkspaceCreate`](/docs/reference/api/types#workspacecreate), [`WorkspaceList`](/docs/reference/api/types#workspacelist), [`WorkspaceMove`](/docs/reference/api/types#workspacemove)

### `RefInteractionId` {#refinteractionid}

A reference to an interaction: its ULID, or its short handle.

Type: string.

**Used by:** [`DebugAnswer`](/docs/reference/api/types#debuganswer), [`InteractionAnswer`](/docs/reference/api/types#interactionanswer), [`InteractionDismiss`](/docs/reference/api/types#interactiondismiss), [`InteractionNext`](/docs/reference/api/types#interactionnext), [`InteractionReveal`](/docs/reference/api/types#interactionreveal), [`InteractionShow`](/docs/reference/api/types#interactionshow)

### `RefPaneId` {#refpaneid}

A reference to a pane: its ULID, or its short handle.

Type: string.

**Used by:** [`AgentLink`](/docs/reference/api/types#agentlink), [`AgentList`](/docs/reference/api/types#agentlist), [`InteractionList`](/docs/reference/api/types#interactionlist), [`LayoutZoom`](/docs/reference/api/types#layoutzoom), [`PaneClaim`](/docs/reference/api/types#paneclaim), [`PaneClipboardAllow`](/docs/reference/api/types#paneclipboardallow), [`PaneClose`](/docs/reference/api/types#paneclose), [`PaneCopy`](/docs/reference/api/types#panecopy), [`PaneDropBlobs`](/docs/reference/api/types#panedropblobs), [`PaneDropFiles`](/docs/reference/api/types#panedropfiles), [`PaneFloat`](/docs/reference/api/types#panefloat), [`PaneFocus`](/docs/reference/api/types#panefocus), [`PaneInput`](/docs/reference/api/types#paneinput), [`PaneInsertPaths`](/docs/reference/api/types#paneinsertpaths), [`PaneMove`](/docs/reference/api/types#panemove), [`PanePaste`](/docs/reference/api/types#panepaste), [`PanePeekClose`](/docs/reference/api/types#panepeekclose), [`PanePeekOpen`](/docs/reference/api/types#panepeekopen), [`PanePopout`](/docs/reference/api/types#panepopout), [`PaneRead`](/docs/reference/api/types#paneread), [`PaneResize`](/docs/reference/api/types#paneresize), [`PaneRestart`](/docs/reference/api/types#panerestart), [`PaneScroll`](/docs/reference/api/types#panescroll), [`PaneSelect`](/docs/reference/api/types#paneselect), …

### `RefRunId` {#refrunid}

A reference to an agent run: its ULID, or its unique name.

Type: string.

**Used by:** [`AgentInterrupt`](/docs/reference/api/types#agentinterrupt), [`AgentMove`](/docs/reference/api/types#agentmove), [`AgentPrompt`](/docs/reference/api/types#agentprompt), [`AgentReply`](/docs/reference/api/types#agentreply), [`AgentResume`](/docs/reference/api/types#agentresume), [`AgentShow`](/docs/reference/api/types#agentshow), [`AgentStop`](/docs/reference/api/types#agentstop), [`AgentUnlink`](/docs/reference/api/types#agentunlink), [`AgentWait`](/docs/reference/api/types#agentwait), [`InteractionList`](/docs/reference/api/types#interactionlist), [`TimelineItemGet`](/docs/reference/api/types#timelineitemget), [`TimelinePageParams`](/docs/reference/api/types#timelinepageparams), [`WorkflowNotesFrom`](/docs/reference/api/types#workflownotesfrom)

### `RefTabId` {#reftabid}

A reference to a tab: its ULID, or its short handle.

Type: string.

**Used by:** [`LayoutEqualize`](/docs/reference/api/types#layoutequalize), [`LayoutSetRatios`](/docs/reference/api/types#layoutsetratios), [`LayoutZoom`](/docs/reference/api/types#layoutzoom), [`PaneList`](/docs/reference/api/types#panelist), [`PaneMove`](/docs/reference/api/types#panemove), [`TabClose`](/docs/reference/api/types#tabclose), [`TabMove`](/docs/reference/api/types#tabmove), [`TabRename`](/docs/reference/api/types#tabrename), [`TabSelect`](/docs/reference/api/types#tabselect), [`WindowOpen`](/docs/reference/api/types#windowopen)

### `RefWorkspaceId` {#refworkspaceid}

A reference to a workspace: its ULID, or its short handle.

Type: string.

**Used by:** [`AgentList`](/docs/reference/api/types#agentlist), [`PaneList`](/docs/reference/api/types#panelist), [`Placement`](/docs/reference/api/types#placement), [`TabCreate`](/docs/reference/api/types#tabcreate), [`TabMove`](/docs/reference/api/types#tabmove), [`WorkspaceClose`](/docs/reference/api/types#workspaceclose), [`WorkspaceMove`](/docs/reference/api/types#workspacemove), [`WorkspaceRename`](/docs/reference/api/types#workspacerename), [`WorkspaceShow`](/docs/reference/api/types#workspaceshow)

### `RenderAck` {#renderack}

`render.ack`. The UI sends it at most once for each painted animation frame, and only when something changed.

| Field | Type | Required | Description |
|---|---|---|---|
| `acks` | [`PaneAck`](/docs/reference/api/types#paneack)[] |  | Default: `[]`. |
| `need_keyframe` | integer (uint32)[] |  | The panes whose frames the UI discarded. The core sends a keyframe for each. Default: `[]`. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L471) · **Used by:** [`render.ack`](/docs/reference/api/methods/render#render-ack)

### `RenderParams` {#renderparams}

Opens the `render` stream of a window.

| Field | Type | Required | Description |
|---|---|---|---|
| `proto` | integer (uint16) | yes | The version of the frame format that the client decodes (`FRAME_PROTO`). |
| `viewport_hz` | number (float) | yes | The refresh rate of the window. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/streams.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/streams.rs#L31) · **Used by:** [stream `render`](/docs/reference/api/streams#render)

### `RendererKind` {#rendererkind}

Which renderer paints a window.

Values: `"webgl2"`, `"canvas2d"`.

**Rust:** [`crates/nh-proto/src/api/common.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/common.rs#L136) · **Used by:** [`HealthValue`](/docs/reference/api/types#healthvalue), [`WindowReportMetrics`](/docs/reference/api/types#windowreportmetrics)

### `Request` {#request}

A request. A request without an `id` is a notification of the client, and gets no response.

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | [`RequestId`](/docs/reference/api/types#requestid) \| `null` |  |  |
| `jsonrpc` | [`JsonRpc`](/docs/reference/api/types#jsonrpc) | yes |  |
| `method` | string | yes |  |
| `params` | any |  | Default: `null`. |

**Rust:** [`crates/nh-proto/src/api/envelope.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/envelope.rs#L57)

### `RequestId` {#requestid}

The id of a request: a number or a string.

One of these forms:

- integer (int64)

- string

**Rust:** [`crates/nh-proto/src/api/envelope.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/envelope.rs#L50) · **Used by:** [`Request`](/docs/reference/api/types#request), [`Response`](/docs/reference/api/types#response)

### `Response` {#response}

A response. Exactly one of `result` and `error` is set. `id` is `null` when the request was too broken to have one.

| Field | Type | Required | Description |
|---|---|---|---|
| `error` | [`RpcError`](/docs/reference/api/types#rpcerror) \| `null` |  |  |
| `id` | [`RequestId`](/docs/reference/api/types#requestid) \| `null` |  |  |
| `jsonrpc` | [`JsonRpc`](/docs/reference/api/types#jsonrpc) | yes |  |
| `result` | any |  |  |

**Rust:** [`crates/nh-proto/src/api/envelope.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/envelope.rs#L69)

### `Reveal` {#reveal}

A request to a window: show the inbox, and focus the card of this interaction.

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `seq` | integer (uint64) | yes | Grows with each request, so that the same interaction can be asked for twice. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L60) · **Used by:** [`WindowValue`](/docs/reference/api/types#windowvalue)

### `RichDiff` {#richdiff}

| Field | Type | Required | Description |
|---|---|---|---|
| `a` | string | yes |  |
| `b` | string | yes |  |
| `lang` | string \| `null` |  |  |
| `path` | string \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L415) · **Used by:** [`rich.diff`](/docs/reference/api/methods/rich#rich-diff)

### `RichHighlight` {#richhighlight}

| Field | Type | Required | Description |
|---|---|---|---|
| `lang` | string | yes |  |
| `text` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L428) · **Used by:** [`rich.highlight`](/docs/reference/api/methods/rich#rich-highlight)

### `RichMarkdown` {#richmarkdown}

| Field | Type | Required | Description |
|---|---|---|---|
| `text` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L424) · **Used by:** [`rich.markdown`](/docs/reference/api/methods/rich#rich-markdown)

### `Risk` {#risk}

How risky a command or an action is.

Values: `"low"`, `"medium"`, `"high"`, `"unknown"`.

**Rust:** [`crates/nh-proto/src/api/common.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/common.rs#L94) · **Used by:** [`Command`](/docs/reference/api/types#command), [`InteractionOpened`](/docs/reference/api/types#interactionopened), [`InteractionValue`](/docs/reference/api/types#interactionvalue)

### `RpcError` {#rpcerror}

The `error` member of a response.

| Field | Type | Required | Description |
|---|---|---|---|
| `code` | integer (int32) | yes | The integer of JSON-RPC: a reserved one for a protocol error, else `-32000`. |
| `data` | [`ErrorData`](/docs/reference/api/types#errordata) \| `null` |  |  |
| `message` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/envelope.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/envelope.rs#L361) · **Used by:** [`Response`](/docs/reference/api/types#response)

### `RunId` {#runid}

The ULID of a run of an agent, as 26 characters.

Type: string. Pattern: `^[0-9A-HJKMNP-TV-Z]{26}$`.

**Used by:** [`AdapterDisagreement`](/docs/reference/api/types#adapterdisagreement), [`AgentDetected`](/docs/reference/api/types#agentdetected), [`AgentIdentified`](/docs/reference/api/types#agentidentified), [`AgentLinked`](/docs/reference/api/types#agentlinked), [`AgentStateChanged`](/docs/reference/api/types#agentstatechanged), [`AgentValue`](/docs/reference/api/types#agentvalue), [`AttentionValue`](/docs/reference/api/types#attentionvalue), [`ExecutionNodeValue`](/docs/reference/api/types#executionnodevalue), [`GitAgent`](/docs/reference/api/types#gitagent), [`GitWrite`](/docs/reference/api/types#gitwrite), [`InteractionOpened`](/docs/reference/api/types#interactionopened), [`InteractionValue`](/docs/reference/api/types#interactionvalue), [`PaneValue`](/docs/reference/api/types#panevalue), [`PhonePushed`](/docs/reference/api/types#phonepushed), [`PhoneReplied`](/docs/reference/api/types#phonereplied), [`WorkflowNodeEvent`](/docs/reference/api/types#workflownodeevent)

## S {#letter-s}

### `ScrollbackParams` {#scrollbackparams}

| Field | Type | Required | Description |
|---|---|---|---|
| `count` | integer (uint32) | yes |  |
| `from_line` | integer (uint64) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/streams.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/streams.rs#L49) · **Used by:** [stream `scrollback`](/docs/reference/api/streams#scrollback)

### `SelectMode` {#selectmode}

Values: `"simple"`, `"word"`, `"line"`, `"block"`.

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L168) · **Used by:** [`PaneSelect`](/docs/reference/api/types#paneselect)

### `SelectPoint` {#selectpoint}

A cell in the scrollback: the column, and the absolute line.

| Field | Type | Required | Description |
|---|---|---|---|
| `col` | integer (uint16) | yes |  |
| `line` | integer (uint64) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/pane.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/pane.rs#L177) · **Used by:** [`PaneSelect`](/docs/reference/api/types#paneselect)

### `SessionInfo` {#sessioninfo}

`session.info`: the core that answers.

| Field | Type | Required | Description |
|---|---|---|---|
| `api_version` | string | yes | `neohar/1`. |
| `log_epoch` | integer (uint64) | yes | Changes when the event log starts again. A cursor from another epoch is stale. |
| `machine_uuid` | string | yes |  |
| `name` | string | yes | The name of the session (`default`). |
| `pid` | integer (uint32) | yes |  |
| `protocols` | [`Protocols`](/docs/reference/api/types#protocols) | yes |  |
| `session_uuid` | string | yes |  |
| `started_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `version` | string | yes | The version of the neohar build. |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L27) · **Used by:** [`session.info`](/docs/reference/api/methods/session#session-info)

### `SessionStopEverything` {#sessionstopeverything}

`session.stop_everything`: stops every program of every pane, as "Quit and stop everything" does. The core keeps running.

| Field | Type | Required | Description |
|---|---|---|---|
| `force` | boolean |  | Skip the grace period: the agents get no time to end by themselves, and a program that runs after `Term` and `Hup` gets `Kill` at once. Default: `false`. |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L45) · **Used by:** [`session.stop_everything`](/docs/reference/api/methods/session#session-stop-everything)

### `SessionStopped` {#sessionstopped}

What `session.stop_everything` did, as numbers of panes.

| Field | Type | Required | Description |
|---|---|---|---|
| `ended` | integer (uint32) | yes | The panes whose program ended after the polite signals. |
| `killed` | integer (uint32) | yes | The panes whose program had to be killed. |
| `left` | integer (uint32) | yes | The panes whose program still ran at the end. Their holders keep running. |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L54) · **Used by:** [`session.stop_everything`](/docs/reference/api/methods/session#session-stop-everything)

### `SettingEntry` {#settingentry}

| Field | Type | Required | Description |
|---|---|---|---|
| `source` | [`SettingSource`](/docs/reference/api/types#settingsource) | yes |  |
| `value` | any | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L872) · **Used by:** [`SettingsSection`](/docs/reference/api/types#settingssection)

### `SettingSource` {#settingsource}

Where a setting got its value.

Values: `"default"`, `"file"`, `"ui"`.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L866) · **Used by:** [`SettingEntry`](/docs/reference/api/types#settingentry)

### `SettingsSection` {#settingssection}

`settings`: one section of the effective config, with provenance.

| Field | Type | Required | Description |
|---|---|---|---|
| `entries` | map of string to [`SettingEntry`](/docs/reference/api/types#settingentry) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L878) · **Used by:** [projection `settings`](/docs/reference/api/projections#settings), [`ConfigValues`](/docs/reference/api/types#configvalues)

### `Source` {#source}

Where a piece of state came from, and how sure the core is.

| Value | Meaning |
|---|---|
| `"structured"` | A hook or a protocol message of the harness. |
| `"screen"` | The VT screen matched a manifest rule. |
| `"process"` | The process tree. |
| `"holder"` | The holder. |
| `"user"` | The user said so. |
| `"transcript"` | A transcript file. |
| `"derived"` | The core derived it from other state. |

**Rust:** [`crates/nh-proto/src/api/common.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/common.rs#L144) · **Used by:** [`AgentStateChanged`](/docs/reference/api/types#agentstatechanged), [`Provenance`](/docs/reference/api/types#provenance)

### `Spans` {#spans}

Text with highlighted ranges.

| Field | Type | Required | Description |
|---|---|---|---|
| `spans` | any[][] | yes | `[start, end, class]`, in UTF-16 code units. |
| `text` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/rich.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/rich.rs#L8) · **Used by:** [`rich.highlight`](/docs/reference/api/methods/rich#rich-highlight), [`Command`](/docs/reference/api/types#command), [`DiffLine`](/docs/reference/api/types#diffline), [`FsReadResult`](/docs/reference/api/types#fsreadresult)

### `SplitDirection` {#splitdirection}

Which side of a pane a new pane goes on.

Values: `"left"`, `"right"`, `"up"`, `"down"`.

**Rust:** [`crates/nh-proto/src/api/common.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/common.rs#L62) · **Used by:** [`PaneMove`](/docs/reference/api/types#panemove), [`PaneSplit`](/docs/reference/api/types#panesplit), [`Placement`](/docs/reference/api/types#placement)

### `StorageHealth` {#storagehealth}

One of these forms:

#### `state: "ok"`

| Field | Type | Required | Description |
|---|---|---|---|
| `state` | `"ok"` | yes |  |

#### `state: "degraded"`

The store is read-only or full (`degraded_storage`).

| Field | Type | Required | Description |
|---|---|---|---|
| `reason` | string | yes |  |
| `state` | `"degraded"` | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L883) · **Used by:** [`HealthValue`](/docs/reference/api/types#healthvalue)

### `StreamKind` {#streamkind}

| Value | Meaning |
|---|---|
| `"render"` | Core to UI: frames for the visible panes of a window. |
| `"peek"` | Core to UI: downscaled keyframes for thumbnails and card context. |
| `"scrollback"` | Core to UI: styled lines from the archive, for the viewport beyond the in-memory grid. |
| `"input"` | UI to core: batched mouse motion. Keys always use `pane.input`. |

**Rust:** [`crates/nh-proto/src/api/streams.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/streams.rs#L19)

## T {#letter-t}

### `TabClose` {#tabclose}

`confirm_required` when a pane has a live process and `force` is false.

| Field | Type | Required | Description |
|---|---|---|---|
| `force` | boolean |  | Default: `false`. |
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L220) · **Used by:** [`tab.close`](/docs/reference/api/methods/tab#tab-close)

### `TabCreate` {#tabcreate}

| Field | Type | Required | Description |
|---|---|---|---|
| `command` | [`ProcessSpec`](/docs/reference/api/types#processspec) \| `null` |  |  |
| `name` | string \| `null` |  |  |
| `workspace` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L199) · **Used by:** [`tab.create`](/docs/reference/api/methods/tab#tab-create)

### `TabCreated` {#tabcreated}

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `pane_handle` | string | yes |  |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L207) · **Used by:** [`tab.create`](/docs/reference/api/methods/tab#tab-create)

### `TabEvent` {#tabevent}

The payload of the `tab.*` events.

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `name` | string \| `null` |  |  |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L215) · **Used by:** [event `tab.created`](/docs/reference/api/events#tab-created), [event `tab.renamed`](/docs/reference/api/events#tab-renamed), [event `tab.moved`](/docs/reference/api/events#tab-moved), [event `tab.closed`](/docs/reference/api/events#tab-closed), [event `tab.selected`](/docs/reference/api/events#tab-selected)

### `TabId` {#tabid}

The ULID of a tab, as 26 characters.

Type: string. Pattern: `^[0-9A-HJKMNP-TV-Z]{26}$`.

**Used by:** [`LayoutChanged`](/docs/reference/api/types#layoutchanged), [`PaneEvent`](/docs/reference/api/types#paneevent), [`PaneValue`](/docs/reference/api/types#panevalue), [`TabCreated`](/docs/reference/api/types#tabcreated), [`TabEvent`](/docs/reference/api/types#tabevent), [`TabValue`](/docs/reference/api/types#tabvalue), [`WindowValue`](/docs/reference/api/types#windowvalue), [`WorkspaceCreated`](/docs/reference/api/types#workspacecreated), [`WorkspaceValue`](/docs/reference/api/types#workspacevalue)

### `TabMove` {#tabmove}

| Field | Type | Required | Description |
|---|---|---|---|
| `index` | integer (uint32) \| `null` |  | The place among the tabs, counted from 0. Without it the tab goes to the end. |
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) | yes |  |
| `workspace` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) \| `null` |  | The workspace to move it to. Without it the tab stays in its workspace. |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L226) · **Used by:** [`tab.move`](/docs/reference/api/methods/tab#tab-move)

### `TabRename` {#tabrename}

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L214) · **Used by:** [`tab.rename`](/docs/reference/api/methods/tab#tab-rename)

### `TabSelect` {#tabselect}

Shows a tab in a window.

| Field | Type | Required | Description |
|---|---|---|---|
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L237) · **Used by:** [`tab.select`](/docs/reference/api/methods/tab#tab-select)

### `TabValue` {#tabvalue}

| Field | Type | Required | Description |
|---|---|---|---|
| `floating` | [`FloatingPane`](/docs/reference/api/types#floatingpane)[] |  |  |
| `handle` | string | yes | The short handle, for example `w3:t2`. |
| `id` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |
| `layout` | [`LayoutNode`](/docs/reference/api/types#layoutnode) | yes |  |
| `name` | string | yes |  |
| `zoomed` | [`PaneId`](/docs/reference/api/types#paneid) \| `null` |  | The pane that fills the tab, if one is zoomed. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L89) · **Used by:** [`WorkspaceValue`](/docs/reference/api/types#workspacevalue)

### `TailscaleState` {#tailscalestate}

The state of the daemon of Tailscale.

| Value | Meaning |
|---|---|
| `"missing"` | No `tailscale` on the `PATH` of the login environment and none in `/Applications/Tailscale.app`. |
| `"unreachable"` | The CLI gave no answer that the core understands, for example because the daemon does not run. |
| `"starting"` | The daemon starts. |
| `"stopped"` | The daemon is not connected to the tailnet. |
| `"needs_login"` | The daemon needs a login, or the admin of the tailnet must approve the machine. |
| `"running"` | The daemon is connected to the tailnet. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1267) · **Used by:** [`TailscaleStatus`](/docs/reference/api/types#tailscalestatus)

### `TailscaleStatus` {#tailscalestatus}

What `tailscale status --json` and `tailscale serve status --json` say about the listener.

| Field | Type | Required | Description |
|---|---|---|---|
| `funnel` | boolean |  | Whether a Funnel is on `https_port`. neohar never turns one on. Default: `false`. |
| `https` | boolean |  | Whether the tailnet gives an HTTPS certificate for the name (`CertDomains`). Default: `false`. |
| `https_port` | integer (uint16) | yes | The HTTPS port of the mapping: `phone.https_port`. |
| `name` | string \| `null` |  | The name of this machine in the tailnet, without the dot at the end, for example `studio.example.ts.net`. |
| `published` | boolean |  | Whether `tailscale serve` maps `https_port` to the listener. Default: `false`. |
| `state` | [`TailscaleState`](/docs/reference/api/types#tailscalestate) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1286) · **Used by:** [`PhoneStatusValue`](/docs/reference/api/types#phonestatusvalue)

### `TimelineBody` {#timelinebody}

One of these forms:

#### `type: "prompt"`

| Field | Type | Required | Description |
|---|---|---|---|
| `preview` | string | yes |  |
| `type` | `"prompt"` | yes |  |

#### `type: "assistant"`

| Field | Type | Required | Description |
|---|---|---|---|
| `preview` | string | yes |  |
| `type` | `"assistant"` | yes |  |

#### `type: "tool"`

| Field | Type | Required | Description |
|---|---|---|---|
| `duration_ms` | integer (uint64) \| `null` |  |  |
| `status` | [`ToolStatus`](/docs/reference/api/types#toolstatus) | yes |  |
| `summary` | string | yes |  |
| `tool` | string | yes |  |
| `type` | `"tool"` | yes |  |

#### `type: "diff"`

| Field | Type | Required | Description |
|---|---|---|---|
| `diff` | [`Diff`](/docs/reference/api/types#diff) | yes |  |
| `type` | `"diff"` | yes |  |

#### `type: "output"`

| Field | Type | Required | Description |
|---|---|---|---|
| `text` | string | yes |  |
| `truncated` | boolean |  | Default: `false`. |
| `type` | `"output"` | yes |  |

#### `type: "subagent"`

| Field | Type | Required | Description |
|---|---|---|---|
| `agent_id` | string | yes |  |
| `agent_type` | string | yes |  |
| `finished` | boolean | yes |  |
| `type` | `"subagent"` | yes |  |

#### `type: "compaction"`

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `"compaction"` | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L546) · **Used by:** [`TimelineItem`](/docs/reference/api/types#timelineitem)

### `TimelineItem` {#timelineitem}

`timeline:<run>`: a turn or an item of a turn.

| Field | Type | Required | Description |
|---|---|---|---|
| `at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `body` | [`TimelineBody`](/docs/reference/api/types#timelinebody) | yes |  |
| `id` | string | yes |  |
| `turn` | integer (uint32) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L567) · **Used by:** [projection `timeline`](/docs/reference/api/projections#timeline), [`TimelinePageResult`](/docs/reference/api/types#timelinepageresult)

### `TimelineItemGet` {#timelineitemget}

The full text of a `prompt` item or an `assistant` item of `timeline:<run>`. An item of the projection has a preview only.

| Field | Type | Required | Description |
|---|---|---|---|
| `item` | string | yes |  |
| `run` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L449) · **Used by:** [`timeline.item`](/docs/reference/api/methods/timeline#timeline-item)

### `TimelineItemText` {#timelineitemtext}

The text as sanitized HTML. `truncated` is true when the text was longer than 64 KiB.

| Field | Type | Required | Description |
|---|---|---|---|
| `html` | string | yes |  |
| `truncated` | boolean | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L455) · **Used by:** [`timeline.item`](/docs/reference/api/methods/timeline#timeline-item)

### `TimelinePageParams` {#timelinepageparams}

Older items of `timeline:<run>`. They are history, not live state.

| Field | Type | Required | Description |
|---|---|---|---|
| `before_item` | string \| `null` |  |  |
| `limit` | integer (uint32) \| `null` |  |  |
| `run` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L434) · **Used by:** [`timeline.page`](/docs/reference/api/methods/timeline#timeline-page)

### `TimelinePageResult` {#timelinepageresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `items` | [`TimelineItem`](/docs/reference/api/types#timelineitem)[] | yes |  |
| `more` | boolean | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/system.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/system.rs#L442) · **Used by:** [`timeline.page`](/docs/reference/api/methods/timeline#timeline-page)

### `Timestamp` {#timestamp}

RFC 3339 in UTC with milliseconds, for example 2026-10-09T12:34:56.789Z.

Type: string (date-time).

**Rust:** [`crates/nh-proto/src/api/time.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/time.rs#L12) · **Used by:** [`AgentValue`](/docs/reference/api/types#agentvalue), [`AttentionValue`](/docs/reference/api/types#attentionvalue), [`ClipboardDropped`](/docs/reference/api/types#clipboarddropped), [`Event`](/docs/reference/api/types#event), [`Execution`](/docs/reference/api/types#execution), [`ExecutionValue`](/docs/reference/api/types#executionvalue), [`GitBranchValue`](/docs/reference/api/types#gitbranchvalue), [`GitCommit`](/docs/reference/api/types#gitcommit), [`GitHead`](/docs/reference/api/types#githead), [`HarnessValue`](/docs/reference/api/types#harnessvalue), [`HermesChatValue`](/docs/reference/api/types#hermeschatvalue), [`HermesItem`](/docs/reference/api/types#hermesitem), [`HermesItemBody`](/docs/reference/api/types#hermesitembody), [`IntegrationStatusEntry`](/docs/reference/api/types#integrationstatusentry), [`InteractionValue`](/docs/reference/api/types#interactionvalue), [`JobValue`](/docs/reference/api/types#jobvalue), [`LinkValue`](/docs/reference/api/types#linkvalue), [`NoticePayload`](/docs/reference/api/types#noticepayload), [`NotificationValue`](/docs/reference/api/types#notificationvalue), [`PhonePairStarted`](/docs/reference/api/types#phonepairstarted), [`PhonePairingValue`](/docs/reference/api/types#phonepairingvalue), [`PhoneValue`](/docs/reference/api/types#phonevalue), [`PolicyRuleValue`](/docs/reference/api/types#policyrulevalue), [`PolicySuggestion`](/docs/reference/api/types#policysuggestion), …

### `ToolStatus` {#toolstatus}

Values: `"running"`, `"ok"`, `"failed"`, `"denied"`.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L538) · **Used by:** [`TimelineBody`](/docs/reference/api/types#timelinebody)

## U {#letter-u}

### `UnconfirmedReason` {#unconfirmedreason}

Values: `"holder_lost"`, `"server_restarted"`, `"not_written"`.

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L172) · **Used by:** [`PaneInputUnconfirmed`](/docs/reference/api/types#paneinputunconfirmed)

### `UnvalidatedHarness` {#unvalidatedharness}

| Field | Type | Required | Description |
|---|---|---|---|
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `version` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L889) · **Used by:** [`HealthValue`](/docs/reference/api/types#healthvalue)

### `Urgency` {#urgency}

How loud a notification is.

Values: `"low"`, `"normal"`, `"high"`.

**Rust:** [`crates/nh-proto/src/api/common.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/common.rs#L104) · **Used by:** [`AttentionValue`](/docs/reference/api/types#attentionvalue), [`NotificationValue`](/docs/reference/api/types#notificationvalue)

### `Usage` {#usage}

Token use and cost. The cost is the figure of the harness when it reports one.

| Field | Type | Required | Description |
|---|---|---|---|
| `cache_read_tokens` | integer (uint64) | yes |  |
| `cache_write_tokens` | integer (uint64) | yes |  |
| `cost_usd` | number (double) \| `null` |  |  |
| `input_tokens` | integer (uint64) | yes |  |
| `output_tokens` | integer (uint64) | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L293) · **Used by:** [`AgentValue`](/docs/reference/api/types#agentvalue), [`LimitValue`](/docs/reference/api/types#limitvalue)

## V {#letter-v}

### `Verdict` {#verdict}

The last line of a report: `Verdict: pass`, `warn` or `fail`.

One of these forms:

- `"pass"` | `"warn"` | `"fail"`

- `"unknown"` — The report has no verdict line.

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1363) · **Used by:** [`ExecutionNodeValue`](/docs/reference/api/types#executionnodevalue)

### `Via` {#via}

The surface a client answers from. The delivery record keeps it.

One of these forms:

- `"card"` | `"notification"` | `"tray"` | `"palette"` | `"cli"`

- `"policy"` — A rule of the policy decided (step 2). No client answered.

- `"phone"` — The phone app.

**Rust:** [`crates/nh-proto/src/api/common.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/common.rs#L113) · **Used by:** [`DebugAnswer`](/docs/reference/api/types#debuganswer), [`DeliveryInfo`](/docs/reference/api/types#deliveryinfo), [`InteractionAnswer`](/docs/reference/api/types#interactionanswer), [`InteractionDecided`](/docs/reference/api/types#interactiondecided)

## W {#letter-w}

### `WaitFor` {#waitfor}

| Value | Meaning |
|---|---|
| `"idle"` | Execution is `idle`. |
| `"exited"` | The process ended. |
| `"attention"` | The run has an open interaction, or it is in `error` or `rate_limited`. |

**Rust:** [`crates/nh-proto/src/api/calls/agent.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/agent.rs#L154) · **Used by:** [`AgentWait`](/docs/reference/api/types#agentwait)

### `WindowClose` {#windowclose}

| Field | Type | Required | Description |
|---|---|---|---|
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L281) · **Used by:** [`window.close`](/docs/reference/api/methods/window#window-close)

### `WindowEvent` {#windowevent}

The payload of the `window.*` events.

| Field | Type | Required | Description |
|---|---|---|---|
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L237) · **Used by:** [event `window.opened`](/docs/reference/api/events#window-opened), [event `window.closed`](/docs/reference/api/events#window-closed)

### `WindowFocusReport` {#windowfocusreport}

Focus and visibility from the OS and the page, for gate mode and pacing.

| Field | Type | Required | Description |
|---|---|---|---|
| `active_pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) \| `null` |  | The pane whose surface is the active element of the page. `null` when focus is in chrome such as the palette. |
| `focused` | boolean | yes |  |
| `minimized` | boolean |  | Default: `false`. |
| `panes_hidden` | boolean |  | A view of the page covers the panes, for example the Git view. The window gets no frames, and it gets a keyframe of each pane when the view closes. Default: `false`. |
| `refresh_hz` | number (float) \| `null` |  |  |
| `visible` | boolean | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L291) · **Used by:** [`window.focus_report`](/docs/reference/api/methods/window#window-focus-report)

### `WindowId` {#windowid}

The ULID of a window, as 26 characters.

Type: string. Pattern: `^[0-9A-HJKMNP-TV-Z]{26}$`.

**Used by:** [`ChangesDiff`](/docs/reference/api/types#changesdiff), [`ChangesFollow`](/docs/reference/api/types#changesfollow), [`FsOpen`](/docs/reference/api/types#fsopen), [`GitDiff`](/docs/reference/api/types#gitdiff), [`GitFetch`](/docs/reference/api/types#gitfetch), [`GitFollow`](/docs/reference/api/types#gitfollow), [`GitLog`](/docs/reference/api/types#gitlog), [`GitPlace`](/docs/reference/api/types#gitplace), [`GitPull`](/docs/reference/api/types#gitpull), [`GitShow`](/docs/reference/api/types#gitshow), [`GitSwitch`](/docs/reference/api/types#gitswitch), [`GitUnstash`](/docs/reference/api/types#gitunstash), [`InputStreamParams`](/docs/reference/api/types#inputstreamparams), [`InteractionReveal`](/docs/reference/api/types#interactionreveal), [`InteractionRevealed`](/docs/reference/api/types#interactionrevealed), [`PaletteExecute`](/docs/reference/api/types#paletteexecute), [`PaletteQuery`](/docs/reference/api/types#palettequery), [`PaneClaim`](/docs/reference/api/types#paneclaim), [`PaneFocus`](/docs/reference/api/types#panefocus), [`PaneResize`](/docs/reference/api/types#paneresize), [`PaneValue`](/docs/reference/api/types#panevalue), [`PhoneWindowResult`](/docs/reference/api/types#phonewindowresult), [`RenderAck`](/docs/reference/api/types#renderack), [`RenderParams`](/docs/reference/api/types#renderparams), …

### `WindowOpen` {#windowopen}

| Field | Type | Required | Description |
|---|---|---|---|
| `bounds` | [`Bounds`](/docs/reference/api/types#bounds) \| `null` |  |  |
| `panes` | [`RefPaneId`](/docs/reference/api/types#refpaneid)[] |  | The panes to show. Without them the window shows `tab`. Default: `[]`. |
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L267) · **Used by:** [`window.open`](/docs/reference/api/methods/window#window-open)

### `WindowOpened` {#windowopened}

| Field | Type | Required | Description |
|---|---|---|---|
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L277) · **Used by:** [`pane.popout`](/docs/reference/api/methods/pane#pane-popout), [`window.open`](/docs/reference/api/methods/window#window-open)

### `WindowReportMetrics` {#windowreportmetrics}

What the page measured.

| Field | Type | Required | Description |
|---|---|---|---|
| `cell_height_px` | number (float) | yes |  |
| `cell_width_px` | number (float) | yes |  |
| `device_pixel_ratio` | number (float) | yes |  |
| `gpu` | string \| `null` |  |  |
| `renderer` | [`RendererKind`](/docs/reference/api/types#rendererkind) | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L310) · **Used by:** [`window.report_metrics`](/docs/reference/api/methods/window#window-report-metrics)

### `WindowSetBounds` {#windowsetbounds}

| Field | Type | Required | Description |
|---|---|---|---|
| `bounds` | [`Bounds`](/docs/reference/api/types#bounds) | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L320) · **Used by:** [`window.set_bounds`](/docs/reference/api/methods/window#window-set-bounds)

### `WindowSetSingle` {#windowsetsingle}

The single view of a window. With `single`, the window draws only its focused pane, or the first pane of its tab when no pane of the tab has the focus.

| Field | Type | Required | Description |
|---|---|---|---|
| `single` | boolean | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L327) · **Used by:** [`window.set_single`](/docs/reference/api/methods/window#window-set-single)

### `WindowSetVisiblePanes` {#windowsetvisiblepanes}

| Field | Type | Required | Description |
|---|---|---|---|
| `panes` | [`RefPaneId`](/docs/reference/api/types#refpaneid)[] | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L285) · **Used by:** [`window.set_visible_panes`](/docs/reference/api/methods/window#window-set-visible-panes)

### `WindowValue` {#windowvalue}

`windows`: one app window. Restored on launch.

| Field | Type | Required | Description |
|---|---|---|---|
| `bounds` | [`Bounds`](/docs/reference/api/types#bounds) \| `null` |  |  |
| `client` | [`DeviceId`](/docs/reference/api/types#deviceid) \| `null` |  | The device id of the client that owns the window. A client opens a native window only for its own rows. A row with none waits for the first client that attaches. |
| `focused` | boolean | yes | The OS focus of the window, as the host reports it. |
| `focused_pane` | [`PaneId`](/docs/reference/api/types#paneid) \| `null` |  | The pane that has keyboard focus in the window. |
| `id` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |
| `refresh_hz` | number (float) \| `null` |  |  |
| `reveal` | [`Reveal`](/docs/reference/api/types#reveal) \| `null` |  | The interaction that the window must show now (`interaction.reveal`). The window acts when `seq` changes. |
| `single` | boolean |  | The single view of a phone: the window draws only its focused pane, or the first pane of its tab. The core keeps it in memory only. Default: `false`. |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) \| `null` |  | The tab that the window shows. |
| `visible` | boolean | yes |  |
| `visible_panes` | [`PaneId`](/docs/reference/api/types#paneid)[] | yes | The panes that the window draws. The render pacer keys on this. |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) \| `null` |  | The workspace of that tab. The tab bar shows its tabs. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L24) · **Used by:** [projection `windows`](/docs/reference/api/projections#windows)

### `WorkflowContinue` {#workflowcontinue}

`workflow.continue`: starts the agent of a node again with its resume handle, in a new tab that the window shows.

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |
| `node` | string | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  | The window that selects the new tab. |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L268) · **Used by:** [`workflow.continue`](/docs/reference/api/methods/workflow#workflow-continue)

### `WorkflowCopied` {#workflowcopied}

The result of `workflow.copy_to_folder`.

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | The absolute path of the copy. |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L285) · **Used by:** [`workflow.copy_to_folder`](/docs/reference/api/methods/workflow#workflow-copy-to-folder)

### `WorkflowCreate` {#workflowcreate}

`workflow.create`: a new workflow file. The name of the file comes from `name`.

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `preset` | string \| `null` |  | A preset workflow of `workflow.presets`, for example `review`. Without it, the workflow has only the trigger. |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L120) · **Used by:** [`workflow.create`](/docs/reference/api/methods/workflow#workflow-create)

### `WorkflowExecute` {#workflowexecute}

`workflow.execute`: starts an execution.

| Field | Type | Required | Description |
|---|---|---|---|
| `base` | string \| `null` |  | The base of the changes. Without it, the default of `workflow.facts`. |
| `folder` | string | yes | The absolute path of the folder in which the agents work. |
| `node` | string \| `null` |  | Execute only this node, with the reports of the last execution in this folder as inputs. |
| `notes` | string \| `null` |  | What the person built and why. `{{start.notes}}` gives it to a prompt. |
| `workflow` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L191) · **Used by:** [`workflow.execute`](/docs/reference/api/methods/workflow#workflow-execute)

### `WorkflowExecuted` {#workflowexecuted}

The result of `workflow.execute`.

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |
| `label` | string | yes | The name of the execution, for example "Review 3". |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L207) · **Used by:** [`workflow.execute`](/docs/reference/api/methods/workflow#workflow-execute)

### `WorkflowExecutionEvent` {#workflowexecutionevent}

The payload of `workflow.execution_started` and `workflow.execution_ended`.

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |
| `state` | [`ExecutionState`](/docs/reference/api/types#executionstate) | yes |  |
| `workflow` | string | yes | The name of the workflow file without `.toml`. |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L322) · **Used by:** [event `workflow.execution_started`](/docs/reference/api/events#workflow-execution-started), [event `workflow.execution_ended`](/docs/reference/api/events#workflow-execution-ended)

### `WorkflowExecutionList` {#workflowexecutionlist}

The result of `workflow.executions`.

| Field | Type | Required | Description |
|---|---|---|---|
| `executions` | [`ExecutionValue`](/docs/reference/api/types#executionvalue)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L231) · **Used by:** [`workflow.executions`](/docs/reference/api/methods/workflow#workflow-executions)

### `WorkflowExecutionRef` {#workflowexecutionref}

`workflow.execution`, `workflow.cancel`, `workflow.copy_to_folder`: one execution.

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L236) · **Used by:** [`workflow.execution`](/docs/reference/api/methods/workflow#workflow-execution), [`workflow.cancel`](/docs/reference/api/methods/workflow#workflow-cancel), [`workflow.copy_to_folder`](/docs/reference/api/methods/workflow#workflow-copy-to-folder)

### `WorkflowExecutionShown` {#workflowexecutionshown}

The result of `workflow.execution`: the execution and the graph that it runs.

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionValue`](/docs/reference/api/types#executionvalue) | yes |  |
| `graph` | [`WorkflowGraph`](/docs/reference/api/types#workflowgraph) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L241) · **Used by:** [`workflow.execution`](/docs/reference/api/methods/workflow#workflow-execution)

### `WorkflowExecutions` {#workflowexecutions}

`workflow.executions`: the executions, the newest first.

| Field | Type | Required | Description |
|---|---|---|---|
| `folder` | string \| `null` |  | Only the executions in this folder. |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L224) · **Used by:** [`workflow.executions`](/docs/reference/api/methods/workflow#workflow-executions)

### `WorkflowFacts` {#workflowfacts}

The facts of a folder for an execution.

| Field | Type | Required | Description |
|---|---|---|---|
| `base` | string \| `null` |  | The default base: the default branch of `origin` (`origin/HEAD`), else `main` or `master`. |
| `branch` | string \| `null` |  |  |
| `folder` | string | yes |  |
| `repo` | string \| `null` |  | The root of the git worktree that holds the folder. |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L178) · **Used by:** [`workflow.facts`](/docs/reference/api/methods/workflow#workflow-facts)

### `WorkflowFactsGet` {#workflowfactsget}

`workflow.facts`: what the dialog of an execution shows for a folder.

| Field | Type | Required | Description |
|---|---|---|---|
| `folder` | string | yes | An absolute path. |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L172) · **Used by:** [`workflow.facts`](/docs/reference/api/methods/workflow#workflow-facts)

### `WorkflowGraph` {#workflowgraph}

The graph of a workflow.

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | The name, for example "Review". |
| `nodes` | [`WorkflowNode`](/docs/reference/api/types#workflownode)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L75) · **Used by:** [`WorkflowExecutionShown`](/docs/reference/api/types#workflowexecutionshown), [`WorkflowSave`](/docs/reference/api/types#workflowsave), [`WorkflowShown`](/docs/reference/api/types#workflowshown)

### `WorkflowLayout` {#workflowlayout}

`workflow.layout`: gives each node a new position, one column for each depth, and saves.

| Field | Type | Required | Description |
|---|---|---|---|
| `revision` | string | yes |  |
| `workflow` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L143) · **Used by:** [`workflow.layout`](/docs/reference/api/methods/workflow#workflow-layout)

### `WorkflowList` {#workflowlist}

`workflow.list`: the workflows, as the `workflows` projection has them.

| Field | Type | Required | Description |
|---|---|---|---|
| `workflows` | [`WorkflowValue`](/docs/reference/api/types#workflowvalue)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L108) · **Used by:** [`workflow.list`](/docs/reference/api/methods/workflow#workflow-list)

### `WorkflowNode` {#workflownode}

A node of a workflow.

| Field | Type | Required | Description |
|---|---|---|---|
| `can_edit` | boolean |  | Agent: the agent can edit files. Without it, the agent is read-only. Default: `false`. |
| `effort` | string \| `null` |  | Agent: the effort level. Without it, the harness uses its own. |
| `harness` | [`Harness`](/docs/reference/api/types#harness) \| `null` |  | Agent: the harness. |
| `id` | string | yes | The id: `[a-z][a-z0-9_]*`, unique in the workflow. A reference names it, for example `{{code.report}}`. |
| `inputs` | string[] |  | The ids of the nodes that connect to this node. Default: `[]`. |
| `kind` | [`WorkflowNodeKind`](/docs/reference/api/types#workflownodekind) | yes |  |
| `model` | string \| `null` |  | Agent: the model, as the harness names it. Without it, the harness uses its own. |
| `prompt` | string |  | Agent: the prompt, with references such as `{{start.base}}`. Default: `""`. |
| `summary_by` | [`Harness`](/docs/reference/api/types#harness) \| `null` |  | Report: the harness that writes a summary at the top of `report.md`. |
| `summary_prompt` | string \| `null` |  | Report: the first part of the prompt of the summary. Without it, the core uses its own. |
| `time_limit_min` | integer (uint32) \| `null` |  | Agent: the time limit in minutes. Without it, `workflows.time_limit_min`. |
| `title` | string |  | The title on the card. Without it, the card shows the id. Default: `""`. |
| `x` | integer (int32) | yes | The position on the canvas, in canvas units. |
| `y` | integer (int32) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L34) · **Used by:** [`WorkflowGraph`](/docs/reference/api/types#workflowgraph), [`WorkflowNodePreset`](/docs/reference/api/types#workflownodepreset)

### `WorkflowNodeEvent` {#workflownodeevent}

The payload of `workflow.node_changed`.

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |
| `node` | string | yes |  |
| `reason` | string \| `null` |  |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) \| `null` |  | The agent run of the node, while it has one. |
| `state` | [`NodeState`](/docs/reference/api/types#nodestate) | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L336) · **Used by:** [event `workflow.node_changed`](/docs/reference/api/events#workflow-node-changed)

### `WorkflowNodeKind` {#workflownodekind}

The kind of a node.

| Value | Meaning |
|---|---|
| `"manual"` | The start of the workflow. Each workflow has exactly one. |
| `"agent"` | Claude Code or Codex does the prompt of the node and gives a report. |
| `"report"` | Writes `report.md` from the reports of its inputs. |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L24) · **Used by:** [`WorkflowNode`](/docs/reference/api/types#workflownode)

### `WorkflowNodePreset` {#workflownodepreset}

A node of the node picker.

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes |  |
| `label` | string | yes | The text of the picker, for example "Security review". |
| `node` | [`WorkflowNode`](/docs/reference/api/types#workflownode) | yes | The node without `id`, `inputs` and position. |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L149) · **Used by:** [`WorkflowPresets`](/docs/reference/api/types#workflowpresets)

### `WorkflowNodeRef` {#workflownoderef}

`workflow.retry`, `workflow.skip`: one node of an execution.

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |
| `node` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L247) · **Used by:** [`workflow.retry`](/docs/reference/api/methods/workflow#workflow-retry), [`workflow.skip`](/docs/reference/api/methods/workflow#workflow-skip)

### `WorkflowNotes` {#workflownotes}

The result of `workflow.notes_from`.

| Field | Type | Required | Description |
|---|---|---|---|
| `text` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L219) · **Used by:** [`workflow.notes_from`](/docs/reference/api/methods/workflow#workflow-notes-from)

### `WorkflowNotesFrom` {#workflownotesfrom}

`workflow.notes_from`: the prompts of an agent run, as one text for the notes.

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L214) · **Used by:** [`workflow.notes_from`](/docs/reference/api/methods/workflow#workflow-notes-from)

### `WorkflowPreset` {#workflowpreset}

A workflow that `workflow.create` can start from.

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes |  |
| `label` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L158) · **Used by:** [`WorkflowPresets`](/docs/reference/api/types#workflowpresets)

### `WorkflowPresets` {#workflowpresets}

`workflow.presets`: the items of the node picker and the preset workflows.

| Field | Type | Required | Description |
|---|---|---|---|
| `nodes` | [`WorkflowNodePreset`](/docs/reference/api/types#workflownodepreset)[] | yes |  |
| `workflows` | [`WorkflowPreset`](/docs/reference/api/types#workflowpreset)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L164) · **Used by:** [`workflow.presets`](/docs/reference/api/methods/workflow#workflow-presets)

### `WorkflowProblem` {#workflowproblem}

An error in a workflow. A workflow with errors can be saved, but not executed.

| Field | Type | Required | Description |
|---|---|---|---|
| `node` | string \| `null` |  | The node with the error, if the error is in one node. |
| `text` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L82) · **Used by:** [`WorkflowShown`](/docs/reference/api/types#workflowshown)

### `WorkflowRef` {#workflowref}

`workflow.show`, `workflow.delete`, `workflow.duplicate`, `workflow.undo`, `workflow.redo`: one workflow.

| Field | Type | Required | Description |
|---|---|---|---|
| `workflow` | string | yes | The name of the file without `.toml`. |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L114) · **Used by:** [`workflow.show`](/docs/reference/api/methods/workflow#workflow-show), [`workflow.duplicate`](/docs/reference/api/methods/workflow#workflow-duplicate), [`workflow.delete`](/docs/reference/api/methods/workflow#workflow-delete), [`workflow.undo`](/docs/reference/api/methods/workflow#workflow-undo), [`workflow.redo`](/docs/reference/api/methods/workflow#workflow-redo)

### `WorkflowRename` {#workflowrename}

`workflow.rename`: gives a workflow a new name, and its file the name that comes from it.

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `workflow` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L137) · **Used by:** [`workflow.rename`](/docs/reference/api/methods/workflow#workflow-rename)

### `WorkflowReport` {#workflowreport}

The result of `workflow.report`.

| Field | Type | Required | Description |
|---|---|---|---|
| `markdown` | [`Markdown`](/docs/reference/api/types#markdown) | yes |  |
| `path` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L261) · **Used by:** [`workflow.report`](/docs/reference/api/methods/workflow#workflow-report)

### `WorkflowReportGet` {#workflowreportget}

`workflow.report`: a report of an execution.

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |
| `node` | string \| `null` |  | The node. Without it, `report.md`. |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L253) · **Used by:** [`workflow.report`](/docs/reference/api/methods/workflow#workflow-report)

### `WorkflowReveal` {#workflowreveal}

`workflow.reveal`: the tab of the agent of a node shows in the tab bar, and the window selects it.

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |
| `node` | string | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L278) · **Used by:** [`workflow.reveal`](/docs/reference/api/methods/workflow#workflow-reveal)

### `WorkflowSave` {#workflowsave}

`workflow.save`: writes the graph of a workflow.

| Field | Type | Required | Description |
|---|---|---|---|
| `graph` | [`WorkflowGraph`](/docs/reference/api/types#workflowgraph) | yes |  |
| `revision` | string | yes | The revision that the change starts from. An older one answers `conflict`. |
| `workflow` | string | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L129) · **Used by:** [`workflow.save`](/docs/reference/api/methods/workflow#workflow-save)

### `WorkflowShown` {#workflowshown}

A workflow and its errors: the result of `workflow.show` and of each change.

| Field | Type | Required | Description |
|---|---|---|---|
| `can_redo` | boolean |  | `workflow.redo` has a version to go forward to. Default: `false`. |
| `can_undo` | boolean |  | `workflow.undo` has a version to go back to. Default: `false`. |
| `graph` | [`WorkflowGraph`](/docs/reference/api/types#workflowgraph) | yes |  |
| `path` | string | yes | The absolute path of the file. |
| `problems` | [`WorkflowProblem`](/docs/reference/api/types#workflowproblem)[] | yes |  |
| `revision` | string | yes | The revision of the file. `workflow.save` takes it. |
| `workflow` | string | yes | The name of the file without `.toml`. |

**Rust:** [`crates/nh-proto/src/api/calls/workflow.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workflow.rs#L90) · **Used by:** [`workflow.show`](/docs/reference/api/methods/workflow#workflow-show), [`workflow.create`](/docs/reference/api/methods/workflow#workflow-create), [`workflow.save`](/docs/reference/api/methods/workflow#workflow-save), [`workflow.rename`](/docs/reference/api/methods/workflow#workflow-rename), [`workflow.duplicate`](/docs/reference/api/methods/workflow#workflow-duplicate), [`workflow.undo`](/docs/reference/api/methods/workflow#workflow-undo), [`workflow.redo`](/docs/reference/api/methods/workflow#workflow-redo), [`workflow.layout`](/docs/reference/api/methods/workflow#workflow-layout)

### `WorkflowValue` {#workflowvalue}

`workflows`: a workflow file in `<config>/workflows/`. The key is the name of the file without `.toml`. The core orders the rows by name.

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | The name in the file, for example "Review". |
| `nodes` | integer (uint32) | yes | The number of nodes. |
| `problems` | integer (uint32) | yes | The number of errors. A workflow with errors cannot be executed. |
| `revision` | string | yes | The revision of the file, as `workflow.show` gives it. |
| `workflow` | string | yes | The name of the file without `.toml`. Calls name the workflow with it. |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L1311) · **Used by:** [projection `workflows`](/docs/reference/api/projections#workflows), [`WorkflowList`](/docs/reference/api/types#workflowlist)

### `WorkspaceClose` {#workspaceclose}

`confirm_required` when a pane has a live process and `force` is false.

| Field | Type | Required | Description |
|---|---|---|---|
| `force` | boolean |  | Default: `false`. |
| `workspace` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L108) · **Used by:** [`workspace.close`](/docs/reference/api/methods/workspace#workspace-close)

### `WorkspaceCreate` {#workspacecreate}

| Field | Type | Required | Description |
|---|---|---|---|
| `command` | [`ProcessSpec`](/docs/reference/api/types#processspec) \| `null` |  | What the first pane runs. |
| `group` | [`RefGroupId`](/docs/reference/api/types#refgroupid) \| `null` |  |  |
| `machine` | string \| `null` |  | The label of the machine. Without it the workspace is local. |
| `name` | string \| `null` |  |  |
| `root` | string \| `null` |  | The root path. It is the cwd of the first pane. |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L76) · **Used by:** [`workspace.create`](/docs/reference/api/methods/workspace#workspace-create)

### `WorkspaceCreated` {#workspacecreated}

A workspace starts with one tab that has one pane.

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `pane_handle` | string | yes |  |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |
| `tab_handle` | string | yes |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L93) · **Used by:** [`workspace.create`](/docs/reference/api/methods/workspace#workspace-create)

### `WorkspaceEvent` {#workspaceevent}

The payload of the `workspace.*` events.

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `name` | string \| `null` |  |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/events.rs#L200) · **Used by:** [event `workspace.created`](/docs/reference/api/events#workspace-created), [event `workspace.renamed`](/docs/reference/api/events#workspace-renamed), [event `workspace.moved`](/docs/reference/api/events#workspace-moved), [event `workspace.closed`](/docs/reference/api/events#workspace-closed)

### `WorkspaceId` {#workspaceid}

The ULID of a workspace, as 26 characters.

Type: string. Pattern: `^[0-9A-HJKMNP-TV-Z]{26}$`.

**Used by:** [`GroupSummary`](/docs/reference/api/types#groupsummary), [`PaneEvent`](/docs/reference/api/types#paneevent), [`PaneValue`](/docs/reference/api/types#panevalue), [`TabEvent`](/docs/reference/api/types#tabevent), [`WindowValue`](/docs/reference/api/types#windowvalue), [`WorkspaceCreated`](/docs/reference/api/types#workspacecreated), [`WorkspaceEvent`](/docs/reference/api/types#workspaceevent), [`WorkspaceSummary`](/docs/reference/api/types#workspacesummary), [`WorkspaceValue`](/docs/reference/api/types#workspacevalue)

### `WorkspaceList` {#workspacelist}

| Field | Type | Required | Description |
|---|---|---|---|
| `group` | [`RefGroupId`](/docs/reference/api/types#refgroupid) \| `null` |  |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L125) · **Used by:** [`workspace.list`](/docs/reference/api/methods/workspace#workspace-list)

### `WorkspaceListResult` {#workspacelistresult}

| Field | Type | Required | Description |
|---|---|---|---|
| `workspaces` | [`WorkspaceSummary`](/docs/reference/api/types#workspacesummary)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L144) · **Used by:** [`workspace.list`](/docs/reference/api/methods/workspace#workspace-list)

### `WorkspaceMove` {#workspacemove}

Moves a workspace in the sidebar.

| Field | Type | Required | Description |
|---|---|---|---|
| `before` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) \| `null` |  | Put it before this workspace. Without it the workspace goes to the end. |
| `group` | [`RefGroupId`](/docs/reference/api/types#refgroupid) \| `null` |  | The group to put it in. Without it the workspace has no group. |
| `workspace` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L115) · **Used by:** [`workspace.move`](/docs/reference/api/methods/workspace#workspace-move)

### `WorkspaceRename` {#workspacerename}

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `workspace` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L102) · **Used by:** [`workspace.rename`](/docs/reference/api/methods/workspace#workspace-rename)

### `WorkspaceShow` {#workspaceshow}

| Field | Type | Required | Description |
|---|---|---|---|
| `workspace` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L148) · **Used by:** [`workspace.show`](/docs/reference/api/methods/workspace#workspace-show)

### `WorkspaceShown` {#workspaceshown}

| Field | Type | Required | Description |
|---|---|---|---|
| `workspace` | [`WorkspaceValue`](/docs/reference/api/types#workspacevalue) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L152) · **Used by:** [`workspace.show`](/docs/reference/api/methods/workspace#workspace-show)

### `WorkspaceSummary` {#workspacesummary}

| Field | Type | Required | Description |
|---|---|---|---|
| `agents` | integer (uint32) | yes | The panes of the workspace that have a live agent run. |
| `group` | [`GroupId`](/docs/reference/api/types#groupid) \| `null` |  |  |
| `handle` | string | yes |  |
| `id` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |
| `name` | string | yes |  |
| `panes` | integer (uint32) | yes |  |
| `root` | string \| `null` |  |  |
| `tabs` | integer (uint32) | yes |  |

**Rust:** [`crates/nh-proto/src/api/calls/workspace.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/calls/workspace.rs#L130) · **Used by:** [`WorkspaceListResult`](/docs/reference/api/types#workspacelistresult)

### `WorkspaceValue` {#workspacevalue}

`tree`: a workspace with its tabs.

| Field | Type | Required | Description |
|---|---|---|---|
| `group` | [`GroupRef`](/docs/reference/api/types#groupref) \| `null` |  |  |
| `handle` | string | yes | The short handle, for example `w3`. |
| `id` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |
| `machine` | string | yes | The label of the machine. The local machine is `local`. |
| `name` | string | yes |  |
| `root` | string \| `null` |  |  |
| `selected_tab` | [`TabId`](/docs/reference/api/types#tabid) \| `null` |  |  |
| `tabs` | [`TabValue`](/docs/reference/api/types#tabvalue)[] | yes |  |

**Rust:** [`crates/nh-proto/src/api/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/model.rs#L73) · **Used by:** [projection `tree`](/docs/reference/api/projections#tree), [`WorkspaceShown`](/docs/reference/api/types#workspaceshown)
