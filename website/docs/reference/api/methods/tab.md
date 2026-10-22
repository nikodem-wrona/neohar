---
title: "tab.*"
description: "Tabs: one layout of panes in a workspace."
sidebar_position: 5
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

Tabs: one layout of panes in a workspace.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`tab.create`](#tab-create) | `ui` `cli` `phone` | [`TabCreate`](/docs/reference/api/types#tabcreate) | [`TabCreated`](/docs/reference/api/types#tabcreated) |
| [`tab.rename`](#tab-rename) | `ui` `cli` `phone` | [`TabRename`](/docs/reference/api/types#tabrename) | [`Empty`](/docs/reference/api/types#empty) |
| [`tab.close`](#tab-close) | `ui` `cli` `phone` | [`TabClose`](/docs/reference/api/types#tabclose) | [`Empty`](/docs/reference/api/types#empty) |
| [`tab.move`](#tab-move) | `ui` `cli` `phone` | [`TabMove`](/docs/reference/api/types#tabmove) | [`Empty`](/docs/reference/api/types#empty) |
| [`tab.select`](#tab-select) | `ui` `cli` `phone` | [`TabSelect`](/docs/reference/api/types#tabselect) | [`Empty`](/docs/reference/api/types#empty) |

## `tab.create` {#tab-create}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh tab create`](/docs/reference/cli/commands/tab#tab-create)

### Params

[`TabCreate`](/docs/reference/api/types#tabcreate)

| Field | Type | Required | Description |
|---|---|---|---|
| `command` | [`ProcessSpec`](/docs/reference/api/types#processspec) \| `null` |  |  |
| `name` | string \| `null` |  |  |
| `workspace` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) | yes |  |

### Result

[`TabCreated`](/docs/reference/api/types#tabcreated)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `pane_handle` | string | yes |  |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |

## `tab.rename` {#tab-rename}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh tab rename`](/docs/reference/cli/commands/tab#tab-rename)

### Params

[`TabRename`](/docs/reference/api/types#tabrename)

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `tab.close` {#tab-close}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh tab close`](/docs/reference/cli/commands/tab#tab-close)

`confirm_required` when a pane has a live process and `force` is false.

### Params

[`TabClose`](/docs/reference/api/types#tabclose)

| Field | Type | Required | Description |
|---|---|---|---|
| `force` | boolean |  | Default: `false`. |
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `tab.move` {#tab-move}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh tab move`](/docs/reference/cli/commands/tab#tab-move)

### Params

[`TabMove`](/docs/reference/api/types#tabmove)

| Field | Type | Required | Description |
|---|---|---|---|
| `index` | integer (uint32) \| `null` |  | The place among the tabs, counted from 0. Without it the tab goes to the end. |
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) | yes |  |
| `workspace` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) \| `null` |  | The workspace to move it to. Without it the tab stays in its workspace. |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `tab.select` {#tab-select}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh tab select`](/docs/reference/cli/commands/tab#tab-select)

Shows a tab in a window.

### Params

[`TabSelect`](/docs/reference/api/types#tabselect)

| Field | Type | Required | Description |
|---|---|---|---|
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.
