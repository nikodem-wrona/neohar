---
title: "search.*"
description: "Search of the scrollback archive."
sidebar_position: 27
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

Search of the scrollback archive.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`search.query`](#search-query) | `ui` `cli` `phone` | [`SearchQuery`](/docs/reference/api/types#searchquery) | [`SearchResults`](/docs/reference/api/types#searchresults) |

## `search.query` {#search-query}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh search query`](/docs/reference/cli/commands/search#search-query)

### Params

[`SearchQuery`](/docs/reference/api/types#searchquery)

| Field | Type | Required | Description |
|---|---|---|---|
| `limit` | integer (uint32) \| `null` |  |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) \| `null` |  |  |
| `query` | string | yes |  |
| `workspace` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) \| `null` |  |  |

### Result

[`SearchResults`](/docs/reference/api/types#searchresults)

| Field | Type | Required | Description |
|---|---|---|---|
| `hits` | [`SearchHit`](/docs/reference/api/types#searchhit)[] | yes |  |
| `job` | [`JobId`](/docs/reference/api/types#jobid) \| `null` |  | A job that goes on searching the archive. Its result has the later hits. |
| `truncated` | boolean |  | More hits exist than `limit`. Default: `false`. |
