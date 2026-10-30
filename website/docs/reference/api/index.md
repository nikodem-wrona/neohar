---
title: "API contract"
description: "The JSON-RPC contract that the UI, the CLI and the hook shim share with the core."
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The UI, the CLI and the hook shim all call the core through one contract. Its version is `neohar/1`. The contract is declared once in Rust, in [`crates/nh-proto/src/api`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-proto/src/api); `nh api schema` prints it as JSON Schema, a snapshot test keeps it stable, and TypeScript types are generated from it. Read [API concepts](/docs/reference/api/concepts) first for the envelope, scopes, subscriptions and errors.

| Part | Count | Reference |
|---|---|---|
| Methods | 175 | [Methods](/docs/reference/api/methods) |
| Projections | 30 | [Projections](/docs/reference/api/projections) |
| Event kinds | 59 | [Events](/docs/reference/api/events) |
| Streams | 4 | [Streams](/docs/reference/api/streams) |
| Error codes | 43 | [Errors](/docs/reference/api/errors) |
| Named types | 475 | [Types](/docs/reference/api/types) |

## Protocol versions

| Protocol | Version |
|---|---|
| API | `neohar/1` |
| frame | 1 |
| holder | 2 |

## Envelope

Every message is one JSON-RPC 2.0 object.

### call error

[`CallError`](/docs/reference/api/types#callerror)

| Field | Type | Required | Description |
|---|---|---|---|
| `code` | [`ErrorCode`](/docs/reference/api/types#errorcode) | yes |  |
| `details` | any |  |  |
| `message` | string | yes |  |

### notification

[`Notification`](/docs/reference/api/types#notification)

| Field | Type | Required | Description |
|---|---|---|---|
| `jsonrpc` | [`JsonRpc`](/docs/reference/api/types#jsonrpc) | yes |  |
| `method` | string | yes |  |
| `params` | any |  | Default: `null`. |

### request

[`Request`](/docs/reference/api/types#request)

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | [`RequestId`](/docs/reference/api/types#requestid) \| `null` |  |  |
| `jsonrpc` | [`JsonRpc`](/docs/reference/api/types#jsonrpc) | yes |  |
| `method` | string | yes |  |
| `params` | any |  | Default: `null`. |

### response

[`Response`](/docs/reference/api/types#response)

| Field | Type | Required | Description |
|---|---|---|---|
| `error` | [`RpcError`](/docs/reference/api/types#rpcerror) \| `null` |  |  |
| `id` | [`RequestId`](/docs/reference/api/types#requestid) \| `null` |  |  |
| `jsonrpc` | [`JsonRpc`](/docs/reference/api/types#jsonrpc) | yes |  |
| `result` | any |  |  |

## Scopes

| Scope | Who | Methods |
|---|---|---|
| `ui` | the UI, over the `rpc` command of the Tauri host or the WebSocket | 156 |
| `cli` | the CLI and any other client on the API socket | 114 |
| `pane` | a hook shim, with the token of a pane | 4 |
| `dev` | development and test builds only | 4 |
| `phone` | a paired phone, over the listener of the phone | 152 |

## Namespaces

| Namespace | What it is for | Methods |
|---|---|---|
| [`session`](/docs/reference/api/methods/session) | The session of the core: its name, version and sockets. | 3 |
| [`machine`](/docs/reference/api/methods/machine) | The machine that the core runs on. | 5 |
| [`workspace`](/docs/reference/api/methods/workspace) | Workspaces: groups of tabs, usually bound to a project root. | 6 |
| [`group`](/docs/reference/api/methods/group) | Groups of workspaces in the sidebar. | 6 |
| [`tab`](/docs/reference/api/methods/tab) | Tabs: one layout of panes in a workspace. | 5 |
| [`layout`](/docs/reference/api/methods/layout) | The split tree of a tab: ratios, equal sizes and zoom. | 3 |
| [`pane`](/docs/reference/api/methods/pane) | Panes: terminals with a holder, and their input, scrolling, selection, copy and text. | 27 |
| [`window`](/docs/reference/api/methods/window) | The windows of the app: which panes they show, focus, metrics, bounds and the single view. | 7 |
| [`fs`](/docs/reference/api/methods/fs) | The files of the machine for the Files tab: list a folder, read a preview, open a path and copy a path. | 4 |
| [`askpass`](/docs/reference/api/methods/askpass) | The questions of ssh that `nh askpass` sends. The app asks the person. | 1 |
| [`render`](/docs/reference/api/methods/render) | Acknowledgements of painted frames, which free the slots of the render pacer. | 1 |
| [`agent`](/docs/reference/api/methods/agent) | Agent runs of Claude Code and Codex: list, start, resume, prompt, interrupt, stop, wait. | 13 |
| [`harness`](/docs/reference/api/methods/harness) | Claude Code and Codex on this machine: discovery, version and login state. | 1 |
| [`interaction`](/docs/reference/api/methods/interaction) | What agents ask a person: approvals, questions, plan reviews and notices. | 6 |
| [`changes`](/docs/reference/api/methods/changes) | The Changes tab: the changes of a git worktree from HEAD, and the diff of one file. | 2 |
| [`git`](/docs/reference/api/methods/git) | The Git view: repositories and their trust, worktrees, branches, changes and history, and switch, fetch and pull. | 15 |
| [`phone`](/docs/reference/api/methods/phone) | The phones that supervise this machine: the offer of a pairing, names and removal, and the mapping of tailscale serve. | 11 |
| [`workflow`](/docs/reference/api/methods/workflow) | The Workflows view: the workflow files and their graphs, and the executions in a folder with the report of each node. | 23 |
| [`events`](/docs/reference/api/methods/events) | Subscriptions to the event log, from a cursor. | 2 |
| [`projection`](/docs/reference/api/methods/projection) | Subscriptions to projections: a snapshot, then keyed patches. | 3 |
| [`config`](/docs/reference/api/methods/config) | The configuration: the merged values with provenance, edits and reload. | 3 |
| [`policy`](/docs/reference/api/methods/policy) | Rules that decide approvals before a person is asked. | 4 |
| [`integration`](/docs/reference/api/methods/integration) | The hooks that neohar installs into Claude Code and Codex. | 6 |
| [`doctor`](/docs/reference/api/methods/doctor) | The checks of `nh doctor`. | 1 |
| [`debug`](/docs/reference/api/methods/debug) | Bug reports and tools for development builds. | 5 |
| [`palette`](/docs/reference/api/methods/palette) | The command palette: the core ranks and runs the commands. | 2 |
| [`rich`](/docs/reference/api/methods/rich) | Rich content on demand: diffs, sanitized markdown and highlighted code. | 3 |
| [`timeline`](/docs/reference/api/methods/timeline) | Older items of the timeline of a run. | 2 |
| [`jobs`](/docs/reference/api/methods/jobs) | Long commands that run as jobs. | 1 |
| [`adapter`](/docs/reference/api/methods/adapter) | The calls of the hook shim. Only a caller with the token of a pane may make them. | 4 |
