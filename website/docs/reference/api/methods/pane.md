---
title: "pane.*"
description: "Panes: terminals with a holder, and their input, scrolling, selection, copy and text."
sidebar_position: 7
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

Panes: terminals with a holder, and their input, scrolling, selection, copy and text.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`pane.split`](#pane-split) | `ui` `cli` `phone` | [`PaneSplit`](/docs/reference/api/types#panesplit) | [`PaneCreated`](/docs/reference/api/types#panecreated) |
| [`pane.close`](#pane-close) | `ui` `cli` `phone` | [`PaneClose`](/docs/reference/api/types#paneclose) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.focus`](#pane-focus) | `ui` `phone` | [`PaneFocus`](/docs/reference/api/types#panefocus) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.claim`](#pane-claim) | `ui` `phone` | [`PaneClaim`](/docs/reference/api/types#paneclaim) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.move`](#pane-move) | `ui` `cli` `phone` | [`PaneMove`](/docs/reference/api/types#panemove) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.float`](#pane-float) | `ui` `cli` `phone` | [`PaneFloat`](/docs/reference/api/types#panefloat) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.unfloat`](#pane-unfloat) | `ui` `cli` `phone` | [`PaneUnfloat`](/docs/reference/api/types#paneunfloat) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.set_float_rect`](#pane-set-float-rect) | `ui` `phone` | [`PaneSetFloatRect`](/docs/reference/api/types#panesetfloatrect) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.popout`](#pane-popout) | `ui` `cli` | [`PanePopout`](/docs/reference/api/types#panepopout) | [`WindowOpened`](/docs/reference/api/types#windowopened) |
| [`pane.restart`](#pane-restart) | `ui` `cli` `phone` | [`PaneRestart`](/docs/reference/api/types#panerestart) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.resize`](#pane-resize) | `ui` `phone` | [`PaneResize`](/docs/reference/api/types#paneresize) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.input`](#pane-input) | `ui` `phone` | [`PaneInput`](/docs/reference/api/types#paneinput) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.paste`](#pane-paste) | `ui` `cli` `phone` | [`PanePaste`](/docs/reference/api/types#panepaste) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.send_bytes`](#pane-send-bytes) | `ui` `cli` `phone` | [`PaneSendBytes`](/docs/reference/api/types#panesendbytes) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.drop_files`](#pane-drop-files) | `ui` `phone` | [`PaneDropFiles`](/docs/reference/api/types#panedropfiles) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.drop_blobs`](#pane-drop-blobs) | `ui` `phone` | [`PaneDropBlobs`](/docs/reference/api/types#panedropblobs) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.insert_paths`](#pane-insert-paths) | `ui` `phone` | [`PaneInsertPaths`](/docs/reference/api/types#paneinsertpaths) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.scroll`](#pane-scroll) | `ui` `cli` `phone` | [`PaneScroll`](/docs/reference/api/types#panescroll) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.select`](#pane-select) | `ui` `phone` | [`PaneSelect`](/docs/reference/api/types#paneselect) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.select_clear`](#pane-select-clear) | `ui` `phone` | [`PaneSelectClear`](/docs/reference/api/types#paneselectclear) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.copy`](#pane-copy) | `ui` `cli` `phone` | [`PaneCopy`](/docs/reference/api/types#panecopy) | [`CopyResult`](/docs/reference/api/types#copyresult) |
| [`pane.read`](#pane-read) | `cli` `phone` | [`PaneRead`](/docs/reference/api/types#paneread) | [`PaneText`](/docs/reference/api/types#panetext) |
| [`pane.peek_open`](#pane-peek-open) | `ui` `phone` | [`PanePeekOpen`](/docs/reference/api/types#panepeekopen) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.peek_close`](#pane-peek-close) | `ui` `phone` | [`PanePeekClose`](/docs/reference/api/types#panepeekclose) | [`Empty`](/docs/reference/api/types#empty) |
| [`pane.clipboard_allow`](#pane-clipboard-allow) | `ui` `phone` | [`PaneClipboardAllow`](/docs/reference/api/types#paneclipboardallow) | [`CopyResult`](/docs/reference/api/types#copyresult) |
| [`pane.list`](#pane-list) | `ui` `cli` `phone` | [`PaneList`](/docs/reference/api/types#panelist) | [`PaneListResult`](/docs/reference/api/types#panelistresult) |
| [`pane.show`](#pane-show) | `ui` `cli` `phone` | [`PaneShow`](/docs/reference/api/types#paneshow) | [`PaneShown`](/docs/reference/api/types#paneshown) |

## `pane.split` {#pane-split}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh pane split`](/docs/reference/cli/commands/pane#pane-split)

### Params

[`PaneSplit`](/docs/reference/api/types#panesplit)

| Field | Type | Required | Description |
|---|---|---|---|
| `command` | [`ProcessSpec`](/docs/reference/api/types#processspec) \| `null` |  |  |
| `direction` | [`SplitDirection`](/docs/reference/api/types#splitdirection) | yes | Which side of `pane` the new pane goes on. |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes | The pane to split. |
| `ratio` | number (float) \| `null` |  | The share of the new pane, from 0 to 1. Without it the two panes share the room. |

### Result

[`PaneCreated`](/docs/reference/api/types#panecreated)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |

## `pane.close` {#pane-close}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh pane close`](/docs/reference/cli/commands/pane#pane-close)

`confirm_required` when the child is alive and `force` is false. `details` has `{process, is_agent}`.

### Params

[`PaneClose`](/docs/reference/api/types#paneclose)

| Field | Type | Required | Description |
|---|---|---|---|
| `force` | boolean |  | Default: `false`. |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.focus` {#pane-focus}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

Keyboard focus moved to a pane in a window.

### Params

[`PaneFocus`](/docs/reference/api/types#panefocus)

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.claim` {#pane-claim}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

The window takes the geometry lease of the pane as an input does, with the same debounce, and sends no byte. A phone calls it when its keyboard opens.

### Params

[`PaneClaim`](/docs/reference/api/types#paneclaim)

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.move` {#pane-move}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh pane move`](/docs/reference/cli/commands/pane#pane-move)

Moves a pane to another tab. Without `beside` the pane becomes a split of the root.

### Params

[`PaneMove`](/docs/reference/api/types#panemove)

| Field | Type | Required | Description |
|---|---|---|---|
| `beside` | [`RefPaneId`](/docs/reference/api/types#refpaneid) \| `null` |  |  |
| `direction` | [`SplitDirection`](/docs/reference/api/types#splitdirection) \| `null` |  |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.float` {#pane-float}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh pane float`](/docs/reference/cli/commands/pane#pane-float)

### Params

[`PaneFloat`](/docs/reference/api/types#panefloat)

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `rect` | [`Rect`](/docs/reference/api/types#rect) \| `null` |  | Where to put it. Without it the core picks a rectangle. |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.unfloat` {#pane-unfloat}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh pane unfloat`](/docs/reference/cli/commands/pane#pane-unfloat)

### Params

[`PaneUnfloat`](/docs/reference/api/types#paneunfloat)

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.set_float_rect` {#pane-set-float-rect}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

At most once for each animation frame while the user drags.

### Params

[`PaneSetFloatRect`](/docs/reference/api/types#panesetfloatrect)

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `rect` | [`Rect`](/docs/reference/api/types#rect) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.popout` {#pane-popout}

**Scopes:** `ui` `cli` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket.

**Command line:** [`nh pane popout`](/docs/reference/cli/commands/pane#pane-popout)

Moves a pane into a window of its own.

### Params

[`PanePopout`](/docs/reference/api/types#panepopout)

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

### Result

[`WindowOpened`](/docs/reference/api/types#windowopened)

| Field | Type | Required | Description |
|---|---|---|---|
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

## `pane.restart` {#pane-restart}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh pane restart`](/docs/reference/cli/commands/pane#pane-restart)

Starts the pane again after its child exited.

### Params

[`PaneRestart`](/docs/reference/api/types#panerestart)

| Field | Type | Required | Description |
|---|---|---|---|
| `command` | [`ProcessSpec`](/docs/reference/api/types#processspec) \| `null` |  |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.resize` {#pane-resize}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

From the `ResizeObserver`. The geometry lease decides whether it resizes the PTY.

### Params

[`PaneResize`](/docs/reference/api/types#paneresize)

| Field | Type | Required | Description |
|---|---|---|---|
| `cols` | integer (uint16) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `rows` | integer (uint16) | yes |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.input` {#pane-input}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

One logical event, at once. `input_id` is unique for the core epoch. A holder drops a repeated id.

### Params

[`PaneInput`](/docs/reference/api/types#paneinput)

| Field | Type | Required | Description |
|---|---|---|---|
| `event` | [`LogicalInput`](/docs/reference/api/types#logicalinput) | yes |  |
| `input_id` | integer (uint64) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.paste` {#pane-paste}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh pane paste`](/docs/reference/cli/commands/pane#pane-paste)

### Params

[`PanePaste`](/docs/reference/api/types#panepaste)

| Field | Type | Required | Description |
|---|---|---|---|
| `input_id` | integer (uint64) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `text` | string | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.send_bytes` {#pane-send-bytes}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

Raw bytes, as the explicit exception to logical input.

### Params

[`PaneSendBytes`](/docs/reference/api/types#panesendbytes)

| Field | Type | Required | Description |
|---|---|---|---|
| `bytes` | [`Bytes`](/docs/reference/api/types#bytes) | yes |  |
| `input_id` | integer (uint64) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.drop_files` {#pane-drop-files}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

The core quotes the paths and pastes them.

### Params

[`PaneDropFiles`](/docs/reference/api/types#panedropfiles)

| Field | Type | Required | Description |
|---|---|---|---|
| `input_id` | integer (uint64) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `paths` | string[] | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.drop_blobs` {#pane-drop-blobs}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

Files dropped on a pane of a remote window. The client has put each file in the blob store over its link. The core writes each blob to a file of the machine of the pane, then quotes the paths and pastes them as `pane.drop_files` does.

### Params

[`PaneDropBlobs`](/docs/reference/api/types#panedropblobs)

| Field | Type | Required | Description |
|---|---|---|---|
| `blobs` | [`DroppedBlob`](/docs/reference/api/types#droppedblob)[] | yes |  |
| `input_id` | integer (uint64) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.insert_paths` {#pane-insert-paths}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

Paths of the machine of the core, from the Files tab. The core quotes and pastes them as `pane.drop_files` does, and the host never uploads them.

### Params

[`PaneInsertPaths`](/docs/reference/api/types#paneinsertpaths)

| Field | Type | Required | Description |
|---|---|---|---|
| `input_id` | integer (uint64) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `paths` | string[] | yes | 1 to 20 absolute paths. |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.scroll` {#pane-scroll}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh pane scroll`](/docs/reference/cli/commands/pane#pane-scroll)

A viewport change. Frames follow.

### Params

[`PaneScroll`](/docs/reference/api/types#panescroll)

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

Plus exactly one of these forms:

#### Form 1

The absolute line at the top of the viewport.

| Field | Type | Required | Description |
|---|---|---|---|
| `top_line` | integer (uint64) | yes |  |

#### Form 2

Lines up (negative) or down (positive).

| Field | Type | Required | Description |
|---|---|---|---|
| `delta` | integer (int64) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.select` {#pane-select}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

The selection lives in the core.

### Params

[`PaneSelect`](/docs/reference/api/types#paneselect)

| Field | Type | Required | Description |
|---|---|---|---|
| `anchor` | [`SelectPoint`](/docs/reference/api/types#selectpoint) | yes |  |
| `head` | [`SelectPoint`](/docs/reference/api/types#selectpoint) | yes |  |
| `mode` | [`SelectMode`](/docs/reference/api/types#selectmode) | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.select_clear` {#pane-select-clear}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

### Params

[`PaneSelectClear`](/docs/reference/api/types#paneselectclear)

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.copy` {#pane-copy}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh pane copy`](/docs/reference/cli/commands/pane#pane-copy)

Writes the selection to the clipboard through the host.

### Params

[`PaneCopy`](/docs/reference/api/types#panecopy)

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `return_text` | boolean |  | Also return the text, for the CLI and for a phone. A phone writes its own clipboard in the tap, so the core writes none for it. Default: `false`. |

### Result

[`CopyResult`](/docs/reference/api/types#copyresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `text` | string \| `null` |  |  |

## `pane.read` {#pane-read}

**Scopes:** `cli` `phone` — the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh pane read`](/docs/reference/cli/commands/pane#pane-read)

Reads text from a pane. Without `lines` it reads the visible screen.

### Params

[`PaneRead`](/docs/reference/api/types#paneread)

| Field | Type | Required | Description |
|---|---|---|---|
| `lines` | [`LineRange`](/docs/reference/api/types#linerange) \| `null` |  |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `visible` | boolean |  | Read the visible screen. It is the default when `lines` is absent. Default: `false`. |

### Result

[`PaneText`](/docs/reference/api/types#panetext)

| Field | Type | Required | Description |
|---|---|---|---|
| `from_line` | integer (uint64) | yes | The absolute line of the first line of `text`. |
| `text` | string | yes |  |

## `pane.peek_open` {#pane-peek-open}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

Starts low-rate frames for a pane that is not in the grid.

### Params

[`PanePeekOpen`](/docs/reference/api/types#panepeekopen)

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.peek_close` {#pane-peek-close}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

### Params

[`PanePeekClose`](/docs/reference/api/types#panepeekclose)

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `pane.clipboard_allow` {#pane-clipboard-allow}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

The answer to an OSC 52 clipboard write.

### Params

[`PaneClipboardAllow`](/docs/reference/api/types#paneclipboardallow)

| Field | Type | Required | Description |
|---|---|---|---|
| `allow` | boolean | yes |  |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |
| `remember` | boolean |  | Later writes of this pane follow the answer without asking. Default: `false`. |
| `return_text` | boolean |  | Return the text, and write no clipboard of a host: a phone writes it in the tap. Default: `false`. |

### Result

[`CopyResult`](/docs/reference/api/types#copyresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `text` | string \| `null` |  |  |

## `pane.list` {#pane-list}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh pane list`](/docs/reference/cli/commands/pane#pane-list)

### Params

[`PaneList`](/docs/reference/api/types#panelist)

| Field | Type | Required | Description |
|---|---|---|---|
| `tab` | [`RefTabId`](/docs/reference/api/types#reftabid) \| `null` |  |  |
| `workspace` | [`RefWorkspaceId`](/docs/reference/api/types#refworkspaceid) \| `null` |  |  |

### Result

[`PaneListResult`](/docs/reference/api/types#panelistresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `panes` | [`PaneValue`](/docs/reference/api/types#panevalue)[] | yes |  |

## `pane.show` {#pane-show}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh pane show`](/docs/reference/cli/commands/pane#pane-show)

### Params

[`PaneShow`](/docs/reference/api/types#paneshow)

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) | yes |  |

### Result

[`PaneShown`](/docs/reference/api/types#paneshown)

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`PaneValue`](/docs/reference/api/types#panevalue) | yes |  |
