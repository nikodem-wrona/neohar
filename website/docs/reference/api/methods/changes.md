---
title: "changes.*"
description: "The changes methods."
sidebar_position: 16
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::



| Method | Scopes | Params | Result |
|---|---|---|---|
| [`changes.follow`](#changes-follow) | `ui` `phone` | [`ChangesFollow`](/docs/reference/api/types#changesfollow) | [`Empty`](/docs/reference/api/types#empty) |
| [`changes.diff`](#changes-diff) | `ui` `phone` | [`ChangesDiff`](/docs/reference/api/types#changesdiff) | [`FileDiff`](/docs/reference/api/types#filediff) |

## `changes.follow` {#changes-follow}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`changes.follow`: the Changes tab of a window shows the changes of the git worktree that holds `path`. The core fills `changes_head:<window>` and `changes_for:<window>` while the window follows one of them, and counts again when the worktree changes.

### Params

[`ChangesFollow`](/docs/reference/api/types#changesfollow)

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | An absolute path: the folder of the Files tab of the window. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `changes.diff` {#changes-diff}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`changes.diff` gives the diff of one path of the rows of `changes_for:<window>`.

### Params

[`ChangesDiff`](/docs/reference/api/types#changesdiff)

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

### Result

[`FileDiff`](/docs/reference/api/types#filediff)

| Field | Type | Required | Description |
|---|---|---|---|
| `hunks` | [`Hunk`](/docs/reference/api/types#hunk)[] | yes |  |
| `path` | string | yes |  |
| `stats` | [`DiffStats`](/docs/reference/api/types#diffstats) | yes |  |
| `status` | [`FileStatus`](/docs/reference/api/types#filestatus) | yes |  |
| `truncated` | boolean |  | The diff of this file was cut at 2,000 lines. The UI offers "open full". Default: `false`. |
