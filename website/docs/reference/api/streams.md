---
title: "Streams"
description: "The binary streams of the contract: render, peek, scrollback and input."
sidebar_position: 5
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

A stream carries binary data next to the JSON-RPC calls: `postcard` frames of packed cells from the core to the UI, and input from the UI to the core. The frame format has version 1 and the holder protocol has version 2. See [Rendering and input](/docs/architecture/rendering-and-input) and [The desktop app](/docs/desktop/overview).

| Stream | Direction | Params |
|---|---|---|
| [`render`](#render) | core to ui | [`RenderParams`](/docs/reference/api/types#renderparams) |
| [`peek`](#peek) | core to ui | [`PeekParams`](/docs/reference/api/types#peekparams) |
| [`scrollback`](#scrollback) | core to ui | [`ScrollbackParams`](/docs/reference/api/types#scrollbackparams) |
| [`input`](#input) | ui to core | [`InputStreamParams`](/docs/reference/api/types#inputstreamparams) |

## `render` {#render}

**Direction:** core to ui

### Params

[`RenderParams`](/docs/reference/api/types#renderparams)

| Field | Type | Required | Description |
|---|---|---|---|
| `proto` | integer (uint16) | yes | The version of the frame format that the client decodes (`FRAME_PROTO`). |
| `viewport_hz` | number (float) | yes | The refresh rate of the window. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

## `peek` {#peek}

**Direction:** core to ui

### Params

[`PeekParams`](/docs/reference/api/types#peekparams)

| Field | Type | Required | Description |
|---|---|---|---|
| `max_cols` | integer (uint16) | yes |  |
| `max_hz` | number (float) |  | Default: `4`. |
| `max_rows` | integer (uint16) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

## `scrollback` {#scrollback}

**Direction:** core to ui

### Params

[`ScrollbackParams`](/docs/reference/api/types#scrollbackparams)

| Field | Type | Required | Description |
|---|---|---|---|
| `count` | integer (uint32) | yes |  |
| `from_line` | integer (uint64) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

## `input` {#input}

**Direction:** ui to core

### Params

[`InputStreamParams`](/docs/reference/api/types#inputstreamparams)

| Field | Type | Required | Description |
|---|---|---|---|
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

## Notifications of the server {#notifications}

The server also sends these JSON-RPC notifications (messages without an `id`):

| Notification | Params |
|---|---|
| `events.event` | [`EventNotification`](/docs/reference/api/types#eventnotification) |
| `events.truncated` | [`EventsTruncated`](/docs/reference/api/types#eventstruncated) |
| `projection.message` | [`ProjectionNotification`](/docs/reference/api/types#projectionnotification) |
| `adapter.gate_result` | [`GateResult`](/docs/reference/api/types#gateresult) |

## Notification `adapter.gate_result` {#notification-adapter-gate-result}

[`GateResult`](/docs/reference/api/types#gateresult)

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
