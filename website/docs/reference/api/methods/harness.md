---
title: "harness.*"
description: "Claude Code and Codex on this machine: discovery, version and login state."
sidebar_position: 13
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

Claude Code and Codex on this machine: discovery, version and login state.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`harness.refresh`](#harness-refresh) | `ui` `cli` `phone` | [`HarnessRefresh`](/docs/reference/api/types#harnessrefresh) | [`Empty`](/docs/reference/api/types#empty) |

## `harness.refresh` {#harness-refresh}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh harness refresh`](/docs/reference/cli/commands/harness#harness-refresh)

Finds the harnesses again and asks each one for its login state now. The call returns when the probes are done, and the result is in the projection `harnesses`.

### Params

[`HarnessRefresh`](/docs/reference/api/types#harnessrefresh)

| Field | Type | Required | Description |
|---|---|---|---|
| `harness` | string \| `null` |  | The id of one manifest, for example `codex`. Without it, every harness. |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.
