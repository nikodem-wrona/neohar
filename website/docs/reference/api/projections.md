---
title: "Projections"
description: "The 32 read models that clients subscribe to: a snapshot, then keyed patches."
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
| [`git_repos`](#git-repos) | common git directory |  | [`GitRepoValue`](/docs/reference/api/types#gitrepovalue) |
| [`git_head`](#git-head) | singleton (`_`) | window id | [`GitHead`](/docs/reference/api/types#githead) |
| [`git_worktrees`](#git-worktrees) | path | window id | [`GitWorktreeValue`](/docs/reference/api/types#gitworktreevalue) |
| [`git_branches`](#git-branches) | reference | window id | [`GitBranchValue`](/docs/reference/api/types#gitbranchvalue) |
| [`git_changes`](#git-changes) | path | window id | [`ChangeRow`](/docs/reference/api/types#changerow) |
| [`workflows`](#workflows) | file name |  | [`WorkflowValue`](/docs/reference/api/types#workflowvalue) |
| [`workflow_executions`](#workflow-executions) | execution id |  | [`ExecutionValue`](/docs/reference/api/types#executionvalue) |
| [`hermes_chats`](#hermes-chats) | chat id |  | [`HermesChatValue`](/docs/reference/api/types#hermeschatvalue) |
| [`hermes_chat`](#hermes-chat) | item id | chat id | [`HermesItem`](/docs/reference/api/types#hermesitem) |

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
| `always_rule` | string \| `null` |  | The rule that "Allow always" adds, such as `pnpm test:*` or `src/app/**`. Only an approval that a rule may cover, with an answer through the hook, has it. Without it the core refuses "always". |
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
| `available` | boolean | yes | A start of an agent with this harness can go on: the harness is installed and not logged out. |
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
| `result` | any |  | What the job made, when it is done (for example `{"run": "…"}`). |
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
| `old_path` | string \| `null` |  | The path in `HEAD` of a renamed file. |
| `path` | string | yes | The path from the root of the worktree. |
| `stamp` | integer (uint64) | yes | Changes when the file in the worktree changes, so that an open diff loads again. It is 0 for a deleted file, and less than 2^53 so that JavaScript holds it exactly. |
| `status` | [`ChangeStatus`](/docs/reference/api/types#changestatus) | yes |  |

## `changes_head:<window>` {#changes-head}

**Key:** singleton, the one key is `_` · **Parameter:** window id

### Value

[`ChangesHead`](/docs/reference/api/types#changeshead)

| Field | Type | Required | Description |
|---|---|---|---|
| `branch` | string \| `null` |  | The branch of `HEAD`, or `None` for a detached `HEAD`. |
| `changes` | [`ChangeTotals`](/docs/reference/api/types#changetotals) \| `null` |  | The totals of the last count, or `None` before the first count. |
| `diff_stats` | [`DiffStats`](/docs/reference/api/types#diffstats) \| `null` |  | The lines of all changed files. |
| `folder` | string | yes | The folder that the window named. |
| `root` | string \| `null` |  | The root of the git worktree that holds the folder, or `None` when the folder is not in a git worktree. |
| `trust` | [`GitTrustAsk`](/docs/reference/api/types#gittrustask) \| `null` |  | The core runs no git command in the repository until the person trusts it. Then `root` is the root of the worktree, and the head has no branch and no count. |

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

## `git_repos` {#git-repos}

**Key:** common git directory

### Value

[`GitRepoValue`](/docs/reference/api/types#gitrepovalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `agents` | integer (uint32) | yes | The live agents whose pane folder is in a worktree of the repository. |
| `common_dir` | string | yes | The git directory that the worktrees of the repository share. |
| `name` | string | yes | The name of the folder of the main worktree. |
| `root` | string | yes | The worktree that the Git view opens: the main one, or the one where the core found the repository when the main one has no files (a bare repository). |
| `source` | [`GitRepoSource`](/docs/reference/api/types#gitreposource) | yes |  |

## `git_head:<window>` {#git-head}

**Key:** singleton, the one key is `_` · **Parameter:** window id

### Value

[`GitHead`](/docs/reference/api/types#githead)

| Field | Type | Required | Description |
|---|---|---|---|
| `agents` | [`GitAgent`](/docs/reference/api/types#gitagent)[] |  | The agents whose pane folder is in the worktree. |
| `ahead` | integer (uint32) |  | The commits of the branch that its upstream does not have. Default: `0`. |
| `behind` | integer (uint32) |  | The commits of the upstream that the branch does not have. Default: `0`. |
| `branch` | string \| `null` |  | The branch of `HEAD`, or `None` for a detached `HEAD`. |
| `changes` | [`ChangeTotals`](/docs/reference/api/types#changetotals) \| `null` |  | The totals of the last count of the changes, or `None` before the first count. |
| `commit` | string \| `null` |  | The short hash of `HEAD`, or `None` before the first commit. |
| `diff_stats` | [`DiffStats`](/docs/reference/api/types#diffstats) \| `null` |  | The lines of all changed files. |
| `error` | string \| `null` |  | Why the last look at the branches and the worktrees failed. The other values are those of the look before. |
| `fetch_note` | string \| `null` |  | Why the last fetch in the background failed, for example "The fetch needs your passphrase. Click Fetch.". A fetch that works clears it. |
| `fetched_at` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  | The time of the last fetch, from `FETCH_HEAD`. |
| `fetched_text` | string \| `null` |  | For example "Last fetched 18 minutes ago" or "Never fetched". `None` without a remote. |
| `folder` | string | yes | The folder that the window named. |
| `job` | [`GitJob`](/docs/reference/api/types#gitjob) \| `null` |  | The fetch or the pull that runs in the repository. |
| `remote` | string \| `null` |  | The remote of a fetch: the remote of the upstream, else `origin`, else the first one. |
| `repo` | string \| `null` |  | The key of the repository in `git_repos`: its common git directory. |
| `repo_name` | string \| `null` |  |  |
| `stash` | [`GitStash`](/docs/reference/api/types#gitstash) \| `null` |  | The stash that a switch left for the branch of `HEAD`, the newest one. |
| `trust` | [`GitTrustAsk`](/docs/reference/api/types#gittrustask) \| `null` |  | The core runs no git command in the repository until the person trusts it. Then the head has the worktree and the repository, and no branch, no count and no rows. |
| `upstream` | string \| `null` |  | The upstream of the branch, for example `origin/main`. |
| `upstream_gone` | boolean |  | The upstream is gone from its remote. Default: `false`. |
| `worktree` | string \| `null` |  | The root of the worktree that holds the folder, or `None` when the folder is not in a git worktree. |
| `worktree_name` | string \| `null` |  | The last part of the root of the worktree. |

## `git_worktrees:<window>` {#git-worktrees}

**Key:** path · **Parameter:** window id

### Value

[`GitWorktreeValue`](/docs/reference/api/types#gitworktreevalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `agents` | [`GitAgent`](/docs/reference/api/types#gitagent)[] |  |  |
| `branch` | string \| `null` |  | The branch of the worktree, or `None` for a detached `HEAD`. |
| `commit` | string \| `null` |  | The short hash of its `HEAD`. |
| `current` | boolean |  | The worktree that the Git view shows. Default: `false`. |
| `dirty` | integer (uint32) \| `null` |  | The files that are not committed, or `None` when the core could not count them. |
| `locked` | boolean |  | Default: `false`. |
| `main` | boolean |  | The main worktree of the repository. Default: `false`. |
| `name` | string | yes | The last part of the path. |
| `path` | string | yes |  |
| `prunable` | boolean |  | The folder of the worktree is gone, and `git worktree prune` would remove it. Default: `false`. |

## `git_branches:<window>` {#git-branches}

**Key:** reference · **Parameter:** window id

### Value

[`GitBranchValue`](/docs/reference/api/types#gitbranchvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `ahead` | integer (uint32) |  | Default: `0`. |
| `behind` | integer (uint32) |  | Default: `0`. |
| `current` | boolean |  | The worktree of the Git view has it out. Default: `false`. |
| `kind` | [`GitBranchKind`](/docs/reference/api/types#gitbranchkind) | yes |  |
| `name` | string | yes | For example `main` or `origin/main`. |
| `recent` | boolean |  | The worktree had it out a short time ago. Default: `false`. |
| `reference` | string | yes | For example `refs/heads/main`. |
| `subject` | string \| `null` |  | The subject of its last commit. |
| `time` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  | The time of its last commit. |
| `time_text` | string \| `null` |  | For example "3 days ago". |
| `upstream` | string \| `null` |  | The upstream of a local branch, for example `origin/main`. |
| `worktree` | string \| `null` |  | Another worktree has the branch out, so a switch to it goes to that worktree. |

## `git_changes:<window>` {#git-changes}

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
| `old_path` | string \| `null` |  | The path in `HEAD` of a renamed file. |
| `path` | string | yes | The path from the root of the worktree. |
| `stamp` | integer (uint64) | yes | Changes when the file in the worktree changes, so that an open diff loads again. It is 0 for a deleted file, and less than 2^53 so that JavaScript holds it exactly. |
| `status` | [`ChangeStatus`](/docs/reference/api/types#changestatus) | yes |  |

## `workflows` {#workflows}

**Key:** file name

### Value

[`WorkflowValue`](/docs/reference/api/types#workflowvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | The name in the file, for example "Review". |
| `nodes` | integer (uint32) | yes | The number of nodes. |
| `problems` | integer (uint32) | yes | The number of errors. A workflow with errors cannot be executed. |
| `revision` | string | yes | The revision of the file, as `workflow.show` gives it. |
| `workflow` | string | yes | The name of the file without `.toml`. Calls name the workflow with it. |

## `workflow_executions` {#workflow-executions}

**Key:** execution id

### Value

[`ExecutionValue`](/docs/reference/api/types#executionvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `base` | string \| `null` |  | The base of the changes, for example `main`. |
| `branch` | string \| `null` |  |  |
| `dir` | string | yes | The folder of the files of the execution. |
| `ended_at` | [`Timestamp`](/docs/reference/api/types#timestamp) \| `null` |  |  |
| `folder` | string | yes | The folder in which the agents work. |
| `head` | string \| `null` |  | The `HEAD` commit at the start, short. |
| `id` | [`ExecutionId`](/docs/reference/api/types#executionid) | yes |  |
| `label` | string | yes | The name of the execution, for example "Review 3". |
| `nodes` | [`ExecutionNodeValue`](/docs/reference/api/types#executionnodevalue)[] | yes | The nodes, in the order of the graph. |
| `number` | integer (uint32) | yes | The number of the execution of this workflow, from 1. |
| `only` | string \| `null` |  | The node of "Execute this node", when the execution has only that node. |
| `report` | string \| `null` |  | The path of `report.md`, when a Report node wrote it. |
| `started_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `state` | [`ExecutionState`](/docs/reference/api/types#executionstate) | yes |  |
| `time_text` | string | yes | The time and the length as a person reads it, for example "14:12 · 14 min". |
| `tree_changed` | boolean |  | The `HEAD` commit or the changes of the worktree are different from the start. Default: `false`. |
| `workflow` | string | yes | The name of the workflow file without `.toml`. |

## `hermes_chats` {#hermes-chats}

**Key:** chat id

### Value

[`HermesChatValue`](/docs/reference/api/types#hermeschatvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `context_text` | string \| `null` |  | The context window as a person reads it, for example "15k of 500k tokens". |
| `cwd` | string | yes | The folder in which Hermes works. |
| `error` | string \| `null` |  |  |
| `guessed` | boolean |  | The state of the turn is a guess: the core found the turn when it resumed the session, and ACP tells the end of a turn only to the client that sent the prompt. Default: `false`. |
| `id` | [`ChatId`](/docs/reference/api/types#chatid) | yes |  |
| `mode` | string \| `null` |  |  |
| `model` | string \| `null` |  |  |
| `models` | [`HermesChoice`](/docs/reference/api/types#hermeschoice)[] |  |  |
| `modes` | [`HermesChoice`](/docs/reference/api/types#hermeschoice)[] |  |  |
| `pending` | integer (uint32) |  | The approvals that wait. Default: `0`. |
| `session` | string \| `null` |  | The session id of Hermes, after `session/new`. |
| `started_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `state` | [`HermesChatState`](/docs/reference/api/types#hermeschatstate) | yes |  |
| `time_text` | string | yes | The start as a person reads it, for example "14:12" or "Oct 31". |
| `title` | string | yes | The title that Hermes gave the session, or the folder name before the first prompt. |

## `hermes_chat:<chat>` {#hermes-chat}

**Key:** item id · **Parameter:** chat id

### Value

[`HermesItem`](/docs/reference/api/types#hermesitem)

| Field | Type | Required | Description |
|---|---|---|---|
| `at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `body` | [`HermesItemBody`](/docs/reference/api/types#hermesitembody) | yes |  |
| `id` | string | yes |  |

## Notification `projection.message` {#notification-projection-message}

[`ProjectionNotification`](/docs/reference/api/types#projectionnotification)

| Field | Type | Required | Description |
|---|---|---|---|
| `message` | [`ProjectionMessage`](/docs/reference/api/types#projectionmessage) | yes |  |
| `subscription` | string | yes |  |
