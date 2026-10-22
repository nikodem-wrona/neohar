---
title: "integration.*"
description: "The hooks that neohar installs into Claude Code and Codex."
sidebar_position: 24
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The hooks that neohar installs into Claude Code and Codex.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`integration.list`](#integration-list) | `ui` `cli` `phone` | [`IntegrationList`](/docs/reference/api/types#integrationlist) | [`IntegrationListResult`](/docs/reference/api/types#integrationlistresult) |
| [`integration.install`](#integration-install) | `ui` `cli` `phone` | [`IntegrationInstall`](/docs/reference/api/types#integrationinstall) | [`IntegrationChanges`](/docs/reference/api/types#integrationchanges) |
| [`integration.status`](#integration-status) | `ui` `cli` `phone` | [`IntegrationStatusParams`](/docs/reference/api/types#integrationstatusparams) | [`IntegrationStatusResult`](/docs/reference/api/types#integrationstatusresult) |
| [`integration.uninstall`](#integration-uninstall) | `ui` `cli` `phone` | [`IntegrationUninstall`](/docs/reference/api/types#integrationuninstall) | [`IntegrationChanges`](/docs/reference/api/types#integrationchanges) |
| [`integration.doctor`](#integration-doctor) | `ui` `cli` `phone` | [`IntegrationDoctor`](/docs/reference/api/types#integrationdoctor) | [`JobStarted`](/docs/reference/api/types#jobstarted) |
| [`integration.capabilities`](#integration-capabilities) | `ui` `cli` `phone` | [`IntegrationCapabilitiesParams`](/docs/reference/api/types#integrationcapabilitiesparams) | [`IntegrationCapabilitiesResult`](/docs/reference/api/types#integrationcapabilitiesresult) |

## `integration.list` {#integration-list}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh integration list`](/docs/reference/cli/commands/integration#integration-list)

### Params

[`IntegrationList`](/docs/reference/api/types#integrationlist)

An object with no fields: `{}`.

### Result

[`IntegrationListResult`](/docs/reference/api/types#integrationlistresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `integrations` | [`IntegrationInfo`](/docs/reference/api/types#integrationinfo)[] | yes |  |

## `integration.install` {#integration-install}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh integration install`](/docs/reference/cli/commands/integration#integration-install)

### Params

[`IntegrationInstall`](/docs/reference/api/types#integrationinstall)

| Field | Type | Required | Description |
|---|---|---|---|
| `dry_run` | boolean |  | Default: `false`. |
| `project_root` | string \| `null` |  | The repository for the scope `project`. The CLI gives its working directory. |
| `scope` | [`IntegrationScope`](/docs/reference/api/types#integrationscope) |  | Default: `"user"`. |
| `target` | [`IntegrationTarget`](/docs/reference/api/types#integrationtarget) | yes |  |

### Result

[`IntegrationChanges`](/docs/reference/api/types#integrationchanges)

| Field | Type | Required | Description |
|---|---|---|---|
| `changes` | [`FileChange`](/docs/reference/api/types#filechange)[] | yes |  |
| `notes` | string[] | yes | What the user must do, for example `/hooks` in Codex. |

## `integration.status` {#integration-status}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh integration status`](/docs/reference/cli/commands/integration#integration-status)

### Params

[`IntegrationStatusParams`](/docs/reference/api/types#integrationstatusparams)

| Field | Type | Required | Description |
|---|---|---|---|
| `target` | [`IntegrationTarget`](/docs/reference/api/types#integrationtarget) \| `null` |  |  |

### Result

[`IntegrationStatusResult`](/docs/reference/api/types#integrationstatusresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `status` | [`IntegrationStatusEntry`](/docs/reference/api/types#integrationstatusentry)[] | yes |  |

## `integration.uninstall` {#integration-uninstall}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh integration uninstall`](/docs/reference/cli/commands/integration#integration-uninstall)

### Params

[`IntegrationUninstall`](/docs/reference/api/types#integrationuninstall)

| Field | Type | Required | Description |
|---|---|---|---|
| `project_root` | string \| `null` |  | The repository for the scope `project`. The CLI gives its working directory. |
| `scope` | [`IntegrationScope`](/docs/reference/api/types#integrationscope) |  | Default: `"user"`. |
| `target` | [`IntegrationTarget`](/docs/reference/api/types#integrationtarget) | yes |  |

### Result

[`IntegrationChanges`](/docs/reference/api/types#integrationchanges)

| Field | Type | Required | Description |
|---|---|---|---|
| `changes` | [`FileChange`](/docs/reference/api/types#filechange)[] | yes |  |
| `notes` | string[] | yes | What the user must do, for example `/hooks` in Codex. |

## `integration.doctor` {#integration-doctor}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh integration doctor`](/docs/reference/cli/commands/integration#integration-doctor)

Runs the round trip of `nh integration doctor`. It returns a job.

### Params

[`IntegrationDoctor`](/docs/reference/api/types#integrationdoctor)

| Field | Type | Required | Description |
|---|---|---|---|
| `target` | [`IntegrationTarget`](/docs/reference/api/types#integrationtarget) \| `null` |  |  |

### Result

[`JobStarted`](/docs/reference/api/types#jobstarted)

| Field | Type | Required | Description |
|---|---|---|---|
| `job` | [`JobId`](/docs/reference/api/types#jobid) | yes |  |

## `integration.capabilities` {#integration-capabilities}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh integration capabilities`](/docs/reference/cli/commands/integration#integration-capabilities)

### Params

[`IntegrationCapabilitiesParams`](/docs/reference/api/types#integrationcapabilitiesparams)

An object with no fields: `{}`.

### Result

[`IntegrationCapabilitiesResult`](/docs/reference/api/types#integrationcapabilitiesresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `rows` | [`CapabilityRow`](/docs/reference/api/types#capabilityrow)[] | yes |  |
