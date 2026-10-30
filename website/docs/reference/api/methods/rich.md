---
title: "rich.*"
description: "Rich content on demand: diffs, sanitized markdown and highlighted code."
sidebar_position: 27
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

Rich content on demand: diffs, sanitized markdown and highlighted code.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`rich.diff`](#rich-diff) | `ui` `phone` | [`RichDiff`](/docs/reference/api/types#richdiff) | [`Diff`](/docs/reference/api/types#diff) |
| [`rich.markdown`](#rich-markdown) | `ui` `phone` | [`RichMarkdown`](/docs/reference/api/types#richmarkdown) | [`Markdown`](/docs/reference/api/types#markdown) |
| [`rich.highlight`](#rich-highlight) | `ui` `phone` | [`RichHighlight`](/docs/reference/api/types#richhighlight) | [`Spans`](/docs/reference/api/types#spans) |

## `rich.diff` {#rich-diff}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

### Params

[`RichDiff`](/docs/reference/api/types#richdiff)

| Field | Type | Required | Description |
|---|---|---|---|
| `a` | string | yes |  |
| `b` | string | yes |  |
| `lang` | string \| `null` |  |  |
| `path` | string \| `null` |  |  |

### Result

[`Diff`](/docs/reference/api/types#diff)

| Field | Type | Required | Description |
|---|---|---|---|
| `files` | [`FileDiff`](/docs/reference/api/types#filediff)[] | yes |  |

## `rich.markdown` {#rich-markdown}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

### Params

[`RichMarkdown`](/docs/reference/api/types#richmarkdown)

| Field | Type | Required | Description |
|---|---|---|---|
| `text` | string | yes |  |

### Result

[`Markdown`](/docs/reference/api/types#markdown)

| Field | Type | Required | Description |
|---|---|---|---|
| `html` | string | yes |  |
| `truncated` | boolean |  | The text was cut at the cap (256 KB for a plan). The UI offers "open full". Default: `false`. |

## `rich.highlight` {#rich-highlight}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

### Params

[`RichHighlight`](/docs/reference/api/types#richhighlight)

| Field | Type | Required | Description |
|---|---|---|---|
| `lang` | string | yes |  |
| `text` | string | yes |  |

### Result

[`Spans`](/docs/reference/api/types#spans)

| Field | Type | Required | Description |
|---|---|---|---|
| `spans` | any[][] | yes | `[start, end, class]`, in UTF-16 code units. |
| `text` | string | yes |  |
