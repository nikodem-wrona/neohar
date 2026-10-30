---
title: "debug.*"
description: "Bug reports and tools for development builds."
sidebar_position: 25
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

Bug reports and tools for development builds.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`debug.bundle`](#debug-bundle) | `cli` | [`DebugBundle`](/docs/reference/api/types#debugbundle) | [`DebugBundled`](/docs/reference/api/types#debugbundled) |
| [`debug.screen`](#debug-screen) | `dev` | [`DebugScreen`](/docs/reference/api/types#debugscreen) | [`DebugScreenResult`](/docs/reference/api/types#debugscreenresult) |
| [`debug.answer`](#debug-answer) | `dev` | [`DebugAnswer`](/docs/reference/api/types#debuganswer) | [`DebugAnswered`](/docs/reference/api/types#debuganswered) |
| [`debug.spawn_panes`](#debug-spawn-panes) | `dev` | [`DebugSpawnPanes`](/docs/reference/api/types#debugspawnpanes) | [`DebugSpawned`](/docs/reference/api/types#debugspawned) |
| [`debug.drop_links`](#debug-drop-links) | `dev` | [`Empty`](/docs/reference/api/types#empty) | [`Empty`](/docs/reference/api/types#empty) |

## `debug.bundle` {#debug-bundle}

**Scopes:** `cli` — the CLI and any other client on the API socket.

**Command line:** [`nh debug bundle`](/docs/reference/cli/commands/debug#debug-bundle)

`debug.bundle`: writes a zip for a bug report. Everything in it has passed the redaction.

### Params

[`DebugBundle`](/docs/reference/api/types#debugbundle)

| Field | Type | Required | Description |
|---|---|---|---|
| `include_scrollback` | boolean |  | Add the text of the panes. Without it the bundle has no scrollback. Default: `false`. |
| `out` | string \| `null` |  | The file to write. The CLI always gives one. Without it the core writes `neohar-debug-<time>.zip` in the directory of the logs. |

### Result

[`DebugBundled`](/docs/reference/api/types#debugbundled)

| Field | Type | Required | Description |
|---|---|---|---|
| `bytes` | integer (uint64) | yes |  |
| `files` | integer (uint32) | yes |  |
| `path` | string | yes |  |

## `debug.screen` {#debug-screen}

**Scopes:** `dev` — development and test builds only.

`debug.screen`: what a pane shows, with the journal offset it belongs to. The pane can be one that `debug.spawn_panes` started, outside the layout.

### Params

[`DebugScreen`](/docs/reference/api/types#debugscreen)

| Field | Type | Required | Description |
|---|---|---|---|
| `dump` | boolean |  | Include the cells (`dump`). A call without them is cheap. Default: `true`. |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |

### Result

[`DebugScreenResult`](/docs/reference/api/types#debugscreenresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `asked_size` | any[] | yes | The size that the last resize request asked for, as columns and rows. |
| `dump` | any |  | The cells, the cursor, the modes and the title (a `ScreenDump` of `nh-term`). Only with `dump`. |
| `fed_offset` | integer (uint64) | yes | The journal offset that the engine read up to. With `dump`, the offset of the dump. |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `recovery` | string | yes | How the pane got its screen: `fresh`, `snapshot`, `journal` or `ring_only`. |
| `restored` | [`DebugRestoredScreen`](/docs/reference/api/types#debugrestoredscreen) \| `null` |  | The screen at the end of the replay. Only with `dump`, and only for a pane that a restart restored. |
| `restoring` | boolean | yes | The pane reads the journal after a restart and shows no final screen yet. |
| `settled` | boolean | yes | The pane has read its journal and the resize nudge is over. A pane that was not restored is settled from the start. |
| `since_resize_ms` | integer (uint64) | yes | How long ago the engine last changed its size. |
| `size` | any[] | yes | The size of the engine, as columns and rows. |

## `debug.answer` {#debug-answer}

**Scopes:** `dev` — development and test builds only.

`debug.answer`: decides an interaction the way a client would, and may hold the delivery back so that a test can control the order of events.

### Params

[`DebugAnswer`](/docs/reference/api/types#debuganswer)

| Field | Type | Required | Description |
|---|---|---|---|
| `by` | string |  | Default: `"debug"`. |
| `decision` | [`Decision`](/docs/reference/api/types#decision) | yes |  |
| `hold` | boolean |  | Record the decision and do not deliver it. A later `debug.deliver` does. Default: `false`. |
| `interaction` | [`RefInteractionId`](/docs/reference/api/types#refinteractionid) | yes |  |
| `via` | [`Via`](/docs/reference/api/types#via) | yes |  |

### Result

[`DebugAnswered`](/docs/reference/api/types#debuganswered)

| Field | Type | Required | Description |
|---|---|---|---|
| `rev` | integer (uint32) | yes |  |
| `state` | [`InteractionState`](/docs/reference/api/types#interactionstate) | yes |  |

## `debug.spawn_panes` {#debug-spawn-panes}

**Scopes:** `dev` — development and test builds only.

`debug.spawn_panes`: panes for a test, outside the layout. No window draws them. The panes start one after the other and run the same program.

### Params

[`DebugSpawnPanes`](/docs/reference/api/types#debugspawnpanes)

| Field | Type | Required | Description |
|---|---|---|---|
| `argv` | string[] \| `null` |  | The program and its arguments. Without it, the `SHELL` of the core, else `/bin/sh`. |
| `cols` | integer (uint16) | yes |  |
| `count` | integer (uint32) |  | How many panes, from 1 to 64. Default: `1`. |
| `cwd` | string \| `null` |  |  |
| `env` | any[][] |  | Extra variables for the environment, as pairs of a name and a value. |
| `isolated_home` | boolean |  | Give each pane its own `HOME`, `CLAUDE_CONFIG_DIR` and `CODEX_HOME` in the data directory of the session, so that a harness never touches the real ones. Without `cwd`, the pane starts in that `HOME`. Default: `false`. |
| `rows` | integer (uint16) | yes |  |

### Result

[`DebugSpawned`](/docs/reference/api/types#debugspawned)

| Field | Type | Required | Description |
|---|---|---|---|
| `panes` | [`DebugSpawnedPane`](/docs/reference/api/types#debugspawnedpane)[] | yes |  |

## `debug.drop_links` {#debug-drop-links}

**Scopes:** `dev` — development and test builds only.

An empty result or empty params: `{}`.

### Params

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## Methods outside the contract {#outside-the-contract}

A build with the cargo feature `debug` of `nh-core` also serves these methods. They are not in `nh api schema`, so they have no generated types; the table comes from the doc comment of [`crates/nh-core/src/debug_calls.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/debug_calls.rs). The tests, `nh-chaos` and `nh-golden` use them.

| Method | Params | Result |
|---|---|---|
| `debug.emit_events` | `{count, kind?, subject?, batch?}` | `{emitted}`; appends `count` events of the kind `debug.tick` (default), `batch` (default 100) in each transaction, and sends them to the subscribers of `events.subscribe` |
| `debug.pane_inputs` | `{pane, since?}` | `{acked, next, pending, unconfirmed}`; the ids that the holder acknowledged since the number `since` |
| `debug.restore_report` | `{}` | what the restart of this core did, pane by pane |
| `debug.interactions` | `{}` | `{interactions}`; the interactions of the hook gate, with their state, decision and attempts |
| `debug.deliver` | `{interaction}` | `{interaction, state}`; delivers a decision that `hold` kept back |
| `debug.release` | `{interaction, reason?}` | `{interaction, mode}`; the hook prints nothing, and the harness shows its dialog |
