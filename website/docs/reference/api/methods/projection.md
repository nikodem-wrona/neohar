---
title: "projection.*"
description: "Subscriptions to projections: a snapshot, then keyed patches."
sidebar_position: 18
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

Subscriptions to projections: a snapshot, then keyed patches.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`projection.subscribe`](#projection-subscribe) | `ui` `cli` `phone` | [`ProjectionSubscribe`](/docs/reference/api/types#projectionsubscribe) | [`ProjectionSubscribed`](/docs/reference/api/types#projectionsubscribed) |
| [`projection.unsubscribe`](#projection-unsubscribe) | `ui` `cli` `phone` | [`ProjectionUnsubscribe`](/docs/reference/api/types#projectionunsubscribe) | [`Empty`](/docs/reference/api/types#empty) |
| [`projection.versions`](#projection-versions) | `ui` `cli` `phone` | [`ProjectionVersions`](/docs/reference/api/types#projectionversions) | [`ProjectionVersionsResult`](/docs/reference/api/types#projectionversionsresult) |

## `projection.subscribe` {#projection-subscribe}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

`projection.subscribe`. Messages arrive as `projection.message` notifications.

### Params

[`ProjectionSubscribe`](/docs/reference/api/types#projectionsubscribe)

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | [`Projection`](/docs/reference/api/types#projection) | yes |  |
| `since` | integer (uint64) \| `null` |  | The version the client has. The core sends only patches if it still retains them. |

### Result

[`ProjectionSubscribed`](/docs/reference/api/types#projectionsubscribed)

| Field | Type | Required | Description |
|---|---|---|---|
| `subscription` | string | yes |  |

## `projection.unsubscribe` {#projection-unsubscribe}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

### Params

[`ProjectionUnsubscribe`](/docs/reference/api/types#projectionunsubscribe)

| Field | Type | Required | Description |
|---|---|---|---|
| `subscription` | string | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `projection.versions` {#projection-versions}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

`projection.versions {names}`: the version of each projection named. A client compares them with the versions of its subscriptions, because a subscription can stop without a gap in its versions: a channel of the host can lose one message and hold the ones after it, and the core drops a sink whose send fails.

### Params

[`ProjectionVersions`](/docs/reference/api/types#projectionversions)

| Field | Type | Required | Description |
|---|---|---|---|
| `names` | [`Projection`](/docs/reference/api/types#projection)[] | yes |  |

### Result

[`ProjectionVersionsResult`](/docs/reference/api/types#projectionversionsresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `versions` | map of string to integer (uint64) | yes | The version by name. A projection that the core has not made is left out. |
