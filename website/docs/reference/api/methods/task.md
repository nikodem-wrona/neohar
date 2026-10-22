---
title: "task.*"
description: "Task workspaces: a worktree, a branch, ports, an environment and an agent."
sidebar_position: 15
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

Task workspaces: a worktree, a branch, ports, an environment and an agent.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`task.create`](#task-create) | `ui` `cli` `phone` | [`TaskCreate`](/docs/reference/api/types#taskcreate) | [`TaskCreated`](/docs/reference/api/types#taskcreated) |
| [`task.update`](#task-update) | `ui` `cli` `phone` | [`TaskUpdate`](/docs/reference/api/types#taskupdate) | [`Empty`](/docs/reference/api/types#empty) |
| [`task.start`](#task-start) | `ui` `cli` `phone` | [`TaskStart`](/docs/reference/api/types#taskstart) | [`JobStarted`](/docs/reference/api/types#jobstarted) |
| [`task.delete`](#task-delete) | `ui` `cli` `phone` | [`TaskDelete`](/docs/reference/api/types#taskdelete) | [`Empty`](/docs/reference/api/types#empty) |
| [`task.list`](#task-list) | `ui` `cli` `phone` | [`TaskList`](/docs/reference/api/types#tasklist) | [`TaskListResult`](/docs/reference/api/types#tasklistresult) |
| [`task.show`](#task-show) | `ui` `cli` `phone` | [`TaskShow`](/docs/reference/api/types#taskshow) | [`TaskShown`](/docs/reference/api/types#taskshown) |
| [`task.changes`](#task-changes) | `ui` `cli` `phone` | [`TaskChanges`](/docs/reference/api/types#taskchanges) | [`TaskChangesResult`](/docs/reference/api/types#taskchangesresult) |
| [`task.diff`](#task-diff) | `ui` `cli` `phone` | [`TaskDiff`](/docs/reference/api/types#taskdiff) | [`FileDiff`](/docs/reference/api/types#filediff) |
| [`task.close`](#task-close) | `ui` `cli` `phone` | [`TaskClose`](/docs/reference/api/types#taskclose) | [`Empty`](/docs/reference/api/types#empty) |

## `task.create` {#task-create}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh task create`](/docs/reference/cli/commands/task#task-create)

`task.create` keeps a draft: the core checks the repository and the words, and stores the task with its files. Nothing happens in git until `task.start`, or until `start`.

### Params

[`TaskCreate`](/docs/reference/api/types#taskcreate)

| Field | Type | Required | Description |
|---|---|---|---|
| `base` | string \| `null` |  |  |
| `branch` | string \| `null` |  |  |
| `effort` | string \| `null` |  | The effort level of the agent. It needs `harness`. |
| `files` | string[] |  | Files to attach, by path. The core reads them now. |
| `harness` | [`Harness`](/docs/reference/api/types#harness) \| `null` |  |  |
| `model` | string \| `null` |  | The model of the agent. It needs `harness`. |
| `ports` | integer (uint8) \| `null` |  | How many ports to lease. |
| `prompt` | string \| `null` |  |  |
| `repo` | string | yes |  |
| `start` | boolean |  | Start the task at once, as `task.start` does. Default: `false`. |
| `title` | string | yes |  |
| `uploads` | [`Upload`](/docs/reference/api/types#upload)[] |  | Files to attach with their bytes: what the UI picked or pasted. |

### Result

[`TaskCreated`](/docs/reference/api/types#taskcreated)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes | The short handle, for example `k7`. |
| `job` | [`JobId`](/docs/reference/api/types#jobid) \| `null` |  | The job of the start, when the call asked for `start`. |
| `task` | [`TaskId`](/docs/reference/api/types#taskid) | yes |  |

## `task.update` {#task-update}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh task update`](/docs/reference/cli/commands/task#task-update)

A change of a task. A missing field keeps its value, and `null` clears it. A draft can change everything. A task that started can change only its title.

### Params

[`TaskUpdate`](/docs/reference/api/types#taskupdate)

| Field | Type | Required | Description |
|---|---|---|---|
| `base` | string \| `null` |  |  |
| `branch` | string \| `null` |  |  |
| `effort` | string \| `null` |  |  |
| `files` | string[] |  | Files to attach, by path. |
| `harness` | [`Harness`](/docs/reference/api/types#harness) \| `null` |  |  |
| `model` | string \| `null` |  |  |
| `ports` | integer (uint8) \| `null` |  |  |
| `prompt` | string \| `null` |  |  |
| `remove` | string[] |  | The attachments to remove, by id. |
| `repo` | string \| `null` |  |  |
| `task` | [`RefTaskId`](/docs/reference/api/types#reftaskid) | yes |  |
| `title` | string \| `null` |  |  |
| `uploads` | [`Upload`](/docs/reference/api/types#upload)[] |  | Files to attach with their bytes. |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `task.start` {#task-start}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh task start`](/docs/reference/cli/commands/task#task-start)

`task.start` returns a job: the worktree, the ports, the setup, the tab and the agent of a draft.

### Params

[`TaskStart`](/docs/reference/api/types#taskstart)

| Field | Type | Required | Description |
|---|---|---|---|
| `task` | [`RefTaskId`](/docs/reference/api/types#reftaskid) | yes |  |

### Result

[`JobStarted`](/docs/reference/api/types#jobstarted)

| Field | Type | Required | Description |
|---|---|---|---|
| `job` | [`JobId`](/docs/reference/api/types#jobid) | yes |  |

## `task.delete` {#task-delete}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh task delete`](/docs/reference/cli/commands/task#task-delete)

`task.delete` removes a draft or a closed task.

### Params

[`TaskDelete`](/docs/reference/api/types#taskdelete)

| Field | Type | Required | Description |
|---|---|---|---|
| `task` | [`RefTaskId`](/docs/reference/api/types#reftaskid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `task.list` {#task-list}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh task list`](/docs/reference/cli/commands/task#task-list)

### Params

[`TaskList`](/docs/reference/api/types#tasklist)

| Field | Type | Required | Description |
|---|---|---|---|
| `status` | [`TaskStatus`](/docs/reference/api/types#taskstatus) \| `null` |  |  |

### Result

[`TaskListResult`](/docs/reference/api/types#tasklistresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `tasks` | [`TaskValue`](/docs/reference/api/types#taskvalue)[] | yes |  |

## `task.show` {#task-show}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh task show`](/docs/reference/cli/commands/task#task-show)

### Params

[`TaskShow`](/docs/reference/api/types#taskshow)

| Field | Type | Required | Description |
|---|---|---|---|
| `task` | [`RefTaskId`](/docs/reference/api/types#reftaskid) | yes |  |

### Result

[`TaskShown`](/docs/reference/api/types#taskshown)

| Field | Type | Required | Description |
|---|---|---|---|
| `task` | [`TaskValue`](/docs/reference/api/types#taskvalue) | yes |  |

## `task.changes` {#task-changes}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh task changes`](/docs/reference/cli/commands/task#task-changes)

`task.changes` counts the changes of a task that started now.

### Params

[`TaskChanges`](/docs/reference/api/types#taskchanges)

| Field | Type | Required | Description |
|---|---|---|---|
| `task` | [`RefTaskId`](/docs/reference/api/types#reftaskid) | yes |  |

### Result

[`TaskChangesResult`](/docs/reference/api/types#taskchangesresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `changes` | [`ChangeTotals`](/docs/reference/api/types#changetotals) | yes |  |
| `files` | [`ChangeRow`](/docs/reference/api/types#changerow)[] | yes |  |

## `task.diff` {#task-diff}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh task diff`](/docs/reference/cli/commands/task#task-diff)

`task.diff` gives the diff of one path of the rows of `changes:<task>`.

### Params

[`TaskDiff`](/docs/reference/api/types#taskdiff)

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes |  |
| `task` | [`RefTaskId`](/docs/reference/api/types#reftaskid) | yes |  |

### Result

[`FileDiff`](/docs/reference/api/types#filediff)

| Field | Type | Required | Description |
|---|---|---|---|
| `hunks` | [`Hunk`](/docs/reference/api/types#hunk)[] | yes |  |
| `path` | string | yes |  |
| `stats` | [`DiffStats`](/docs/reference/api/types#diffstats) | yes |  |
| `status` | [`FileStatus`](/docs/reference/api/types#filestatus) | yes |  |
| `truncated` | boolean |  | The diff of this file was cut at 2,000 lines. The UI offers "open full". Default: `false`. |

## `task.close` {#task-close}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh task close`](/docs/reference/cli/commands/task#task-close)

The core refuses to remove a dirty worktree without `force`.

### Params

[`TaskClose`](/docs/reference/api/types#taskclose)

| Field | Type | Required | Description |
|---|---|---|---|
| `force` | boolean |  | Default: `false`. |
| `remove_worktree` | boolean |  | Default: `false`. |
| `task` | [`RefTaskId`](/docs/reference/api/types#reftaskid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.
