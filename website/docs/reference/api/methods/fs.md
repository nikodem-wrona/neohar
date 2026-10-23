---
title: "fs.*"
description: "The files of the machine for the Files tab: list a folder, read a preview, open a path and copy a path."
sidebar_position: 9
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The files of the machine for the Files tab: list a folder, read a preview, open a path and copy a path.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`fs.list`](#fs-list) | `ui` `cli` `phone` | [`FsList`](/docs/reference/api/types#fslist) | [`FsListResult`](/docs/reference/api/types#fslistresult) |
| [`fs.read`](#fs-read) | `ui` `phone` | [`FsRead`](/docs/reference/api/types#fsread) | [`FsReadResult`](/docs/reference/api/types#fsreadresult) |
| [`fs.open`](#fs-open) | `ui` | [`FsOpen`](/docs/reference/api/types#fsopen) | [`Empty`](/docs/reference/api/types#empty) |
| [`fs.copy_path`](#fs-copy-path) | `ui` `phone` | [`FsCopyPath`](/docs/reference/api/types#fscopypath) | [`CopyResult`](/docs/reference/api/types#copyresult) |

## `fs.list` {#fs-list}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh fs list`](/docs/reference/cli/commands/fs#fs-list)

`fs.list`: the entries of a folder of the machine, for the Files tab.

### Params

[`FsList`](/docs/reference/api/types#fslist)

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | An absolute path. |

### Result

[`FsListResult`](/docs/reference/api/types#fslistresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `entries` | [`FsEntry`](/docs/reference/api/types#fsentry)[] | yes |  |
| `path` | string | yes | The folder that the core listed, as an absolute path. |
| `truncated` | boolean | yes | True when the folder has more entries than the core sends (2,000). |

## `fs.read` {#fs-read}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`fs.read`: the preview of a regular file of the machine.

### Params

[`FsRead`](/docs/reference/api/types#fsread)

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | An absolute path. |

### Result

[`FsReadResult`](/docs/reference/api/types#fsreadresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `icon` | string | yes | The icon of the file, as in `FsEntry`. |
| `image` | string \| `null` |  | A `data:` URL of an image of at most 8 MiB. |
| `info` | string | yes | The text of the head of the file view, for example "120 lines · 4.2 KB". |
| `kind` | [`FsPreviewKind`](/docs/reference/api/types#fspreviewkind) | yes |  |
| `lines` | [`Spans`](/docs/reference/api/types#spans)[] | yes | One line of text for each line of the file, with its highlight. Empty for an image and for a binary file. |
| `markdown` | [`Markdown`](/docs/reference/api/types#markdown) \| `null` |  | The HTML of a Markdown file of at most 256 KiB. |
| `name` | string | yes | The last part of the path. |
| `path` | string | yes |  |
| `truncated` | boolean | yes | The file has more than the core sends: 1 MiB or 10,000 lines. |

## `fs.open` {#fs-open}

**Scopes:** `ui` — the UI, over the `rpc` command of the Tauri host or the WebSocket.

`fs.open`: the host of the window opens a path, or shows it in the Finder.

### Params

[`FsOpen`](/docs/reference/api/types#fsopen)

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | An absolute path. |
| `reveal` | boolean |  | Show the path in the Finder, and do not open it. Default: `false`. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `fs.copy_path` {#fs-copy-path}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`fs.copy_path`: the core writes a path to the clipboard of the device that called.

### Params

[`FsCopyPath`](/docs/reference/api/types#fscopypath)

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | An absolute path. |
| `relative_to` | string \| `null` |  | A folder that holds `path`. The clipboard gets the path from this folder. |
| `return_text` | boolean |  | Return the text, and write no clipboard of a host: a phone writes it in the tap. Default: `false`. |

### Result

[`CopyResult`](/docs/reference/api/types#copyresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `text` | string \| `null` |  |  |
