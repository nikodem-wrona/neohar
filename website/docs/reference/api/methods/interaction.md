---
title: "interaction.*"
description: "What agents ask a person: approvals, questions, plan reviews and notices."
sidebar_position: 14
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

What agents ask a person: approvals, questions, plan reviews and notices.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`interaction.list`](#interaction-list) | `ui` `cli` `phone` | [`InteractionList`](/docs/reference/api/types#interactionlist) | [`InteractionListResult`](/docs/reference/api/types#interactionlistresult) |
| [`interaction.show`](#interaction-show) | `ui` `cli` `phone` | [`InteractionShow`](/docs/reference/api/types#interactionshow) | [`InteractionShown`](/docs/reference/api/types#interactionshown) |
| [`interaction.answer`](#interaction-answer) | `ui` `cli` `phone` | [`InteractionAnswer`](/docs/reference/api/types#interactionanswer) | [`InteractionAnswered`](/docs/reference/api/types#interactionanswered) |
| [`interaction.dismiss`](#interaction-dismiss) | `ui` `cli` `phone` | [`InteractionDismiss`](/docs/reference/api/types#interactiondismiss) | [`Empty`](/docs/reference/api/types#empty) |
| [`interaction.next`](#interaction-next) | `ui` `cli` `phone` | [`InteractionNext`](/docs/reference/api/types#interactionnext) | [`InteractionNextResult`](/docs/reference/api/types#interactionnextresult) |
| [`interaction.reveal`](#interaction-reveal) | `ui` `cli` `phone` | [`InteractionReveal`](/docs/reference/api/types#interactionreveal) | [`InteractionRevealed`](/docs/reference/api/types#interactionrevealed) |

## `interaction.list` {#interaction-list}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh interaction list`](/docs/reference/cli/commands/interaction#interaction-list)

### Params

[`InteractionList`](/docs/reference/api/types#interactionlist)

| Field | Type | Required | Description |
|---|---|---|---|
| `limit` | integer (uint32) \| `null` |  |  |
| `open_only` | boolean |  | Only the open ones. It is the default. Default: `true`. |
| `pane` | [`RefPaneId`](/docs/reference/api/types#refpaneid) \| `null` |  |  |
| `run` | [`RefRunId`](/docs/reference/api/types#refrunid) \| `null` |  |  |

### Result

[`InteractionListResult`](/docs/reference/api/types#interactionlistresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `interactions` | [`InteractionValue`](/docs/reference/api/types#interactionvalue)[] | yes |  |

## `interaction.show` {#interaction-show}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh interaction show`](/docs/reference/cli/commands/interaction#interaction-show)

### Params

[`InteractionShow`](/docs/reference/api/types#interactionshow)

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`RefInteractionId`](/docs/reference/api/types#refinteractionid) | yes |  |

### Result

[`InteractionShown`](/docs/reference/api/types#interactionshown)

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionValue`](/docs/reference/api/types#interactionvalue) | yes |  |

## `interaction.answer` {#interaction-answer}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh interaction answer`](/docs/reference/cli/commands/interaction#interaction-answer)

`via` goes into the delivery record. A second answer gets `already_answered`.

### Params

[`InteractionAnswer`](/docs/reference/api/types#interactionanswer)

| Field | Type | Required | Description |
|---|---|---|---|
| `decision` | [`Decision`](/docs/reference/api/types#decision) | yes |  |
| `interaction` | [`RefInteractionId`](/docs/reference/api/types#refinteractionid) | yes |  |
| `via` | [`Via`](/docs/reference/api/types#via) | yes |  |

### Result

[`InteractionAnswered`](/docs/reference/api/types#interactionanswered)

| Field | Type | Required | Description |
|---|---|---|---|
| `rev` | integer (uint32) | yes | The revision of the decision. |
| `state` | [`InteractionState`](/docs/reference/api/types#interactionstate) | yes |  |

## `interaction.dismiss` {#interaction-dismiss}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh interaction dismiss`](/docs/reference/cli/commands/interaction#interaction-dismiss)

Closes an interaction without an answer (a notice, or a card the user does not want).

### Params

[`InteractionDismiss`](/docs/reference/api/types#interactiondismiss)

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`RefInteractionId`](/docs/reference/api/types#refinteractionid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `interaction.next` {#interaction-next}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh interaction next`](/docs/reference/cli/commands/interaction#interaction-next)

The most urgent open interaction that a client can answer.

### Params

[`InteractionNext`](/docs/reference/api/types#interactionnext)

| Field | Type | Required | Description |
|---|---|---|---|
| `after` | [`RefInteractionId`](/docs/reference/api/types#refinteractionid) \| `null` |  |  |

### Result

[`InteractionNextResult`](/docs/reference/api/types#interactionnextresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string \| `null` |  |  |
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) \| `null` |  |  |

## `interaction.reveal` {#interaction-reveal}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh interaction reveal`](/docs/reference/cli/commands/interaction#interaction-reveal)

Brings an interaction to the person: the app comes to the front, and the window shows the inbox with the card focused. It does not answer and does not focus the pane. Without `interaction`, it takes the result of `interaction.next`.

### Params

[`InteractionReveal`](/docs/reference/api/types#interactionreveal)

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`RefInteractionId`](/docs/reference/api/types#refinteractionid) \| `null` |  |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  | The window that shows the card. The default is the window with the OS focus, else the first one. |

### Result

[`InteractionRevealed`](/docs/reference/api/types#interactionrevealed)

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) \| `null` |  | The interaction that the window shows, or none when nothing waits. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  |  |
