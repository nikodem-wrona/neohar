---
title: "machine.*"
description: "The machine that the core runs on."
sidebar_position: 2
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The machine that the core runs on.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`machine.info`](#machine-info) | `ui` `cli` `phone` | [`Empty`](/docs/reference/api/types#empty) | [`MachineInfo`](/docs/reference/api/types#machineinfo) |
| [`machine.list`](#machine-list) | `ui` `cli` `phone` | [`Empty`](/docs/reference/api/types#empty) | [`MachineList`](/docs/reference/api/types#machinelist) |
| [`machine.connect`](#machine-connect) | `ui` `cli` `phone` | [`MachineTarget`](/docs/reference/api/types#machinetarget) | [`Empty`](/docs/reference/api/types#empty) |
| [`machine.disconnect`](#machine-disconnect) | `ui` `cli` `phone` | [`MachineTarget`](/docs/reference/api/types#machinetarget) | [`Empty`](/docs/reference/api/types#empty) |
| [`machine.open_window`](#machine-open-window) | `ui` `cli` | [`MachineTarget`](/docs/reference/api/types#machinetarget) | [`Empty`](/docs/reference/api/types#empty) |

## `machine.info` {#machine-info}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh machine info`](/docs/reference/cli/commands/machine#machine-info)

An empty result or empty params: `{}`.

### Params

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

### Result

[`MachineInfo`](/docs/reference/api/types#machineinfo)

| Field | Type | Required | Description |
|---|---|---|---|
| `arch` | string | yes |  |
| `hostname` | string \| `null` |  |  |
| `label` | string | yes | The label of the machine. The local machine is `local`. |
| `os` | string | yes |  |
| `uuid` | string | yes |  |

## `machine.list` {#machine-list}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh machine list`](/docs/reference/cli/commands/machine#machine-list)

An empty result or empty params: `{}`.

### Params

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

### Result

[`MachineList`](/docs/reference/api/types#machinelist)

| Field | Type | Required | Description |
|---|---|---|---|
| `machines` | [`MachineValue`](/docs/reference/api/types#machinevalue)[] | yes |  |

## `machine.connect` {#machine-connect}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh machine connect`](/docs/reference/cli/commands/machine#machine-connect)

`machine.connect`, `machine.disconnect` and `machine.open_window`: the label of a machine of `config.toml`.

### Params

[`MachineTarget`](/docs/reference/api/types#machinetarget)

| Field | Type | Required | Description |
|---|---|---|---|
| `machine` | string | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `machine.disconnect` {#machine-disconnect}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh machine disconnect`](/docs/reference/cli/commands/machine#machine-disconnect)

`machine.connect`, `machine.disconnect` and `machine.open_window`: the label of a machine of `config.toml`.

### Params

[`MachineTarget`](/docs/reference/api/types#machinetarget)

| Field | Type | Required | Description |
|---|---|---|---|
| `machine` | string | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `machine.open_window` {#machine-open-window}

**Scopes:** `ui` `cli` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket.

**Command line:** [`nh machine open-window`](/docs/reference/cli/commands/machine#machine-open-window)

`machine.connect`, `machine.disconnect` and `machine.open_window`: the label of a machine of `config.toml`.

### Params

[`MachineTarget`](/docs/reference/api/types#machinetarget)

| Field | Type | Required | Description |
|---|---|---|---|
| `machine` | string | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.
