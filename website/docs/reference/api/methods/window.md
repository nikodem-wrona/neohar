---
title: "window.*"
description: "The windows of the app: which panes they show, focus, metrics, bounds and the single view."
sidebar_position: 8
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The windows of the app: which panes they show, focus, metrics, bounds and the single view.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`window.open`](#window-open) | `ui` | [`WindowOpen`](/docs/reference/api/types#windowopen) | [`WindowOpened`](/docs/reference/api/types#windowopened) |
| [`window.close`](#window-close) | `ui` | [`WindowClose`](/docs/reference/api/types#windowclose) | [`Empty`](/docs/reference/api/types#empty) |
| [`window.set_visible_panes`](#window-set-visible-panes) | `ui` `phone` | [`WindowSetVisiblePanes`](/docs/reference/api/types#windowsetvisiblepanes) | [`Empty`](/docs/reference/api/types#empty) |
| [`window.focus_report`](#window-focus-report) | `ui` `phone` | [`WindowFocusReport`](/docs/reference/api/types#windowfocusreport) | [`Empty`](/docs/reference/api/types#empty) |
| [`window.report_metrics`](#window-report-metrics) | `ui` `phone` | [`WindowReportMetrics`](/docs/reference/api/types#windowreportmetrics) | [`Empty`](/docs/reference/api/types#empty) |
| [`window.set_bounds`](#window-set-bounds) | `ui` `phone` | [`WindowSetBounds`](/docs/reference/api/types#windowsetbounds) | [`Empty`](/docs/reference/api/types#empty) |
| [`window.set_single`](#window-set-single) | `ui` `phone` | [`WindowSetSingle`](/docs/reference/api/types#windowsetsingle) | [`Empty`](/docs/reference/api/types#empty) |

## `window.open` {#window-open}

**Scopes:** `ui` — the UI, over the `rpc` command of the Tauri host or the WebSocket.

### Params

[`WindowOpen`](/docs/reference/api/types#windowopen)

| Field | Type | Required | Description |
|---|---|---|---|
| `bounds` | [`Bounds`](/docs/reference/api/types#bounds) \| `null` |  |  |
| `panes` | [`RefPaneId`](/docs/reference/api/types#refpaneid)[] |  | The panes to show. Without them the window shows `tab`. Default: `[]`. |
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) \| `null` |  |  |

### Result

[`WindowOpened`](/docs/reference/api/types#windowopened)

| Field | Type | Required | Description |
|---|---|---|---|
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

## `window.close` {#window-close}

**Scopes:** `ui` — the UI, over the `rpc` command of the Tauri host or the WebSocket.

### Params

[`WindowClose`](/docs/reference/api/types#windowclose)

| Field | Type | Required | Description |
|---|---|---|---|
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `window.set_visible_panes` {#window-set-visible-panes}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

### Params

[`WindowSetVisiblePanes`](/docs/reference/api/types#windowsetvisiblepanes)

| Field | Type | Required | Description |
|---|---|---|---|
| `panes` | [`RefPaneId`](/docs/reference/api/types#refpaneid)[] | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `window.focus_report` {#window-focus-report}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

Focus and visibility from the OS and the page, for gate mode and pacing.

### Params

[`WindowFocusReport`](/docs/reference/api/types#windowfocusreport)

| Field | Type | Required | Description |
|---|---|---|---|
| `active_pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) \| `null` |  | The pane whose surface is the active element of the page. `null` when focus is in chrome such as the palette. |
| `focused` | boolean | yes |  |
| `minimized` | boolean |  | Default: `false`. |
| `panes_hidden` | boolean |  | A view of the page covers the panes, for example the Git view. The window gets no frames, and it gets a keyframe of each pane when the view closes. Default: `false`. |
| `refresh_hz` | number (float) \| `null` |  |  |
| `visible` | boolean | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `window.report_metrics` {#window-report-metrics}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

What the page measured.

### Params

[`WindowReportMetrics`](/docs/reference/api/types#windowreportmetrics)

| Field | Type | Required | Description |
|---|---|---|---|
| `cell_height_px` | number (float) | yes |  |
| `cell_width_px` | number (float) | yes |  |
| `device_pixel_ratio` | number (float) | yes |  |
| `gpu` | string \| `null` |  |  |
| `renderer` | [`RendererKind`](/docs/reference/api/types#rendererkind) | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `window.set_bounds` {#window-set-bounds}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

### Params

[`WindowSetBounds`](/docs/reference/api/types#windowsetbounds)

| Field | Type | Required | Description |
|---|---|---|---|
| `bounds` | [`Bounds`](/docs/reference/api/types#bounds) | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `window.set_single` {#window-set-single}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

The single view of a window. With `single`, the window draws only its focused pane, or the first pane of its tab when no pane of the tab has the focus.

### Params

[`WindowSetSingle`](/docs/reference/api/types#windowsetsingle)

| Field | Type | Required | Description |
|---|---|---|---|
| `single` | boolean | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.
