---
title: "palette.*"
description: "The command palette: the core ranks and runs the commands."
sidebar_position: 25
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The command palette: the core ranks and runs the commands.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`palette.query`](#palette-query) | `ui` `phone` | [`PaletteQuery`](/docs/reference/api/types#palettequery) | [`PaletteResults`](/docs/reference/api/types#paletteresults) |
| [`palette.execute`](#palette-execute) | `ui` `phone` | [`PaletteExecute`](/docs/reference/api/types#paletteexecute) | [`PaletteExecuted`](/docs/reference/api/types#paletteexecuted) |

## `palette.query` {#palette-query}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

### Params

[`PaletteQuery`](/docs/reference/api/types#palettequery)

| Field | Type | Required | Description |
|---|---|---|---|
| `limit` | integer (uint32) \| `null` |  |  |
| `query` | string | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  | The window that asks. A later version ranks the commands of that window first. |

### Result

[`PaletteResults`](/docs/reference/api/types#paletteresults)

| Field | Type | Required | Description |
|---|---|---|---|
| `items` | [`PaletteMatch`](/docs/reference/api/types#palettematch)[] | yes |  |

## `palette.execute` {#palette-execute}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

### Params

[`PaletteExecute`](/docs/reference/api/types#paletteexecute)

| Field | Type | Required | Description |
|---|---|---|---|
| `args` | any |  |  |
| `command` | string | yes | The id of the command (`PaletteCommand.id`). |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  | The window that runs the command. A command on the tab or the pane in focus needs it. |

### Result

[`PaletteExecuted`](/docs/reference/api/types#paletteexecuted)

| Field | Type | Required | Description |
|---|---|---|---|
| `ui` | string \| `null` |  |  |
