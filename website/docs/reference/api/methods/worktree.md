---
title: "worktree.*"
description: "The git worktrees of tasks."
sidebar_position: 18
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The git worktrees of tasks.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`worktree.list`](#worktree-list) | `ui` `cli` `phone` | [`WorktreeList`](/docs/reference/api/types#worktreelist) | [`WorktreeListResult`](/docs/reference/api/types#worktreelistresult) |

## `worktree.list` {#worktree-list}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh worktree list`](/docs/reference/cli/commands/worktree#worktree-list)

### Params

[`WorktreeList`](/docs/reference/api/types#worktreelist)

| Field | Type | Required | Description |
|---|---|---|---|
| `repo` | string \| `null` |  |  |

### Result

[`WorktreeListResult`](/docs/reference/api/types#worktreelistresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `worktrees` | [`WorktreeValue`](/docs/reference/api/types#worktreevalue)[] | yes |  |
