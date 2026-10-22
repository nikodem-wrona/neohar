---
title: "Errors"
description: "The 46 error codes of the API and their JSON-RPC numbers."
sidebar_position: 6
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The string code of an error. The codes are part of the contract, and this list is complete: the core, the hosts and the link send no other code. To send a new code, add it here first. The UI and the CLI act on a few of them (`already_answered`, `confirm_required`, `not_found`, `unsupported`). On the wire a code is its `snake_case` name.

A protocol error has the reserved JSON-RPC number. An application error has `-32000`, and `error.data` is `{code, details}`, where `code` is one of the strings below. The UI shows `message` and acts on `code` only where the contract says so. The shape of an error as a caller sees it is [`CallError`](/docs/reference/api/types#callerror); on the wire it is [`RpcError`](/docs/reference/api/types#rpcerror).

| Code | JSON-RPC | Meaning |
|---|---|---|
| [`parse_error`](#parse-error) | -32700 | The line is not JSON (-32700). |
| [`invalid_request`](#invalid-request) | -32600 | The JSON is not a request (-32600). |
| [`method_not_found`](#method-not-found) | -32601 | The method does not exist (-32601). |
| [`invalid_params`](#invalid-params) | -32602 | The params do not fit the method (-32602). |
| [`not_found`](#not-found) | -32000 | The thing that the call names does not exist. |
| [`pane_gone`](#pane-gone) | -32000 | The pane exists and has no process: its holder starts, the program ended before the core restarted, or the holder is lost. `pane.restart` starts the program again. |
| [`permission_denied`](#permission-denied) | -32000 | The caller may not call this method, or not on this thing. |
| [`already_answered`](#already-answered) | -32000 | Another client answered the interaction first. `details` has `{by, via, decision}`. |
| [`input_unconfirmed`](#input-unconfirmed) | -32000 | An input was in flight when the holder or the core went away. `details` has `{input_id, reason}`. |
| [`delivery_unknown`](#delivery-unknown) | -32000 | The core cannot say whether an answer reached the harness. |
| [`degraded_storage`](#degraded-storage) | -32000 | The store is read-only or full. The core runs, but it does not keep changes. |
| [`confirm_required`](#confirm-required) | -32000 | The call needs a second call with `force`. `details` says what the user would lose. |
| [`stale_version`](#stale-version) | -32000 | The caller worked with an older version of the thing. Read it again. |
| [`busy`](#busy) | -32000 | The core is busy with the same work. Try again. |
| [`harness_unavailable`](#harness-unavailable) | -32000 | The harness of a start is not installed, or it is logged out after a new probe. `details` has `{harness, reason, hint}`, and `reason` is `not_installed` or `logged_out`. |
| [`unsupported`](#unsupported) | -32000 | The method exists in the contract, and the core has no handler for it yet. |
| [`link_down`](#link-down) | -32000 | The link to the core of a machine is down. A call fails with it at once, and a call in flight when the link went down fails with it too: its outcome is unknown. |
| [`version_mismatch`](#version-mismatch) | -32000 | The core of a machine has another major version of the API. |
| [`not_idle`](#not-idle) | -32000 | `agent.reply` on a run that works or has an open interaction. `details` has `{execution, pending}`. |
| [`pane_busy`](#pane-busy) | -32000 | The pane runs a program or an agent, and the call needs an idle shell. |
| [`pane_spawn_failed`](#pane-spawn-failed) | -32000 | The core cannot start the holder or the program of a pane. |
| [`holder_error`](#holder-error) | -32000 | The holder of a pane failed or sent something that the core does not accept. |
| [`agent_live`](#agent-live) | -32000 | The agent still runs, and the call needs an agent that has ended. |
| [`agent_gone`](#agent-gone) | -32000 | The agent has ended, and the call needs a live agent. |
| [`agent_failed`](#agent-failed) | -32000 | The task exists, but its agent did not start. `details` has `{task}`. |
| [`not_bound`](#not-bound) | -32000 | The run is not linked to its pane, so the core types nothing into it. |
| [`name_taken`](#name-taken) | -32000 | A live agent or a task has the name already. |
| [`harness_mismatch`](#harness-mismatch) | -32000 | The run in the pane is of another harness than the call names. |
| [`manifest_error`](#manifest-error) | -32000 | The manifest of a start has a value that is not correct. |
| [`interaction_closed`](#interaction-closed) | -32000 | The interaction has ended. `details` has `{state}`. |
| [`not_answerable`](#not-answerable) | -32000 | No channel reaches the harness of the interaction, so nothing can answer it. |
| [`gate_unavailable`](#gate-unavailable) | -32000 | This core does not hold the gates of the hooks. |
| [`cancelled`](#cancelled) | -32000 | The job of the call was cancelled. |
| [`timeout`](#timeout) | -32000 | The work of the call did not end in time. |
| [`too_large`](#too-large) | -32000 | The request, the file or the blob is larger than the limit. |
| [`blob_corrupt`](#blob-corrupt) | -32000 | The bytes of a blob do not agree with its hash. |
| [`conflict`](#conflict) | -32000 | A policy rule with the same id exists. |
| [`config_rule`](#config-rule) | -32000 | The policy rule is in `config.toml`, so only a change of that file removes it. |
| [`git_failed`](#git-failed) | -32000 | A command of git failed. |
| [`setup_failed`](#setup-failed) | -32000 | The setup command of a task failed. |
| [`no_free_ports`](#no-free-ports) | -32000 | The port range of a task does not have sufficient free ports. |
| [`forward_failed`](#forward-failed) | -32000 | The core cannot forward the port of a machine. |
| [`proto_unsupported`](#proto-unsupported) | -32000 | The client decodes another version of the frame format. |
| [`io_error`](#io-error) | -32000 | The core cannot write a file. |
| [`store_error`](#store-error) | -32000 | The database failed. |
| [`internal`](#internal) | -32603 | A fault of the core. The log has the reason. |

## `parse_error` {#parse-error}

**JSON-RPC number:** `-32700`

The line is not JSON (-32700).

## `invalid_request` {#invalid-request}

**JSON-RPC number:** `-32600`

The JSON is not a request (-32600).

## `method_not_found` {#method-not-found}

**JSON-RPC number:** `-32601`

The method does not exist (-32601).

## `invalid_params` {#invalid-params}

**JSON-RPC number:** `-32602`

The params do not fit the method (-32602).

## `not_found` {#not-found}

**JSON-RPC number:** `-32000`

The thing that the call names does not exist.

## `pane_gone` {#pane-gone}

**JSON-RPC number:** `-32000`

The pane exists and has no process: its holder starts, the program ended before the core restarted, or the holder is lost. `pane.restart` starts the program again.

## `permission_denied` {#permission-denied}

**JSON-RPC number:** `-32000`

The caller may not call this method, or not on this thing.

## `already_answered` {#already-answered}

**JSON-RPC number:** `-32000`

Another client answered the interaction first. `details` has `{by, via, decision}`.

## `input_unconfirmed` {#input-unconfirmed}

**JSON-RPC number:** `-32000`

An input was in flight when the holder or the core went away. `details` has `{input_id, reason}`.

## `delivery_unknown` {#delivery-unknown}

**JSON-RPC number:** `-32000`

The core cannot say whether an answer reached the harness.

## `degraded_storage` {#degraded-storage}

**JSON-RPC number:** `-32000`

The store is read-only or full. The core runs, but it does not keep changes.

## `confirm_required` {#confirm-required}

**JSON-RPC number:** `-32000`

The call needs a second call with `force`. `details` says what the user would lose.

## `stale_version` {#stale-version}

**JSON-RPC number:** `-32000`

The caller worked with an older version of the thing. Read it again.

## `busy` {#busy}

**JSON-RPC number:** `-32000`

The core is busy with the same work. Try again.

## `harness_unavailable` {#harness-unavailable}

**JSON-RPC number:** `-32000`

The harness of a start is not installed, or it is logged out after a new probe. `details` has `{harness, reason, hint}`, and `reason` is `not_installed` or `logged_out`.

## `unsupported` {#unsupported}

**JSON-RPC number:** `-32000`

The method exists in the contract, and the core has no handler for it yet.

## `link_down` {#link-down}

**JSON-RPC number:** `-32000`

The link to the core of a machine is down. A call fails with it at once, and a call in flight when the link went down fails with it too: its outcome is unknown.

## `version_mismatch` {#version-mismatch}

**JSON-RPC number:** `-32000`

The core of a machine has another major version of the API.

## `not_idle` {#not-idle}

**JSON-RPC number:** `-32000`

`agent.reply` on a run that works or has an open interaction. `details` has `{execution, pending}`.

## `pane_busy` {#pane-busy}

**JSON-RPC number:** `-32000`

The pane runs a program or an agent, and the call needs an idle shell.

## `pane_spawn_failed` {#pane-spawn-failed}

**JSON-RPC number:** `-32000`

The core cannot start the holder or the program of a pane.

## `holder_error` {#holder-error}

**JSON-RPC number:** `-32000`

The holder of a pane failed or sent something that the core does not accept.

## `agent_live` {#agent-live}

**JSON-RPC number:** `-32000`

The agent still runs, and the call needs an agent that has ended.

## `agent_gone` {#agent-gone}

**JSON-RPC number:** `-32000`

The agent has ended, and the call needs a live agent.

## `agent_failed` {#agent-failed}

**JSON-RPC number:** `-32000`

The task exists, but its agent did not start. `details` has `{task}`.

## `not_bound` {#not-bound}

**JSON-RPC number:** `-32000`

The run is not linked to its pane, so the core types nothing into it.

## `name_taken` {#name-taken}

**JSON-RPC number:** `-32000`

A live agent or a task has the name already.

## `harness_mismatch` {#harness-mismatch}

**JSON-RPC number:** `-32000`

The run in the pane is of another harness than the call names.

## `manifest_error` {#manifest-error}

**JSON-RPC number:** `-32000`

The manifest of a start has a value that is not correct.

## `interaction_closed` {#interaction-closed}

**JSON-RPC number:** `-32000`

The interaction has ended. `details` has `{state}`.

## `not_answerable` {#not-answerable}

**JSON-RPC number:** `-32000`

No channel reaches the harness of the interaction, so nothing can answer it.

## `gate_unavailable` {#gate-unavailable}

**JSON-RPC number:** `-32000`

This core does not hold the gates of the hooks.

## `cancelled` {#cancelled}

**JSON-RPC number:** `-32000`

The job of the call was cancelled.

## `timeout` {#timeout}

**JSON-RPC number:** `-32000`

The work of the call did not end in time.

## `too_large` {#too-large}

**JSON-RPC number:** `-32000`

The request, the file or the blob is larger than the limit.

## `blob_corrupt` {#blob-corrupt}

**JSON-RPC number:** `-32000`

The bytes of a blob do not agree with its hash.

## `conflict` {#conflict}

**JSON-RPC number:** `-32000`

A policy rule with the same id exists.

## `config_rule` {#config-rule}

**JSON-RPC number:** `-32000`

The policy rule is in `config.toml`, so only a change of that file removes it.

## `git_failed` {#git-failed}

**JSON-RPC number:** `-32000`

A command of git failed.

## `setup_failed` {#setup-failed}

**JSON-RPC number:** `-32000`

The setup command of a task failed.

## `no_free_ports` {#no-free-ports}

**JSON-RPC number:** `-32000`

The port range of a task does not have sufficient free ports.

## `forward_failed` {#forward-failed}

**JSON-RPC number:** `-32000`

The core cannot forward the port of a machine.

## `proto_unsupported` {#proto-unsupported}

**JSON-RPC number:** `-32000`

The client decodes another version of the frame format.

## `io_error` {#io-error}

**JSON-RPC number:** `-32000`

The core cannot write a file.

## `store_error` {#store-error}

**JSON-RPC number:** `-32000`

The database failed.

## `internal` {#internal}

**JSON-RPC number:** `-32603`

A fault of the core. The log has the reason.
