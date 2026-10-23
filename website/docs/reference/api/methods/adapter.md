---
title: "adapter.*"
description: "The calls of the hook shim. Only a caller with the token of a pane may make them."
sidebar_position: 30
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The calls of the hook shim. Only a caller with the token of a pane may make them.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`adapter.signal`](#adapter-signal) | `pane` | [`AdapterSignal`](/docs/reference/api/types#adaptersignal) | [`AdapterReceipt`](/docs/reference/api/types#adapterreceipt) |
| [`adapter.gate`](#adapter-gate) | `pane` | [`AdapterGate`](/docs/reference/api/types#adaptergate) | [`GateReply`](/docs/reference/api/types#gatereply) |
| [`adapter.gate_resume`](#adapter-gate-resume) | `pane` | [`AdapterGateResume`](/docs/reference/api/types#adaptergateresume) | [`GateReply`](/docs/reference/api/types#gatereply) |
| [`adapter.delivery_ack`](#adapter-delivery-ack) | `pane` | [`AdapterDeliveryAck`](/docs/reference/api/types#adapterdeliveryack) | [`Empty`](/docs/reference/api/types#empty) |

## `adapter.signal` {#adapter-signal}

**Scopes:** `pane` — a hook shim, with the token of a pane.

A hook of a harness, as the shim sends it. The payload is the raw JSON that the harness wrote to the stdin of the hook.

### Params

[`AdapterSignal`](/docs/reference/api/types#adaptersignal)

| Field | Type | Required | Description |
|---|---|---|---|
| `event` | string | yes | The name of the hook event, for example `PreToolUse`. |
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `payload` | any | yes |  |
| `token` | string | yes | The `NEOHAR_PANE_TOKEN` of the pane. |
| `truncated` | boolean |  | The shim cut the payload at its cap of 4 MiB. Default: `false`. |

### Result

[`AdapterReceipt`](/docs/reference/api/types#adapterreceipt)

| Field | Type | Required | Description |
|---|---|---|---|
| `ok` | boolean | yes |  |

## `adapter.gate` {#adapter-gate}

**Scopes:** `pane` — a hook shim, with the token of a pane.

The params of `adapter.gate` are those of a signal. The shim holds the hook until the core answers.

### Params

[`AdapterGate`](/docs/reference/api/types#adaptergate)

| Field | Type | Required | Description |
|---|---|---|---|
| `event` | string | yes |  |
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `payload` | any | yes |  |
| `token` | string | yes |  |
| `truncated` | boolean |  | Default: `false`. |

### Result

[`GateReply`](/docs/reference/api/types#gatereply)

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

## `adapter.gate_resume` {#adapter-gate-resume}

**Scopes:** `pane` — a hook shim, with the token of a pane.

A shim whose connection dropped asks for its interaction again, once.

### Params

[`AdapterGateResume`](/docs/reference/api/types#adaptergateresume)

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |

### Result

[`GateReply`](/docs/reference/api/types#gatereply)

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

## `adapter.delivery_ack` {#adapter-delivery-ack}

**Scopes:** `pane` — a hook shim, with the token of a pane.

The shim applied the decision. It sends this just before it prints.

### Params

[`AdapterDeliveryAck`](/docs/reference/api/types#adapterdeliveryack)

| Field | Type | Required | Description |
|---|---|---|---|
| `applied` | boolean | yes |  |
| `idempotency_key` | string | yes |  |
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.
