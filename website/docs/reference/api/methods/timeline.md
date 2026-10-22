---
title: "timeline.*"
description: "Older items of the timeline of a run."
sidebar_position: 30
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

Older items of the timeline of a run.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`timeline.page`](#timeline-page) | `ui` `phone` | [`TimelinePageParams`](/docs/reference/api/types#timelinepageparams) | [`TimelinePageResult`](/docs/reference/api/types#timelinepageresult) |
| [`timeline.item`](#timeline-item) | `ui` `phone` | [`TimelineItemGet`](/docs/reference/api/types#timelineitemget) | [`TimelineItemText`](/docs/reference/api/types#timelineitemtext) |

## `timeline.page` {#timeline-page}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

Older items of `timeline:<run>`. They are history, not live state.

### Params

[`TimelinePageParams`](/docs/reference/api/types#timelinepageparams)

| Field | Type | Required | Description |
|---|---|---|---|
| `before_item` | string \| `null` |  |  |
| `limit` | integer (uint32) \| `null` |  |  |
| `run` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |

### Result

[`TimelinePageResult`](/docs/reference/api/types#timelinepageresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `items` | [`TimelineItem`](/docs/reference/api/types#timelineitem)[] | yes |  |
| `more` | boolean | yes |  |

## `timeline.item` {#timeline-item}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

The full text of a `prompt` item or an `assistant` item of `timeline:<run>`. An item of the projection has a preview only.

### Params

[`TimelineItemGet`](/docs/reference/api/types#timelineitemget)

| Field | Type | Required | Description |
|---|---|---|---|
| `item` | string | yes |  |
| `run` | [`RefRunId`](/docs/reference/api/types#refrunid) | yes |  |

### Result

[`TimelineItemText`](/docs/reference/api/types#timelineitemtext)

| Field | Type | Required | Description |
|---|---|---|---|
| `html` | string | yes |  |
| `truncated` | boolean | yes |  |
