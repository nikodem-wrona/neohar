---
title: "render.*"
description: "Acknowledgements of painted frames, which free the slots of the render pacer."
sidebar_position: 11
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

Acknowledgements of painted frames, which free the slots of the render pacer.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`render.ack`](#render-ack) | `ui` `phone` | [`RenderAck`](/docs/reference/api/types#renderack) | [`Empty`](/docs/reference/api/types#empty) |

## `render.ack` {#render-ack}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`render.ack`. The UI sends it at most once for each painted animation frame, and only when something changed.

### Params

[`RenderAck`](/docs/reference/api/types#renderack)

| Field | Type | Required | Description |
|---|---|---|---|
| `acks` | [`PaneAck`](/docs/reference/api/types#paneack)[] |  | Default: `[]`. |
| `need_keyframe` | integer (uint32)[] |  | The panes whose frames the UI discarded. The core sends a keyframe for each. Default: `[]`. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.
