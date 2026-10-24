---
title: "config.*"
description: "The configuration: the merged values with provenance, edits and reload."
sidebar_position: 20
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The configuration: the merged values with provenance, edits and reload.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`config.get`](#config-get) | `ui` `cli` `phone` | [`ConfigGet`](/docs/reference/api/types#configget) | [`ConfigValues`](/docs/reference/api/types#configvalues) |
| [`config.set`](#config-set) | `ui` `cli` `phone` | [`ConfigSet`](/docs/reference/api/types#configset) | [`Empty`](/docs/reference/api/types#empty) |
| [`config.reload`](#config-reload) | `ui` `cli` `phone` | [`ConfigReload`](/docs/reference/api/types#configreload) | [`ConfigReloaded`](/docs/reference/api/types#configreloaded) |

## `config.get` {#config-get}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh config get`](/docs/reference/cli/commands/config#config-get)

### Params

[`ConfigGet`](/docs/reference/api/types#configget)

| Field | Type | Required | Description |
|---|---|---|---|
| `section` | string \| `null` |  | One section, for example `approvals`. Without it all sections. |

### Result

[`ConfigValues`](/docs/reference/api/types#configvalues)

| Field | Type | Required | Description |
|---|---|---|---|
| `sections` | map of string to [`SettingsSection`](/docs/reference/api/types#settingssection) | yes |  |

## `config.set` {#config-set}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh config set`](/docs/reference/cli/commands/config#config-set)

Sets a value in `config.toml`. The file keeps its comments and its order. A `null` removes the key, so the default applies again.

### Params

[`ConfigSet`](/docs/reference/api/types#configset)

| Field | Type | Required | Description |
|---|---|---|---|
| `key` | string | yes | The path, for example `approvals.mode`. |
| `value` | any | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `config.reload` {#config-reload}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh config reload`](/docs/reference/cli/commands/config#config-reload)

### Params

[`ConfigReload`](/docs/reference/api/types#configreload)

An object with no fields: `{}`.

### Result

[`ConfigReloaded`](/docs/reference/api/types#configreloaded)

| Field | Type | Required | Description |
|---|---|---|---|
| `errors` | string[] | yes | Errors of the files. The old values stay in force. |
