---
title: "doctor.*"
description: "The checks of `nh doctor`."
sidebar_position: 23
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The checks of `nh doctor`.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`doctor.run`](#doctor-run) | `ui` `cli` `phone` | [`DoctorRun`](/docs/reference/api/types#doctorrun) | [`DoctorReport`](/docs/reference/api/types#doctorreport) |

## `doctor.run` {#doctor-run}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

### Params

[`DoctorRun`](/docs/reference/api/types#doctorrun)

An object with no fields: `{}`.

### Result

[`DoctorReport`](/docs/reference/api/types#doctorreport)

| Field | Type | Required | Description |
|---|---|---|---|
| `checks` | [`DoctorCheck`](/docs/reference/api/types#doctorcheck)[] | yes |  |
