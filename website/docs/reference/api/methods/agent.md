---
title: "agent.*"
description: "Agent runs of Claude Code and Codex: list, start, resume, prompt, interrupt, stop, wait."
sidebar_position: 12
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

Agent runs of Claude Code and Codex: list, start, resume, prompt, interrupt, stop, wait.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`agent.list`](#agent-list) | `ui` `cli` `phone` | [`AgentList`](/docs/reference/api/types#agentlist) | [`AgentListResult`](/docs/reference/api/types#agentlistresult) |
| [`agent.show`](#agent-show) | `ui` `cli` `phone` | [`AgentShow`](/docs/reference/api/types#agentshow) | [`AgentShown`](/docs/reference/api/types#agentshown) |
| [`agent.start`](#agent-start) | `ui` `cli` `phone` | [`AgentStart`](/docs/reference/api/types#agentstart) | [`JobStarted`](/docs/reference/api/types#jobstarted) |
| [`agent.choices`](#agent-choices) | `ui` `cli` `phone` | [`AgentChoices`](/docs/reference/api/types#agentchoices) | [`AgentChoicesResult`](/docs/reference/api/types#agentchoicesresult) |
| [`agent.skills`](#agent-skills) | `ui` `cli` `phone` | [`AgentSkills`](/docs/reference/api/types#agentskills) | [`AgentSkillsResult`](/docs/reference/api/types#agentskillsresult) |
| [`agent.stop`](#agent-stop) | `ui` `cli` `phone` | [`AgentStop`](/docs/reference/api/types#agentstop) | [`Empty`](/docs/reference/api/types#empty) |
| [`agent.interrupt`](#agent-interrupt) | `ui` `cli` `phone` | [`AgentInterrupt`](/docs/reference/api/types#agentinterrupt) | [`Empty`](/docs/reference/api/types#empty) |
| [`agent.prompt`](#agent-prompt) | `ui` `cli` `phone` | [`AgentPrompt`](/docs/reference/api/types#agentprompt) | [`Empty`](/docs/reference/api/types#empty) |
| [`agent.reply`](#agent-reply) | `ui` `cli` `phone` | [`AgentReply`](/docs/reference/api/types#agentreply) | [`Empty`](/docs/reference/api/types#empty) |
| [`agent.resume`](#agent-resume) | `ui` `cli` `phone` | [`AgentResume`](/docs/reference/api/types#agentresume) | [`JobStarted`](/docs/reference/api/types#jobstarted) |
| [`agent.wait`](#agent-wait) | `cli` | [`AgentWait`](/docs/reference/api/types#agentwait) | [`AgentWaitResult`](/docs/reference/api/types#agentwaitresult) |
| [`agent.link`](#agent-link) | `ui` `cli` `phone` | [`AgentLink`](/docs/reference/api/types#agentlink) | [`AgentLinked`](/docs/reference/api/types#agentlinked) |
| [`agent.unlink`](#agent-unlink) | `ui` `cli` `phone` | [`AgentUnlink`](/docs/reference/api/types#agentunlink) | [`Empty`](/docs/reference/api/types#empty) |
| [`agent.move`](#agent-move) | `ui` `cli` `phone` | [`AgentMove`](/docs/reference/api/types#agentmove) | [`Empty`](/docs/reference/api/types#empty) |

## `agent.list` {#agent-list}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh agent list`](/docs/reference/cli/commands/agent#agent-list)

### Params

[`AgentList`](/docs/reference/api/types#agentlist)

| Field | Type | Required | Description |
|---|---|---|---|
| `harness` | [`Harness`](/docs/reference/api/types#harness) \| `null` |  |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) \| `null` |  |  |
| `workspace` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) \| `null` |  |  |

### Result

[`AgentListResult`](/docs/reference/api/types#agentlistresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `agents` | [`AgentValue`](/docs/reference/api/types#agentvalue)[] | yes |  |

## `agent.show` {#agent-show}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh agent show`](/docs/reference/cli/commands/agent#agent-show)

### Params

[`AgentShow`](/docs/reference/api/types#agentshow)

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |

### Result

[`AgentShown`](/docs/reference/api/types#agentshown)

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`AgentValue`](/docs/reference/api/types#agentvalue) | yes |  |

## `agent.start` {#agent-start}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh agent start`](/docs/reference/cli/commands/agent#agent-start)

`agent.start` returns a job. The run appears in the `agents` projection.

### Params

[`AgentStart`](/docs/reference/api/types#agentstart)

| Field | Type | Required | Description |
|---|---|---|---|
| `args` | string[] |  | Extra arguments for the harness. |
| `cwd` | string \| `null` |  |  |
| `effort` | string \| `null` |  | The effort level (`high`). Without it, the harness uses the level of its own settings. A manifest without `effort_arg` refuses it. |
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `model` | string \| `null` |  | The model, as the harness names it (`opus`, `gpt-5.5`). Without it, the harness uses the model of its own settings. |
| `name` | string \| `null` |  | A unique name for the run. A call can use it instead of the id. |
| `placement` | [`Placement`](/docs/reference/api/types#placement) | yes |  |
| `prompt` | string \| `null` |  |  |

### Result

[`JobStarted`](/docs/reference/api/types#jobstarted)

| Field | Type | Required | Description |
|---|---|---|---|
| `job` | [`JobId`](/docs/reference/api/types#jobid) | yes |  |

## `agent.choices` {#agent-choices}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh agent choices`](/docs/reference/cli/commands/agent#agent-choices)

The models and the effort levels that `agent.start` can choose.

### Params

[`AgentChoices`](/docs/reference/api/types#agentchoices)

| Field | Type | Required | Description |
|---|---|---|---|
| `harness` | [`Harness`](/docs/reference/api/types#harness) \| `null` |  |  |

### Result

[`AgentChoicesResult`](/docs/reference/api/types#agentchoicesresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `harnesses` | [`HarnessChoices`](/docs/reference/api/types#harnesschoices)[] | yes |  |

## `agent.skills` {#agent-skills}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh agent skills`](/docs/reference/cli/commands/agent#agent-skills)

The skills and the custom commands that the harness of a run can start in the folder of the pane. With `query`, only the names that have it, ignoring case, and those that start with it first.

### Params

[`AgentSkills`](/docs/reference/api/types#agentskills)

| Field | Type | Required | Description |
|---|---|---|---|
| `query` | string \| `null` |  |  |
| `run` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |

### Result

[`AgentSkillsResult`](/docs/reference/api/types#agentskillsresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `skills` | [`Skill`](/docs/reference/api/types#skill)[] | yes |  |

## `agent.stop` {#agent-stop}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh agent stop`](/docs/reference/cli/commands/agent#agent-stop)

Asks the agent to end. With `force` the core ends the process.

### Params

[`AgentStop`](/docs/reference/api/types#agentstop)

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |
| `force` | boolean |  | Default: `false`. |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `agent.interrupt` {#agent-interrupt}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh agent interrupt`](/docs/reference/cli/commands/agent#agent-interrupt)

### Params

[`AgentInterrupt`](/docs/reference/api/types#agentinterrupt)

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `agent.prompt` {#agent-prompt}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh agent prompt`](/docs/reference/cli/commands/agent#agent-prompt)

Types a prompt into a bound run (never into a suggested one).

### Params

[`AgentPrompt`](/docs/reference/api/types#agentprompt)

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |
| `submit` | boolean |  | Press Enter after the text. Default: `true`. |
| `text` | string | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `agent.reply` {#agent-reply}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh agent reply`](/docs/reference/cli/commands/agent#agent-reply)

The next prompt for a run that is `idle` and has no open interaction. The core writes the text as a bracketed paste and a carriage return, at most once for an `input_id`, and takes no geometry lease.

### Params

[`AgentReply`](/docs/reference/api/types#agentreply)

| Field | Type | Required | Description |
|---|---|---|---|
| `input_id` | integer (uint64) | yes |  |
| `run` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |
| `text` | string | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `agent.resume` {#agent-resume}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh agent resume`](/docs/reference/cli/commands/agent#agent-resume)

Starts the harness again with the resume handle of the run. It returns a job.

### Params

[`AgentResume`](/docs/reference/api/types#agentresume)

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |
| `placement` | [`Placement`](/docs/reference/api/types#placement) \| `null` |  |  |

### Result

[`JobStarted`](/docs/reference/api/types#jobstarted)

| Field | Type | Required | Description |
|---|---|---|---|
| `job` | [`JobId`](/docs/reference/api/types#jobid) | yes |  |

## `agent.wait` {#agent-wait}

**Scopes:** `cli` — the CLI and any other client on the API socket.

**Command line:** [`nh agent wait`](/docs/reference/cli/commands/agent#agent-wait)

### Params

[`AgentWait`](/docs/reference/api/types#agentwait)

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |
| `timeout_ms` | integer (uint64) \| `null` |  |  |
| `until` | [`WaitFor`](/docs/reference/api/types#waitfor) | yes |  |

### Result

[`AgentWaitResult`](/docs/reference/api/types#agentwaitresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`AgentValue`](/docs/reference/api/types#agentvalue) | yes |  |
| `reached` | boolean | yes | `false` when the time ran out. |

## `agent.link` {#agent-link}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh agent link`](/docs/reference/cli/commands/agent#agent-link)

Binds a run to a pane by the user's decision.

### Params

[`AgentLink`](/docs/reference/api/types#agentlink)

| Field | Type | Required | Description |
|---|---|---|---|
| `harness` | [`Harness`](/docs/reference/api/types#harness) \| `null` |  |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `session_id` | string \| `null` |  |  |
| `thread` | string \| `null` |  | The thread of a Codex run on the shared daemon. |

### Result

[`AgentLinked`](/docs/reference/api/types#agentlinked)

| Field | Type | Required | Description |
|---|---|---|---|
| `run` | [`RunId`](/docs/reference/api/types#runid) | yes |  |

## `agent.unlink` {#agent-unlink}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh agent unlink`](/docs/reference/cli/commands/agent#agent-unlink)

### Params

[`AgentUnlink`](/docs/reference/api/types#agentunlink)

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `agent.move` {#agent-move}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh agent move`](/docs/reference/cli/commands/agent#agent-move)

Puts a run at another place on the board of every window. The order stays across a restart of the core, and a new run in the pane of a run that ended takes its place.

### Params

[`AgentMove`](/docs/reference/api/types#agentmove)

| Field | Type | Required | Description |
|---|---|---|---|
| `agent` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |
| `index` | integer (uint32) | yes | The place among the runs, counted from 0. A place after the last run is the end. |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.
