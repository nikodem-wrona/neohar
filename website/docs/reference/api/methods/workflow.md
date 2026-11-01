---
title: "workflow.*"
description: "The Workflows view: the workflow files and their graphs, and the executions in a folder with the report of each node."
sidebar_position: 18
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The Workflows view: the workflow files and their graphs, and the executions in a folder with the report of each node.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`workflow.list`](#workflow-list) | `ui` `cli` `phone` | [`Empty`](/docs/reference/api/types#empty) | [`WorkflowList`](/docs/reference/api/types#workflowlist) |
| [`workflow.show`](#workflow-show) | `ui` `phone` | [`WorkflowRef`](/docs/reference/api/types#workflowref) | [`WorkflowShown`](/docs/reference/api/types#workflowshown) |
| [`workflow.create`](#workflow-create) | `ui` `phone` | [`WorkflowCreate`](/docs/reference/api/types#workflowcreate) | [`WorkflowShown`](/docs/reference/api/types#workflowshown) |
| [`workflow.save`](#workflow-save) | `ui` `phone` | [`WorkflowSave`](/docs/reference/api/types#workflowsave) | [`WorkflowShown`](/docs/reference/api/types#workflowshown) |
| [`workflow.rename`](#workflow-rename) | `ui` `phone` | [`WorkflowRename`](/docs/reference/api/types#workflowrename) | [`WorkflowShown`](/docs/reference/api/types#workflowshown) |
| [`workflow.duplicate`](#workflow-duplicate) | `ui` `phone` | [`WorkflowRef`](/docs/reference/api/types#workflowref) | [`WorkflowShown`](/docs/reference/api/types#workflowshown) |
| [`workflow.delete`](#workflow-delete) | `ui` `phone` | [`WorkflowRef`](/docs/reference/api/types#workflowref) | [`Empty`](/docs/reference/api/types#empty) |
| [`workflow.undo`](#workflow-undo) | `ui` `phone` | [`WorkflowRef`](/docs/reference/api/types#workflowref) | [`WorkflowShown`](/docs/reference/api/types#workflowshown) |
| [`workflow.redo`](#workflow-redo) | `ui` `phone` | [`WorkflowRef`](/docs/reference/api/types#workflowref) | [`WorkflowShown`](/docs/reference/api/types#workflowshown) |
| [`workflow.layout`](#workflow-layout) | `ui` `phone` | [`WorkflowLayout`](/docs/reference/api/types#workflowlayout) | [`WorkflowShown`](/docs/reference/api/types#workflowshown) |
| [`workflow.presets`](#workflow-presets) | `ui` `phone` | [`Empty`](/docs/reference/api/types#empty) | [`WorkflowPresets`](/docs/reference/api/types#workflowpresets) |
| [`workflow.facts`](#workflow-facts) | `ui` `phone` | [`WorkflowFactsGet`](/docs/reference/api/types#workflowfactsget) | [`WorkflowFacts`](/docs/reference/api/types#workflowfacts) |
| [`workflow.execute`](#workflow-execute) | `ui` `cli` `phone` | [`WorkflowExecute`](/docs/reference/api/types#workflowexecute) | [`WorkflowExecuted`](/docs/reference/api/types#workflowexecuted) |
| [`workflow.notes_from`](#workflow-notes-from) | `ui` `phone` | [`WorkflowNotesFrom`](/docs/reference/api/types#workflownotesfrom) | [`WorkflowNotes`](/docs/reference/api/types#workflownotes) |
| [`workflow.executions`](#workflow-executions) | `ui` `cli` `phone` | [`WorkflowExecutions`](/docs/reference/api/types#workflowexecutions) | [`WorkflowExecutionList`](/docs/reference/api/types#workflowexecutionlist) |
| [`workflow.execution`](#workflow-execution) | `ui` `cli` `phone` | [`WorkflowExecutionRef`](/docs/reference/api/types#workflowexecutionref) | [`WorkflowExecutionShown`](/docs/reference/api/types#workflowexecutionshown) |
| [`workflow.report`](#workflow-report) | `ui` `cli` `phone` | [`WorkflowReportGet`](/docs/reference/api/types#workflowreportget) | [`WorkflowReport`](/docs/reference/api/types#workflowreport) |
| [`workflow.retry`](#workflow-retry) | `ui` `cli` `phone` | [`WorkflowNodeRef`](/docs/reference/api/types#workflownoderef) | [`Empty`](/docs/reference/api/types#empty) |
| [`workflow.skip`](#workflow-skip) | `ui` `cli` `phone` | [`WorkflowNodeRef`](/docs/reference/api/types#workflownoderef) | [`Empty`](/docs/reference/api/types#empty) |
| [`workflow.cancel`](#workflow-cancel) | `ui` `cli` `phone` | [`WorkflowExecutionRef`](/docs/reference/api/types#workflowexecutionref) | [`Empty`](/docs/reference/api/types#empty) |
| [`workflow.continue`](#workflow-continue) | `ui` `phone` | [`WorkflowContinue`](/docs/reference/api/types#workflowcontinue) | [`JobStarted`](/docs/reference/api/types#jobstarted) |
| [`workflow.reveal`](#workflow-reveal) | `ui` `phone` | [`WorkflowReveal`](/docs/reference/api/types#workflowreveal) | [`Empty`](/docs/reference/api/types#empty) |
| [`workflow.copy_to_folder`](#workflow-copy-to-folder) | `ui` `phone` | [`WorkflowExecutionRef`](/docs/reference/api/types#workflowexecutionref) | [`WorkflowCopied`](/docs/reference/api/types#workflowcopied) |

## `workflow.list` {#workflow-list}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh workflow list`](/docs/reference/cli/commands/workflow#workflow-list)

An empty result or empty params: `{}`.

### Params

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

### Result

[`WorkflowList`](/docs/reference/api/types#workflowlist)

| Field | Type | Required | Description |
|---|---|---|---|
| `workflows` | [`WorkflowValue`](/docs/reference/api/types#workflowvalue)[] | yes |  |

## `workflow.show` {#workflow-show}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

**Command line:** [`nh workflow show`](/docs/reference/cli/commands/workflow#workflow-show)

`workflow.show`, `workflow.delete`, `workflow.duplicate`, `workflow.undo`, `workflow.redo`: one workflow.

### Params

[`WorkflowRef`](/docs/reference/api/types#workflowref)

| Field | Type | Required | Description |
|---|---|---|---|
| `workflow` | string | yes | The name of the file without `.toml`. |

### Result

[`WorkflowShown`](/docs/reference/api/types#workflowshown)

| Field | Type | Required | Description |
|---|---|---|---|
| `can_redo` | boolean |  | `workflow.redo` has a version to go forward to. Default: `false`. |
| `can_undo` | boolean |  | `workflow.undo` has a version to go back to. Default: `false`. |
| `graph` | [`WorkflowGraph`](/docs/reference/api/types#workflowgraph) | yes |  |
| `path` | string | yes | The absolute path of the file. |
| `problems` | [`WorkflowProblem`](/docs/reference/api/types#workflowproblem)[] | yes |  |
| `revision` | string | yes | The revision of the file. `workflow.save` takes it. |
| `workflow` | string | yes | The name of the file without `.toml`. |

## `workflow.create` {#workflow-create}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`workflow.create`: a new workflow file. The name of the file comes from `name`.

### Params

[`WorkflowCreate`](/docs/reference/api/types#workflowcreate)

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `preset` | string \| `null` |  | A preset workflow of `workflow.presets`, for example `review`. Without it, the workflow has only the trigger. |

### Result

[`WorkflowShown`](/docs/reference/api/types#workflowshown)

| Field | Type | Required | Description |
|---|---|---|---|
| `can_redo` | boolean |  | `workflow.redo` has a version to go forward to. Default: `false`. |
| `can_undo` | boolean |  | `workflow.undo` has a version to go back to. Default: `false`. |
| `graph` | [`WorkflowGraph`](/docs/reference/api/types#workflowgraph) | yes |  |
| `path` | string | yes | The absolute path of the file. |
| `problems` | [`WorkflowProblem`](/docs/reference/api/types#workflowproblem)[] | yes |  |
| `revision` | string | yes | The revision of the file. `workflow.save` takes it. |
| `workflow` | string | yes | The name of the file without `.toml`. |

## `workflow.save` {#workflow-save}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`workflow.save`: writes the graph of a workflow.

### Params

[`WorkflowSave`](/docs/reference/api/types#workflowsave)

| Field | Type | Required | Description |
|---|---|---|---|
| `graph` | [`WorkflowGraph`](/docs/reference/api/types#workflowgraph) | yes |  |
| `revision` | string | yes | The revision that the change starts from. An older one answers `conflict`. |
| `workflow` | string | yes |  |

### Result

[`WorkflowShown`](/docs/reference/api/types#workflowshown)

| Field | Type | Required | Description |
|---|---|---|---|
| `can_redo` | boolean |  | `workflow.redo` has a version to go forward to. Default: `false`. |
| `can_undo` | boolean |  | `workflow.undo` has a version to go back to. Default: `false`. |
| `graph` | [`WorkflowGraph`](/docs/reference/api/types#workflowgraph) | yes |  |
| `path` | string | yes | The absolute path of the file. |
| `problems` | [`WorkflowProblem`](/docs/reference/api/types#workflowproblem)[] | yes |  |
| `revision` | string | yes | The revision of the file. `workflow.save` takes it. |
| `workflow` | string | yes | The name of the file without `.toml`. |

## `workflow.rename` {#workflow-rename}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`workflow.rename`: gives a workflow a new name, and its file the name that comes from it.

### Params

[`WorkflowRename`](/docs/reference/api/types#workflowrename)

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `workflow` | string | yes |  |

### Result

[`WorkflowShown`](/docs/reference/api/types#workflowshown)

| Field | Type | Required | Description |
|---|---|---|---|
| `can_redo` | boolean |  | `workflow.redo` has a version to go forward to. Default: `false`. |
| `can_undo` | boolean |  | `workflow.undo` has a version to go back to. Default: `false`. |
| `graph` | [`WorkflowGraph`](/docs/reference/api/types#workflowgraph) | yes |  |
| `path` | string | yes | The absolute path of the file. |
| `problems` | [`WorkflowProblem`](/docs/reference/api/types#workflowproblem)[] | yes |  |
| `revision` | string | yes | The revision of the file. `workflow.save` takes it. |
| `workflow` | string | yes | The name of the file without `.toml`. |

## `workflow.duplicate` {#workflow-duplicate}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`workflow.show`, `workflow.delete`, `workflow.duplicate`, `workflow.undo`, `workflow.redo`: one workflow.

### Params

[`WorkflowRef`](/docs/reference/api/types#workflowref)

| Field | Type | Required | Description |
|---|---|---|---|
| `workflow` | string | yes | The name of the file without `.toml`. |

### Result

[`WorkflowShown`](/docs/reference/api/types#workflowshown)

| Field | Type | Required | Description |
|---|---|---|---|
| `can_redo` | boolean |  | `workflow.redo` has a version to go forward to. Default: `false`. |
| `can_undo` | boolean |  | `workflow.undo` has a version to go back to. Default: `false`. |
| `graph` | [`WorkflowGraph`](/docs/reference/api/types#workflowgraph) | yes |  |
| `path` | string | yes | The absolute path of the file. |
| `problems` | [`WorkflowProblem`](/docs/reference/api/types#workflowproblem)[] | yes |  |
| `revision` | string | yes | The revision of the file. `workflow.save` takes it. |
| `workflow` | string | yes | The name of the file without `.toml`. |

## `workflow.delete` {#workflow-delete}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`workflow.show`, `workflow.delete`, `workflow.duplicate`, `workflow.undo`, `workflow.redo`: one workflow.

### Params

[`WorkflowRef`](/docs/reference/api/types#workflowref)

| Field | Type | Required | Description |
|---|---|---|---|
| `workflow` | string | yes | The name of the file without `.toml`. |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `workflow.undo` {#workflow-undo}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`workflow.show`, `workflow.delete`, `workflow.duplicate`, `workflow.undo`, `workflow.redo`: one workflow.

### Params

[`WorkflowRef`](/docs/reference/api/types#workflowref)

| Field | Type | Required | Description |
|---|---|---|---|
| `workflow` | string | yes | The name of the file without `.toml`. |

### Result

[`WorkflowShown`](/docs/reference/api/types#workflowshown)

| Field | Type | Required | Description |
|---|---|---|---|
| `can_redo` | boolean |  | `workflow.redo` has a version to go forward to. Default: `false`. |
| `can_undo` | boolean |  | `workflow.undo` has a version to go back to. Default: `false`. |
| `graph` | [`WorkflowGraph`](/docs/reference/api/types#workflowgraph) | yes |  |
| `path` | string | yes | The absolute path of the file. |
| `problems` | [`WorkflowProblem`](/docs/reference/api/types#workflowproblem)[] | yes |  |
| `revision` | string | yes | The revision of the file. `workflow.save` takes it. |
| `workflow` | string | yes | The name of the file without `.toml`. |

## `workflow.redo` {#workflow-redo}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`workflow.show`, `workflow.delete`, `workflow.duplicate`, `workflow.undo`, `workflow.redo`: one workflow.

### Params

[`WorkflowRef`](/docs/reference/api/types#workflowref)

| Field | Type | Required | Description |
|---|---|---|---|
| `workflow` | string | yes | The name of the file without `.toml`. |

### Result

[`WorkflowShown`](/docs/reference/api/types#workflowshown)

| Field | Type | Required | Description |
|---|---|---|---|
| `can_redo` | boolean |  | `workflow.redo` has a version to go forward to. Default: `false`. |
| `can_undo` | boolean |  | `workflow.undo` has a version to go back to. Default: `false`. |
| `graph` | [`WorkflowGraph`](/docs/reference/api/types#workflowgraph) | yes |  |
| `path` | string | yes | The absolute path of the file. |
| `problems` | [`WorkflowProblem`](/docs/reference/api/types#workflowproblem)[] | yes |  |
| `revision` | string | yes | The revision of the file. `workflow.save` takes it. |
| `workflow` | string | yes | The name of the file without `.toml`. |

## `workflow.layout` {#workflow-layout}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`workflow.layout`: gives each node a new position, one column for each depth, and saves.

### Params

[`WorkflowLayout`](/docs/reference/api/types#workflowlayout)

| Field | Type | Required | Description |
|---|---|---|---|
| `revision` | string | yes |  |
| `workflow` | string | yes |  |

### Result

[`WorkflowShown`](/docs/reference/api/types#workflowshown)

| Field | Type | Required | Description |
|---|---|---|---|
| `can_redo` | boolean |  | `workflow.redo` has a version to go forward to. Default: `false`. |
| `can_undo` | boolean |  | `workflow.undo` has a version to go back to. Default: `false`. |
| `graph` | [`WorkflowGraph`](/docs/reference/api/types#workflowgraph) | yes |  |
| `path` | string | yes | The absolute path of the file. |
| `problems` | [`WorkflowProblem`](/docs/reference/api/types#workflowproblem)[] | yes |  |
| `revision` | string | yes | The revision of the file. `workflow.save` takes it. |
| `workflow` | string | yes | The name of the file without `.toml`. |

## `workflow.presets` {#workflow-presets}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

An empty result or empty params: `{}`.

### Params

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

### Result

[`WorkflowPresets`](/docs/reference/api/types#workflowpresets)

| Field | Type | Required | Description |
|---|---|---|---|
| `nodes` | [`WorkflowNodePreset`](/docs/reference/api/types#workflownodepreset)[] | yes |  |
| `workflows` | [`WorkflowPreset`](/docs/reference/api/types#workflowpreset)[] | yes |  |

## `workflow.facts` {#workflow-facts}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`workflow.facts`: what the dialog of an execution shows for a folder.

### Params

[`WorkflowFactsGet`](/docs/reference/api/types#workflowfactsget)

| Field | Type | Required | Description |
|---|---|---|---|
| `folder` | string | yes | An absolute path. |

### Result

[`WorkflowFacts`](/docs/reference/api/types#workflowfacts)

| Field | Type | Required | Description |
|---|---|---|---|
| `base` | string \| `null` |  | The default base: the default branch of `origin` (`origin/HEAD`), else `main` or `master`. |
| `branch` | string \| `null` |  |  |
| `folder` | string | yes |  |
| `repo` | string \| `null` |  | The root of the git worktree that holds the folder. |

## `workflow.execute` {#workflow-execute}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh workflow execute`](/docs/reference/cli/commands/workflow#workflow-execute)

`workflow.execute`: starts an execution.

### Params

[`WorkflowExecute`](/docs/reference/api/types#workflowexecute)

| Field | Type | Required | Description |
|---|---|---|---|
| `base` | string \| `null` |  | The base of the changes. Without it, the default of `workflow.facts`. |
| `folder` | string | yes | The absolute path of the folder in which the agents work. |
| `node` | string \| `null` |  | Execute only this node, with the reports of the last execution in this folder as inputs. |
| `notes` | string \| `null` |  | What the person built and why. `{{start.notes}}` gives it to a prompt. |
| `workflow` | string | yes |  |

### Result

[`WorkflowExecuted`](/docs/reference/api/types#workflowexecuted)

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |
| `label` | string | yes | The name of the execution, for example "Review 3". |

## `workflow.notes_from` {#workflow-notes-from}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`workflow.notes_from`: the prompts of an agent run, as one text for the notes.

### Params

[`WorkflowNotesFrom`](/docs/reference/api/types#workflownotesfrom)

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |

### Result

[`WorkflowNotes`](/docs/reference/api/types#workflownotes)

| Field | Type | Required | Description |
|---|---|---|---|
| `text` | string | yes |  |

## `workflow.executions` {#workflow-executions}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh workflow executions`](/docs/reference/cli/commands/workflow#workflow-executions)

`workflow.executions`: the executions, the newest first.

### Params

[`WorkflowExecutions`](/docs/reference/api/types#workflowexecutions)

| Field | Type | Required | Description |
|---|---|---|---|
| `folder` | string \| `null` |  | Only the executions in this folder. |

### Result

[`WorkflowExecutionList`](/docs/reference/api/types#workflowexecutionlist)

| Field | Type | Required | Description |
|---|---|---|---|
| `executions` | [`ExecutionValue`](/docs/reference/api/types#executionvalue)[] | yes |  |

## `workflow.execution` {#workflow-execution}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

`workflow.execution`, `workflow.cancel`, `workflow.copy_to_folder`: one execution.

### Params

[`WorkflowExecutionRef`](/docs/reference/api/types#workflowexecutionref)

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |

### Result

[`WorkflowExecutionShown`](/docs/reference/api/types#workflowexecutionshown)

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionValue`](/docs/reference/api/types#executionvalue) | yes |  |
| `graph` | [`WorkflowGraph`](/docs/reference/api/types#workflowgraph) | yes |  |

## `workflow.report` {#workflow-report}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh workflow report`](/docs/reference/cli/commands/workflow#workflow-report)

`workflow.report`: a report of an execution.

### Params

[`WorkflowReportGet`](/docs/reference/api/types#workflowreportget)

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |
| `node` | string \| `null` |  | The node. Without it, `report.md`. |

### Result

[`WorkflowReport`](/docs/reference/api/types#workflowreport)

| Field | Type | Required | Description |
|---|---|---|---|
| `markdown` | [`Markdown`](/docs/reference/api/types#markdown) | yes |  |
| `path` | string | yes |  |

## `workflow.retry` {#workflow-retry}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh workflow retry`](/docs/reference/cli/commands/workflow#workflow-retry)

`workflow.retry`, `workflow.skip`: one node of an execution.

### Params

[`WorkflowNodeRef`](/docs/reference/api/types#workflownoderef)

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |
| `node` | string | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `workflow.skip` {#workflow-skip}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh workflow skip`](/docs/reference/cli/commands/workflow#workflow-skip)

`workflow.retry`, `workflow.skip`: one node of an execution.

### Params

[`WorkflowNodeRef`](/docs/reference/api/types#workflownoderef)

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |
| `node` | string | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `workflow.cancel` {#workflow-cancel}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh workflow cancel`](/docs/reference/cli/commands/workflow#workflow-cancel)

`workflow.execution`, `workflow.cancel`, `workflow.copy_to_folder`: one execution.

### Params

[`WorkflowExecutionRef`](/docs/reference/api/types#workflowexecutionref)

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `workflow.continue` {#workflow-continue}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`workflow.continue`: starts the agent of a node again with its resume handle, in a new tab that the window shows.

### Params

[`WorkflowContinue`](/docs/reference/api/types#workflowcontinue)

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |
| `node` | string | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  | The window that selects the new tab. |

### Result

[`JobStarted`](/docs/reference/api/types#jobstarted)

| Field | Type | Required | Description |
|---|---|---|---|
| `job` | [`JobId`](/docs/reference/api/types#jobid) | yes |  |

## `workflow.reveal` {#workflow-reveal}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`workflow.reveal`: the tab of the agent of a node shows in the tab bar, and the window selects it.

### Params

[`WorkflowReveal`](/docs/reference/api/types#workflowreveal)

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |
| `node` | string | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `workflow.copy_to_folder` {#workflow-copy-to-folder}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`workflow.execution`, `workflow.cancel`, `workflow.copy_to_folder`: one execution.

### Params

[`WorkflowExecutionRef`](/docs/reference/api/types#workflowexecutionref)

| Field | Type | Required | Description |
|---|---|---|---|
| `execution` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |

### Result

[`WorkflowCopied`](/docs/reference/api/types#workflowcopied)

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | The absolute path of the copy. |
