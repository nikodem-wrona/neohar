---
title: "workspace.*"
description: "Workspaces: groups of tabs, usually bound to a project root."
sidebar_position: 3
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

Workspaces: groups of tabs, usually bound to a project root.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`workspace.create`](#workspace-create) | `ui` `cli` `phone` | [`WorkspaceCreate`](/docs/reference/api/types#workspacecreate) | [`WorkspaceCreated`](/docs/reference/api/types#workspacecreated) |
| [`workspace.rename`](#workspace-rename) | `ui` `cli` `phone` | [`WorkspaceRename`](/docs/reference/api/types#workspacerename) | [`Empty`](/docs/reference/api/types#empty) |
| [`workspace.close`](#workspace-close) | `ui` `cli` `phone` | [`WorkspaceClose`](/docs/reference/api/types#workspaceclose) | [`Empty`](/docs/reference/api/types#empty) |
| [`workspace.move`](#workspace-move) | `ui` `cli` `phone` | [`WorkspaceMove`](/docs/reference/api/types#workspacemove) | [`Empty`](/docs/reference/api/types#empty) |
| [`workspace.list`](#workspace-list) | `ui` `cli` `phone` | [`WorkspaceList`](/docs/reference/api/types#workspacelist) | [`WorkspaceListResult`](/docs/reference/api/types#workspacelistresult) |
| [`workspace.show`](#workspace-show) | `ui` `cli` `phone` | [`WorkspaceShow`](/docs/reference/api/types#workspaceshow) | [`WorkspaceShown`](/docs/reference/api/types#workspaceshown) |

## `workspace.create` {#workspace-create}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh workspace create`](/docs/reference/cli/commands/workspace#workspace-create)

### Params

[`WorkspaceCreate`](/docs/reference/api/types#workspacecreate)

| Field | Type | Required | Description |
|---|---|---|---|
| `command` | [`ProcessSpec`](/docs/reference/api/types#processspec) \| `null` |  | What the first pane runs. |
| `group` | [`RefGroupId`](/docs/reference/api/types#refgroupid) \| `null` |  |  |
| `machine` | string \| `null` |  | The label of the machine. Without it the workspace is local. |
| `name` | string \| `null` |  |  |
| `root` | string \| `null` |  | The root path. It is the cwd of the first pane. |

### Result

[`WorkspaceCreated`](/docs/reference/api/types#workspacecreated)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `pane_handle` | string | yes |  |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |
| `tab_handle` | string | yes |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

## `workspace.rename` {#workspace-rename}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh workspace rename`](/docs/reference/cli/commands/workspace#workspace-rename)

### Params

[`WorkspaceRename`](/docs/reference/api/types#workspacerename)

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `workspace` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `workspace.close` {#workspace-close}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh workspace close`](/docs/reference/cli/commands/workspace#workspace-close)

`confirm_required` when a pane has a live process and `force` is false.

### Params

[`WorkspaceClose`](/docs/reference/api/types#workspaceclose)

| Field | Type | Required | Description |
|---|---|---|---|
| `force` | boolean |  | Default: `false`. |
| `workspace` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `workspace.move` {#workspace-move}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh workspace move`](/docs/reference/cli/commands/workspace#workspace-move)

Moves a workspace in the sidebar.

### Params

[`WorkspaceMove`](/docs/reference/api/types#workspacemove)

| Field | Type | Required | Description |
|---|---|---|---|
| `before` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) \| `null` |  | Put it before this workspace. Without it the workspace goes to the end. |
| `group` | [`RefGroupId`](/docs/reference/api/types#refgroupid) \| `null` |  | The group to put it in. Without it the workspace has no group. |
| `workspace` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `workspace.list` {#workspace-list}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh workspace list`](/docs/reference/cli/commands/workspace#workspace-list)

### Params

[`WorkspaceList`](/docs/reference/api/types#workspacelist)

| Field | Type | Required | Description |
|---|---|---|---|
| `group` | [`RefGroupId`](/docs/reference/api/types#refgroupid) \| `null` |  |  |

### Result

[`WorkspaceListResult`](/docs/reference/api/types#workspacelistresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `workspaces` | [`WorkspaceSummary`](/docs/reference/api/types#workspacesummary)[] | yes |  |

## `workspace.show` {#workspace-show}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh workspace show`](/docs/reference/cli/commands/workspace#workspace-show)

### Params

[`WorkspaceShow`](/docs/reference/api/types#workspaceshow)

| Field | Type | Required | Description |
|---|---|---|---|
| `workspace` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) | yes |  |

### Result

[`WorkspaceShown`](/docs/reference/api/types#workspaceshown)

| Field | Type | Required | Description |
|---|---|---|---|
| `workspace` | [`WorkspaceValue`](/docs/reference/api/types#workspacevalue) | yes |  |
