---
title: "group.*"
description: "Groups of workspaces in the sidebar."
sidebar_position: 4
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

Groups of workspaces in the sidebar.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`group.create`](#group-create) | `ui` `cli` `phone` | [`GroupCreate`](/docs/reference/api/types#groupcreate) | [`GroupCreated`](/docs/reference/api/types#groupcreated) |
| [`group.rename`](#group-rename) | `ui` `cli` `phone` | [`GroupRename`](/docs/reference/api/types#grouprename) | [`Empty`](/docs/reference/api/types#empty) |
| [`group.close`](#group-close) | `ui` `cli` `phone` | [`GroupClose`](/docs/reference/api/types#groupclose) | [`Empty`](/docs/reference/api/types#empty) |
| [`group.move`](#group-move) | `ui` `cli` `phone` | [`GroupMove`](/docs/reference/api/types#groupmove) | [`Empty`](/docs/reference/api/types#empty) |
| [`group.list`](#group-list) | `ui` `cli` `phone` | [`GroupList`](/docs/reference/api/types#grouplist) | [`GroupListResult`](/docs/reference/api/types#grouplistresult) |
| [`group.show`](#group-show) | `ui` `cli` `phone` | [`GroupShow`](/docs/reference/api/types#groupshow) | [`GroupSummary`](/docs/reference/api/types#groupsummary) |

## `group.create` {#group-create}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh group create`](/docs/reference/cli/commands/group#group-create)

### Params

[`GroupCreate`](/docs/reference/api/types#groupcreate)

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |

### Result

[`GroupCreated`](/docs/reference/api/types#groupcreated)

| Field | Type | Required | Description |
|---|---|---|---|
| `group` | [`GroupId`](/docs/reference/api/types#groupid) | yes |  |

## `group.rename` {#group-rename}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh group rename`](/docs/reference/cli/commands/group#group-rename)

### Params

[`GroupRename`](/docs/reference/api/types#grouprename)

| Field | Type | Required | Description |
|---|---|---|---|
| `group` | [`RefGroupId`](/docs/reference/api/types#refgroupid) | yes |  |
| `name` | string | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `group.close` {#group-close}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh group close`](/docs/reference/cli/commands/group#group-close)

Closing a group keeps its workspaces. They have no group afterwards.

### Params

[`GroupClose`](/docs/reference/api/types#groupclose)

| Field | Type | Required | Description |
|---|---|---|---|
| `group` | [`RefGroupId`](/docs/reference/api/types#refgroupid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `group.move` {#group-move}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh group move`](/docs/reference/cli/commands/group#group-move)

### Params

[`GroupMove`](/docs/reference/api/types#groupmove)

| Field | Type | Required | Description |
|---|---|---|---|
| `before` | [`RefGroupId`](/docs/reference/api/types#refgroupid) \| `null` |  | Put it before this group. Without it the group goes to the end. |
| `group` | [`RefGroupId`](/docs/reference/api/types#refgroupid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `group.list` {#group-list}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh group list`](/docs/reference/cli/commands/group#group-list)

### Params

[`GroupList`](/docs/reference/api/types#grouplist)

An object with no fields: `{}`.

### Result

[`GroupListResult`](/docs/reference/api/types#grouplistresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `groups` | [`GroupSummary`](/docs/reference/api/types#groupsummary)[] | yes |  |

## `group.show` {#group-show}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh group show`](/docs/reference/cli/commands/group#group-show)

### Params

[`GroupShow`](/docs/reference/api/types#groupshow)

| Field | Type | Required | Description |
|---|---|---|---|
| `group` | [`RefGroupId`](/docs/reference/api/types#refgroupid) | yes |  |

### Result

[`GroupSummary`](/docs/reference/api/types#groupsummary)

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | [`GroupId`](/docs/reference/api/types#groupid) | yes |  |
| `name` | string | yes |  |
| `workspaces` | [`WorkspaceId`](/docs/reference/api/types#workspaceid)[] | yes |  |
