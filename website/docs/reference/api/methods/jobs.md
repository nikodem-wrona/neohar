---
title: "jobs.*"
description: "Long commands that run as jobs."
sidebar_position: 31
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

Long commands that run as jobs.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`jobs.cancel`](#jobs-cancel) | `ui` `cli` `phone` | [`JobsCancel`](/docs/reference/api/types#jobscancel) | [`Empty`](/docs/reference/api/types#empty) |

## `jobs.cancel` {#jobs-cancel}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh jobs cancel`](/docs/reference/cli/commands/jobs#jobs-cancel)

### Params

[`JobsCancel`](/docs/reference/api/types#jobscancel)

| Field | Type | Required | Description |
|---|---|---|---|
| `job` | [`JobId`](/docs/reference/api/types#jobid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.
