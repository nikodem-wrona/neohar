---
title: "session.*"
description: "The session of the core: its name, version and sockets."
sidebar_position: 1
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The session of the core: its name, version and sockets.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`session.info`](#session-info) | `ui` `cli` `phone` | [`Empty`](/docs/reference/api/types#empty) | [`SessionInfo`](/docs/reference/api/types#sessioninfo) |
| [`session.stop_everything`](#session-stop-everything) | `ui` `cli` `phone` | [`SessionStopEverything`](/docs/reference/api/types#sessionstopeverything) | [`SessionStopped`](/docs/reference/api/types#sessionstopped) |
| [`session.shutdown`](#session-shutdown) | `cli` | [`Empty`](/docs/reference/api/types#empty) | [`Empty`](/docs/reference/api/types#empty) |

## `session.info` {#session-info}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh session info`](/docs/reference/cli/commands/session#session-info)

An empty result or empty params: `{}`.

### Params

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

### Result

[`SessionInfo`](/docs/reference/api/types#sessioninfo)

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

## `session.stop_everything` {#session-stop-everything}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh session stop-everything`](/docs/reference/cli/commands/session#session-stop-everything)

`session.stop_everything`: stops every program of every pane, as "Quit and stop everything" does. The core keeps running.

### Params

[`SessionStopEverything`](/docs/reference/api/types#sessionstopeverything)

| Field | Type | Required | Description |
|---|---|---|---|
| `force` | boolean |  | Skip the grace period: the agents get no time to end by themselves, and a program that runs after `Term` and `Hup` gets `Kill` at once. Default: `false`. |

### Result

[`SessionStopped`](/docs/reference/api/types#sessionstopped)

| Field | Type | Required | Description |
|---|---|---|---|
| `ended` | integer (uint32) | yes | The panes whose program ended after the polite signals. |
| `killed` | integer (uint32) | yes | The panes whose program had to be killed. |
| `left` | integer (uint32) | yes | The panes whose program still ran at the end. Their holders keep running. |

## `session.shutdown` {#session-shutdown}

**Scopes:** `cli` — the CLI and any other client on the API socket.

**Command line:** [`nh session shutdown`](/docs/reference/cli/commands/session#session-shutdown)

An empty result or empty params: `{}`.

### Params

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.
