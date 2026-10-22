---
title: "events.*"
description: "Subscriptions to the event log, from a cursor."
sidebar_position: 20
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

Subscriptions to the event log, from a cursor.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`events.subscribe`](#events-subscribe) | `cli` | [`EventsSubscribe`](/docs/reference/api/types#eventssubscribe) | [`EventsSubscribed`](/docs/reference/api/types#eventssubscribed) |
| [`events.unsubscribe`](#events-unsubscribe) | `cli` | [`EventsUnsubscribe`](/docs/reference/api/types#eventsunsubscribe) | [`Empty`](/docs/reference/api/types#empty) |

## `events.subscribe` {#events-subscribe}

**Scopes:** `cli` — the CLI and any other client on the API socket.

`events.subscribe {cursor, filter}`. Events arrive as `events.event` notifications. A client whose cursor is older than the retention, or from another `log_epoch`, gets `events.truncated` and takes a fresh snapshot.

### Params

[`EventsSubscribe`](/docs/reference/api/types#eventssubscribe)

| Field | Type | Required | Description |
|---|---|---|---|
| `cursor` | [`Cursor`](/docs/reference/api/types#cursor) \| `null` |  | Where to start. Without it the subscription starts at the end of the log. |
| `filter` | [`EventFilter`](/docs/reference/api/types#eventfilter) |  | Default: `{}`. |

### Result

[`EventsSubscribed`](/docs/reference/api/types#eventssubscribed)

| Field | Type | Required | Description |
|---|---|---|---|
| `cursor` | [`Cursor`](/docs/reference/api/types#cursor) | yes | The cursor of the end of the log at the moment of the call. |
| `subscription` | string | yes | The id of the subscription, as it is in the notifications. |

## `events.unsubscribe` {#events-unsubscribe}

**Scopes:** `cli` — the CLI and any other client on the API socket.

### Params

[`EventsUnsubscribe`](/docs/reference/api/types#eventsunsubscribe)

| Field | Type | Required | Description |
|---|---|---|---|
| `subscription` | string | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.
