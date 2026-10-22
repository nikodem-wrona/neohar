---
title: "Projections"
description: "The 26 read models that clients subscribe to: a snapshot, then keyed patches."
sidebar_position: 3
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

A projection is a read model that the core keeps in memory. A client calls [`projection.subscribe`](/docs/reference/api/methods/projection#projection-subscribe), gets a snapshot (`{items, order}`), then keyed patches in [`projection.message`](#notification-projection-message) notifications. A parameterized projection has a name of the form `name:<parameter>`, for example `timeline:<run>`. A singleton projection has one item with the key `_`. See [API concepts](/docs/reference/api/concepts) for versions, `since` and `resync`.

| Projection | Key | Parameter | Value |
|---|---|---|---|
| [`windows`](#windows) | window id |  | [`WindowValue`](/docs/reference/api/types#windowvalue) |
| [`tree`](#tree) | workspace id |  | [`WorkspaceValue`](/docs/reference/api/types#workspacevalue) |
| [`panes`](#panes) | pane id |  | [`PaneValue`](/docs/reference/api/types#panevalue) |
| [`agents`](#agents) | run id |  | [`AgentValue`](/docs/reference/api/types#agentvalue) |
| [`attention`](#attention) | run id |  | [`AttentionValue`](/docs/reference/api/types#attentionvalue) |
| [`interactions`](#interactions) | interaction id |  | [`InteractionValue`](/docs/reference/api/types#interactionvalue) |
| [`timeline`](#timeline) | item id | run id | [`TimelineItem`](/docs/reference/api/types#timelineitem) |
| [`tasks`](#tasks) | task id |  | [`TaskValue`](/docs/reference/api/types#taskvalue) |
| [`changes`](#changes) | path | task id | [`ChangeRow`](/docs/reference/api/types#changerow) |
| [`favorites`](#favorites) | path |  | [`FavoriteValue`](/docs/reference/api/types#favoritevalue) |
| [`notifications`](#notifications) | notification id |  | [`NotificationValue`](/docs/reference/api/types#notificationvalue) |
| [`machines`](#machines) | machine label |  | [`MachineValue`](/docs/reference/api/types#machinevalue) |
| [`harnesses`](#harnesses) | harness id |  | [`HarnessValue`](/docs/reference/api/types#harnessvalue) |
| [`limits`](#limits) | account and harness |  | [`LimitValue`](/docs/reference/api/types#limitvalue) |
| [`jobs`](#jobs) | job id |  | [`JobValue`](/docs/reference/api/types#jobvalue) |
| [`palette`](#palette) | singleton (`_`) |  | [`PaletteValue`](/docs/reference/api/types#palettevalue) |
| [`settings`](#settings) | section name |  | [`SettingsSection`](/docs/reference/api/types#settingssection) |
| [`health`](#health) | singleton (`_`) |  | [`HealthValue`](/docs/reference/api/types#healthvalue) |
| [`link`](#link) | singleton (`_`) |  | [`LinkValue`](/docs/reference/api/types#linkvalue) |
| [`pane_text`](#pane-text) | row number | pane id | string |
| [`agents_for`](#agents-for) | run id | window id | [`AgentValue`](/docs/reference/api/types#agentvalue) |
| [`changes_for`](#changes-for) | path | window id | [`ChangeRow`](/docs/reference/api/types#changerow) |
| [`changes_head`](#changes-head) | singleton (`_`) | window id | [`ChangesHead`](/docs/reference/api/types#changeshead) |
| [`phones`](#phones) | device id |  | [`PhoneValue`](/docs/reference/api/types#phonevalue) |
| [`phone_pairing`](#phone-pairing) | singleton (`_`) |  | [`PhonePairingValue`](/docs/reference/api/types#phonepairingvalue) |
| [`phone_status`](#phone-status) | singleton (`_`) |  | [`PhoneStatusValue`](/docs/reference/api/types#phonestatusvalue) |

## `windows` {#windows}

**Key:** window id

### Value

[`WindowValue`](/docs/reference/api/types#windowvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `bounds` | [`Bounds`](/docs/reference/api/types#bounds) \| `null` |  |  |
| `client` | [`DeviceId`](/docs/reference/api/types#deviceid) \| `null` |  | The device id of the client that owns the window. A client opens a native window only for its own rows. A row with none waits for the first client that attaches. |
| `focused` | boolean | yes | The OS focus of the window, as the host reports it. |
| `focused_pane` | [`PaneId`](/docs/reference/api/types#paneid) \| `null` |  | The pane that has keyboard focus in the window. |
| `id` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |
| `refresh_hz` | number (float) \| `null` |  |  |
| `reveal` | [`Reveal`](/docs/reference/api/types#reveal) \| `null` |  | The interaction that the window must show now (`interaction.reveal`). The window acts when `seq` changes. |
| `single` | boolean |  | The single view of a phone: the window draws only its focused pane, or the first pane of its tab. The core keeps it in memory only. Default: `false`. |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) \| `null` |  | The tab that the window shows. |
| `visible` | boolean | yes |  |
| `visible_panes` | [`PaneId`](/docs/reference/api/types#paneid)[] | yes | The panes that the window draws. The render pacer keys on this. |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) \| `null` |  | The workspace of that tab. The tab bar shows its tabs. |

## `tree` {#tree}

**Key:** workspace id

### Value

[`WorkspaceValue`](/docs/reference/api/types#workspacevalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `group` | [`GroupRef`](/docs/reference/api/types#groupref) \| `null` |  |  |
| `handle` | string | yes | The short handle, for example `w3`. |
| `id` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |
| `machine` | string | yes | The label of the machine. The local machine is `local`. |
| `name` | string | yes |  |
| `root` | string \| `null` |  |  |
| `selected_tab` | [`TabId`](/docs/reference/api/types#tabid) \| `null` |  |  |
| `tabs` | [`TabValue`](/docs/reference/api/types#tabvalue)[] | yes |  |

## `panes` {#panes}

**Key:** pane id

### Value

[`PaneValue`](/docs/reference/api/types#panevalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `alt_screen` | boolean |  | Default: `false`. |
| `bracketed_paste` | boolean |  | Default: `false`. |
| `clipboard_dropped` | [`ClipboardDropped`](/docs/reference/api/types#clipboarddropped) \| `null` |  | The last OSC 52 write that no client got. The next write that a client gets clears it. |
| `clipboard_request` | [`ClipboardRequest`](/docs/reference/api/types#clipboardrequest) \| `null` |  |  |
| `cols` | integer (uint16) | yes |  |
| `cwd` | string \| `null` |  |  |
| `fg_process` | string \| `null` |  | The foreground process. |
| `geometry_owner` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  | The window whose size the PTY follows. |
| `handle` | string | yes | The short handle, for example `w3:p5`. |
| `id` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `input_unconfirmed` | integer (uint64)[] |  | Inputs that were in flight when something went away (`pane.input_unconfirmed`). |
| `liveness` | [`PaneLiveness`](/docs/reference/api/types#paneliveness) | yes |  |
| `mouse_mode` | [`MouseMode`](/docs/reference/api/types#mousemode) | yes |  |
| `render_id` | integer (uint32) | yes | The number that frames carry for this pane. |
| `restoring` | boolean |  | The pane shows the restored screen and the holder has not caught up. Default: `false`. |
| `rows` | integer (uint16) | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) \| `null` |  | The run of an agent in the pane. |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |
| `title` | string | yes |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

## `agents` {#agents}

**Key:** run id

### Value

[`AgentValue`](/docs/reference/api/types#agentvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `binding` | [`Binding`](/docs/reference/api/types#binding) | yes |  |
| `capabilities` | [`Capabilities`](/docs/reference/api/types#capabilities) | yes |  |
| `facets` | [`Facets`](/docs/reference/api/types#facets) | yes |  |
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `harness_version` | string \| `null` |  |  |
| `id` | [`RunId`](/docs/reference/api/types#runid) | yes |  |
| `isolation` | string \| `null` |  |  |
| `model` | string \| `null` |  |  |
| `name` | string \| `null` |  | The unique name, if the run has one. A call can use it instead of the id. |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) \| `null` |  |  |
| `rate_limit` | [`RateLimitWindow`](/docs/reference/api/types#ratelimitwindow) \| `null` |  |  |
| `resumable` | boolean |  | A resume handle exists (`claude --resume`, `codex resume`). Default: `false`. |
| `session_id` | string \| `null` |  |  |
| `started_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `task` | [`TaskId`](/docs/reference/api/types#taskid) \| `null` |  |  |
| `usage` | [`Usage`](/docs/reference/api/types#usage) | yes |  |
| `yolo` | boolean |  | The run bypasses the permission prompts of the harness. Default: `false`. |

## `attention` {#attention}

**Key:** run id

### Value

[`AttentionValue`](/docs/reference/api/types#attentionvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `interactions` | [`InteractionId`](/docs/reference/api/types#interactionid)[] | yes |  |
| `reasons` | [`AttentionReason`](/docs/reference/api/types#attentionreason)[] | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) | yes |  |
| `since` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `urgency` | [`Urgency`](/docs/reference/api/types#urgency) | yes |  |

## `interactions` {#interactions}

**Key:** interaction id

### Value

[`InteractionValue`](/docs/reference/api/types#interactionvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `answerable` | boolean | yes | A client can answer it now. A run that is only suggested is not answerable. |
| `channels` | [`AnswerChannel`](/docs/reference/api/types#answerchannel)[] | yes | The channels that this kind has for this run. |
| `deadline` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  |  |
| `delivery` | [`DeliveryInfo`](/docs/reference/api/types#deliveryinfo) | yes |  |
| `handle` | string | yes | The short handle, for example `i42`. |
| `id` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `kind` | [`InteractionKind`](/docs/reference/api/types#interactionkind) | yes |  |
| `mode` | [`GateMode`](/docs/reference/api/types#gatemode) | yes |  |
| `opened_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) \| `null` |  |  |
| `payload` | [`InteractionPayload`](/docs/reference/api/types#interactionpayload) | yes |  |
| `risk` | [`Risk`](/docs/reference/api/types#risk) | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) \| `null` |  |  |
| `state` | [`InteractionState`](/docs/reference/api/types#interactionstate) | yes |  |
| `title` | string | yes |  |

## `timeline:<run>` {#timeline}

**Key:** item id · **Parameter:** run id

### Value

[`TimelineItem`](/docs/reference/api/types#timelineitem)

| Field | Type | Required | Description |
|---|---|---|---|
| `at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `body` | [`TimelineBody`](/docs/reference/api/types#timelinebody) | yes |  |
| `id` | string | yes |  |
| `turn` | integer (uint32) | yes |  |

## `tasks` {#tasks}

**Key:** task id

### Value

[`TaskValue`](/docs/reference/api/types#taskvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `agents` | [`RunId`](/docs/reference/api/types#runid)[] | yes |  |
| `attachments` | [`AttachmentValue`](/docs/reference/api/types#attachmentvalue)[] | yes | The files of the task, in the order that they came. |
| `base` | string \| `null` |  | The base. A draft has one only when the person asked for it; else the start takes the current branch of the repository. |
| `branch` | string \| `null` |  | The branch. A draft has one only when the person asked for a name. |
| `changes` | [`ChangeTotals`](/docs/reference/api/types#changetotals) \| `null` |  | The totals of the last count of the changes. A task that the core has not counted yet has none. |
| `created_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `diff_stats` | [`DiffStats`](/docs/reference/api/types#diffstats) \| `null` |  | The lines from the merge base of the base and `HEAD` to the worktree. |
| `effort` | string \| `null` |  |  |
| `handle` | string | yes | The short handle, for example `k7`. |
| `harness` | [`Harness`](/docs/reference/api/types#harness) \| `null` |  | The agent that the start runs, with its model, effort level and prompt. |
| `id` | [`TaskId`](/docs/reference/api/types#taskid) | yes |  |
| `model` | string \| `null` |  |  |
| `ports` | integer (uint16)[] | yes |  |
| `prompt` | string \| `null` |  |  |
| `repo` | string | yes |  |
| `status` | [`TaskStatus`](/docs/reference/api/types#taskstatus) | yes |  |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) \| `null` |  | The tab of the task. A window that shows it shows the task in its drawer. |
| `title` | string | yes |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) \| `null` |  |  |
| `worktree` | string \| `null` |  | The worktree. A draft has none yet. |

## `changes:<task>` {#changes}

**Key:** path · **Parameter:** task id

### Value

[`ChangeRow`](/docs/reference/api/types#changerow)

| Field | Type | Required | Description |
|---|---|---|---|
| `add` | integer (uint32) | yes |  |
| `binary` | boolean |  | Git shows no lines for the file. Default: `false`. |
| `del` | integer (uint32) | yes |  |
| `dir` | string | yes | The folder of the path with a `/` at its end, or empty for a file at the root. |
| `name` | string | yes | The last part of the path. |
| `old_path` | string \| `null` |  | The path in the merge base of a renamed file. |
| `path` | string | yes | The path from the root of the worktree. |
| `stamp` | integer (uint64) | yes | Changes when the file in the worktree changes, so that an open diff loads again. It is 0 for a deleted file, and less than 2^53 so that JavaScript holds it exactly. |
| `status` | [`ChangeStatus`](/docs/reference/api/types#changestatus) | yes |  |
| `uncommitted` | boolean |  | The file has a change that is not in a commit. Default: `false`. |

## `favorites` {#favorites}

**Key:** path

### Value

[`FavoriteValue`](/docs/reference/api/types#favoritevalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | The name that the form shows: the last part of the path, or the last two parts when two favorites have the same last part. |
| `path` | string | yes | The root of a git repository. |
| `used_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes | The last task that the person made or changed with this repository, else the time when the favorite was added. |

## `notifications` {#notifications}

**Key:** notification id

### Value

[`NotificationValue`](/docs/reference/api/types#notificationvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `id` | [`NotificationId`](/docs/reference/api/types#notificationid) | yes |  |
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) \| `null` |  |  |
| `seen` | boolean | yes |  |
| `text` | string | yes |  |
| `urgency` | [`Urgency`](/docs/reference/api/types#urgency) | yes |  |

## `machines` {#machines}

**Key:** machine label

### Value

[`MachineValue`](/docs/reference/api/types#machinevalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `forwards` | [`PortForward`](/docs/reference/api/types#portforward)[] |  | The ports forwarded to the machine. Default: `[]`. |
| `harnesses` | [`MachineHarness`](/docs/reference/api/types#machineharness)[] |  | The summary of the harnesses of the machine, copied from its `harnesses` projection. Default: `[]`. |
| `label` | string | yes |  |
| `latency_ms` | number (float) \| `null` |  |  |
| `reason` | string \| `null` |  | Why the link is not connected: the reason of the last disconnect or the last failure. |
| `state` | [`MachineState`](/docs/reference/api/types#machinestate) | yes |  |
| `update` | [`MachineUpdate`](/docs/reference/api/types#machineupdate) \| `null` |  |  |
| `version` | string \| `null` |  | The version of the core of the machine, from `Welcome`. |

## `harnesses` {#harnesses}

**Key:** harness id

### Value

[`HarnessValue`](/docs/reference/api/types#harnessvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `available` | boolean | yes | A start of an agent with this harness can go on: the harness is installed and not logged out. The form of a task turns its Start button off without it. |
| `checked_at` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  | The time of the last check. |
| `hint` | string \| `null` |  | What the person can do: `[auth] login_hint` of the manifest, or why the program was not found. |
| `id` | string | yes |  |
| `installed` | boolean | yes | The program of `[launch] argv` is on the `PATH` of the login environment. |
| `login` | [`LoginState`](/docs/reference/api/types#loginstate) | yes |  |
| `login_method` | [`LoginMethod`](/docs/reference/api/types#loginmethod) \| `null` |  |  |
| `login_source` | [`LoginSource`](/docs/reference/api/types#loginsource) \| `null` |  |  |
| `name` | string | yes |  |
| `path` | string \| `null` |  | The program that the core found. |
| `validated` | boolean | yes | The version is in a range of `[[capabilities]]` of the manifest. |
| `version` | string \| `null` |  |  |

## `limits` {#limits}

**Key:** account and harness

### Value

[`LimitValue`](/docs/reference/api/types#limitvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `account` | string | yes |  |
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `usage_today` | [`Usage`](/docs/reference/api/types#usage) | yes |  |
| `windows` | [`RateLimitWindow`](/docs/reference/api/types#ratelimitwindow)[] | yes |  |

## `jobs` {#jobs}

**Key:** job id

### Value

[`JobValue`](/docs/reference/api/types#jobvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `error` | [`CallError`](/docs/reference/api/types#callerror) \| `null` |  |  |
| `finished_at` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  |  |
| `id` | [`JobId`](/docs/reference/api/types#jobid) | yes |  |
| `message` | string \| `null` |  |  |
| `progress` | number (float) \| `null` |  | From 0 to 1, when the job knows. |
| `result` | any |  | What the job made, when it is done (for example `{"task": "…"}`). |
| `started_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `status` | [`JobStatus`](/docs/reference/api/types#jobstatus) | yes |  |
| `title` | string | yes |  |

## `palette` {#palette}

**Key:** singleton, the one key is `_`

### Value

[`PaletteValue`](/docs/reference/api/types#palettevalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `commands` | [`PaletteCommand`](/docs/reference/api/types#palettecommand)[] | yes |  |

## `settings` {#settings}

**Key:** section name

### Value

[`SettingsSection`](/docs/reference/api/types#settingssection)

| Field | Type | Required | Description |
|---|---|---|---|
| `entries` | map of string to [`SettingEntry`](/docs/reference/api/types#settingentry) | yes |  |

## `health` {#health}

**Key:** singleton, the one key is `_`

### Value

[`HealthValue`](/docs/reference/api/types#healthvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `holder_protocols` | integer (uint16)[] | yes |  |
| `renderer` | [`RendererKind`](/docs/reference/api/types#rendererkind) \| `null` |  |  |
| `storage` | [`StorageHealth`](/docs/reference/api/types#storagehealth) | yes |  |
| `unvalidated_harnesses` | [`UnvalidatedHarness`](/docs/reference/api/types#unvalidatedharness)[] | yes |  |
| `warnings` | [`DoctorWarning`](/docs/reference/api/types#doctorwarning)[] | yes |  |

## `link` {#link}

**Key:** singleton, the one key is `_`

### Value

[`LinkValue`](/docs/reference/api/types#linkvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `changed_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes | When the state last changed. |
| `machine` | string | yes | The label of the machine. |
| `reason` | string \| `null` |  | Why the link is not connected. |
| `rtt_ms` | number (float) \| `null` |  |  |
| `state` | [`MachineState`](/docs/reference/api/types#machinestate) | yes |  |
| `text` | string | yes | What the status bar shows: `local`, the label of a link that is up, or the label with the state for a link that is not. The host makes it, and the page prints it. |

## `pane_text:<pane>` {#pane-text}

**Key:** row number · **Parameter:** pane id

### Value

Type: string.

## `agents_for:<window>` {#agents-for}

**Key:** run id · **Parameter:** window id

### Value

[`AgentValue`](/docs/reference/api/types#agentvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `binding` | [`Binding`](/docs/reference/api/types#binding) | yes |  |
| `capabilities` | [`Capabilities`](/docs/reference/api/types#capabilities) | yes |  |
| `facets` | [`Facets`](/docs/reference/api/types#facets) | yes |  |
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `harness_version` | string \| `null` |  |  |
| `id` | [`RunId`](/docs/reference/api/types#runid) | yes |  |
| `isolation` | string \| `null` |  |  |
| `model` | string \| `null` |  |  |
| `name` | string \| `null` |  | The unique name, if the run has one. A call can use it instead of the id. |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) \| `null` |  |  |
| `rate_limit` | [`RateLimitWindow`](/docs/reference/api/types#ratelimitwindow) \| `null` |  |  |
| `resumable` | boolean |  | A resume handle exists (`claude --resume`, `codex resume`). Default: `false`. |
| `session_id` | string \| `null` |  |  |
| `started_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `task` | [`TaskId`](/docs/reference/api/types#taskid) \| `null` |  |  |
| `usage` | [`Usage`](/docs/reference/api/types#usage) | yes |  |
| `yolo` | boolean |  | The run bypasses the permission prompts of the harness. Default: `false`. |

## `changes_for:<window>` {#changes-for}

**Key:** path · **Parameter:** window id

### Value

[`ChangeRow`](/docs/reference/api/types#changerow)

| Field | Type | Required | Description |
|---|---|---|---|
| `add` | integer (uint32) | yes |  |
| `binary` | boolean |  | Git shows no lines for the file. Default: `false`. |
| `del` | integer (uint32) | yes |  |
| `dir` | string | yes | The folder of the path with a `/` at its end, or empty for a file at the root. |
| `name` | string | yes | The last part of the path. |
| `old_path` | string \| `null` |  | The path in the merge base of a renamed file. |
| `path` | string | yes | The path from the root of the worktree. |
| `stamp` | integer (uint64) | yes | Changes when the file in the worktree changes, so that an open diff loads again. It is 0 for a deleted file, and less than 2^53 so that JavaScript holds it exactly. |
| `status` | [`ChangeStatus`](/docs/reference/api/types#changestatus) | yes |  |
| `uncommitted` | boolean |  | The file has a change that is not in a commit. Default: `false`. |

## `changes_head:<window>` {#changes-head}

**Key:** singleton, the one key is `_` · **Parameter:** window id

### Value

[`ChangesHead`](/docs/reference/api/types#changeshead)

| Field | Type | Required | Description |
|---|---|---|---|
| `base` | string \| `null` |  | The base of that task: the changes go from the merge base of the base and `HEAD`, as for `changes:<task>`. With `None`, they go from `HEAD`: only what is not committed. |
| `branch` | string \| `null` |  | The branch of `HEAD`, or `None` for a detached `HEAD`. |
| `changes` | [`ChangeTotals`](/docs/reference/api/types#changetotals) \| `null` |  | The totals of the last count, or `None` before the first count. |
| `diff_stats` | [`DiffStats`](/docs/reference/api/types#diffstats) \| `null` |  | The lines of all changed files. |
| `folder` | string | yes | The folder that the window named. |
| `root` | string \| `null` |  | The root of the git worktree that holds the folder, or `None` when the folder is not in a git worktree. |
| `task` | [`TaskId`](/docs/reference/api/types#taskid) \| `null` |  | The task whose worktree is the root. |

## `phones` {#phones}

**Key:** device id

### Value

[`PhoneValue`](/docs/reference/api/types#phonevalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) | yes |  |
| `name` | string | yes | The name that the phone gave when it paired, or that `phone.rename` set. |
| `paired_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `push` | boolean |  | Whether the phone has a push subscription. Default: `false`. |
| `seen_at` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  | The time of the last connection of the phone, or `null` before the first one. |

## `phone_pairing` {#phone-pairing}

**Key:** singleton, the one key is `_`

### Value

[`PhonePairingValue`](/docs/reference/api/types#phonepairingvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `code` | string \| `null` |  | The short code that a person types: 8 Crockford base32 characters, as `ABCD-EFGH`. |
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) \| `null` |  | The phone that paired with the offer. |
| `expires_at` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  | When the offer ends. |
| `note` | string \| `null` |  | The time that the offer is good for, as a person reads it, for example "The code is good for 10 minutes and for one phone.". The page shows it as it is. |
| `qr` | string \| `null` |  | The QR code of `url` as an SVG `data:` URL. The page shows it in an image. |
| `state` | [`PairingState`](/docs/reference/api/types#pairingstate) | yes |  |
| `url` | string \| `null` |  | The URL of the QR code: the public origin, `/pair?code=` and the long code. |

## `phone_status` {#phone-status}

**Key:** singleton, the one key is `_`

### Value

[`PhoneStatusValue`](/docs/reference/api/types#phonestatusvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `hint` | string \| `null` |  | What the person can do when the listener cannot serve a phone. `phone.pair_begin` refuses while there is a hint. |
| `listening` | boolean | yes | Whether the listener serves its port. |
| `origin` | string \| `null` |  | The HTTPS origin that a phone uses, for example `https://studio.example.ts.net:8443`. |
| `port` | integer (uint16) \| `null` |  | The port of `127.0.0.1`, when the listener has one. |
| `summary` | string |  | One line about the listener, for the settings window and the doctor, for example "Published with Tailscale at https://studio.example.ts.net:8443.". Default: `""`. |
| `tailscale` | [`TailscaleStatus`](/docs/reference/api/types#tailscalestatus) \| `null` |  | Tailscale. `null` when the listener does not use it: the listener is off, `phone.url` names the origin, or the run is a development run. |
| `vapid_key` | string \| `null` |  | The public VAPID key of this core: the uncompressed P-256 point, base64url. The phone app subscribes to push with it. `null` while the listener does not run. |

## Notification `projection.message` {#notification-projection-message}

[`ProjectionNotification`](/docs/reference/api/types#projectionnotification)

| Field | Type | Required | Description |
|---|---|---|---|
| `message` | [`ProjectionMessage`](/docs/reference/api/types#projectionmessage) | yes |  |
| `subscription` | string | yes |  |
