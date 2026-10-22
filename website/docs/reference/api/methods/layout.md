---
title: "layout.*"
description: "The split tree of a tab: ratios, equal sizes and zoom."
sidebar_position: 6
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The split tree of a tab: ratios, equal sizes and zoom.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`layout.set_ratios`](#layout-set-ratios) | `ui` `cli` `phone` | [`LayoutSetRatios`](/docs/reference/api/types#layoutsetratios) | [`Empty`](/docs/reference/api/types#empty) |
| [`layout.equalize`](#layout-equalize) | `ui` `cli` `phone` | [`LayoutEqualize`](/docs/reference/api/types#layoutequalize) | [`Empty`](/docs/reference/api/types#empty) |
| [`layout.zoom`](#layout-zoom) | `ui` `cli` `phone` | [`LayoutZoom`](/docs/reference/api/types#layoutzoom) | [`Empty`](/docs/reference/api/types#empty) |

## `layout.set_ratios` {#layout-set-ratios}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh layout set-ratios`](/docs/reference/cli/commands/layout#layout-set-ratios)

### Params

[`LayoutSetRatios`](/docs/reference/api/types#layoutsetratios)

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | integer (uint32)[] | yes | The split node, as the indices of the children from the root. `[]` is the root. |
| `ratios` | number (float)[] | yes |  |
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `layout.equalize` {#layout-equalize}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh layout equalize`](/docs/reference/cli/commands/layout#layout-equalize)

### Params

[`LayoutEqualize`](/docs/reference/api/types#layoutequalize)

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | integer (uint32)[] |  | Default: `[]`. |
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `layout.zoom` {#layout-zoom}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh layout zoom`](/docs/reference/cli/commands/layout#layout-zoom)

### Params

[`LayoutZoom`](/docs/reference/api/types#layoutzoom)

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) \| `null` |  | The pane to zoom. Without it the zoom ends. |
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.
