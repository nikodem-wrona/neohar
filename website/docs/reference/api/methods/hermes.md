---
title: "hermes.*"
description: "The hermes methods."
sidebar_position: 19
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::



| Method | Scopes | Params | Result |
|---|---|---|---|
| [`hermes.check`](#hermes-check) | `ui` `cli` `phone` | [`Empty`](/docs/reference/api/types#empty) | [`HermesStatus`](/docs/reference/api/types#hermesstatus) |
| [`hermes.start`](#hermes-start) | `ui` `cli` `phone` | [`HermesStart`](/docs/reference/api/types#hermesstart) | [`HermesStarted`](/docs/reference/api/types#hermesstarted) |
| [`hermes.prompt`](#hermes-prompt) | `ui` `cli` `phone` | [`HermesPrompt`](/docs/reference/api/types#hermesprompt) | [`Empty`](/docs/reference/api/types#empty) |
| [`hermes.cancel`](#hermes-cancel) | `ui` `cli` `phone` | [`HermesChatRef`](/docs/reference/api/types#hermeschatref) | [`Empty`](/docs/reference/api/types#empty) |
| [`hermes.answer`](#hermes-answer) | `ui` `cli` `phone` | [`HermesAnswer`](/docs/reference/api/types#hermesanswer) | [`Empty`](/docs/reference/api/types#empty) |
| [`hermes.set`](#hermes-set) | `ui` `cli` `phone` | [`HermesSet`](/docs/reference/api/types#hermesset) | [`Empty`](/docs/reference/api/types#empty) |
| [`hermes.retry`](#hermes-retry) | `ui` `cli` `phone` | [`HermesChatRef`](/docs/reference/api/types#hermeschatref) | [`Empty`](/docs/reference/api/types#empty) |
| [`hermes.close`](#hermes-close) | `ui` `cli` `phone` | [`HermesChatRef`](/docs/reference/api/types#hermeschatref) | [`Empty`](/docs/reference/api/types#empty) |

## `hermes.check` {#hermes-check}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh hermes check`](/docs/reference/cli/commands/hermes#hermes-check)

An empty result or empty params: `{}`.

### Params

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

### Result

[`HermesStatus`](/docs/reference/api/types#hermesstatus)

| Field | Type | Required | Description |
|---|---|---|---|
| `approvals_mode` | string \| `null` |  | `approvals.mode` of the config of Hermes. With `smart` or `off`, shell commands do not ask in the chat. |
| `found` | boolean | yes | `hermes acp --version` answered. |
| `message` | string \| `null` |  | What to do, when Hermes is not found or the check failed. |
| `ok` | boolean | yes | `hermes acp --check` passed. |
| `version` | string \| `null` |  |  |

## `hermes.start` {#hermes-start}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh hermes start`](/docs/reference/cli/commands/hermes#hermes-start)

### Params

[`HermesStart`](/docs/reference/api/types#hermesstart)

| Field | Type | Required | Description |
|---|---|---|---|
| `cwd` | string | yes | The absolute path of the folder in which Hermes works. |
| `prompt` | string \| `null` |  | The first prompt. |

### Result

[`HermesStarted`](/docs/reference/api/types#hermesstarted)

| Field | Type | Required | Description |
|---|---|---|---|
| `chat` | [`ChatId`](/docs/reference/api/types#chatid) | yes |  |

## `hermes.prompt` {#hermes-prompt}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh hermes prompt`](/docs/reference/cli/commands/hermes#hermes-prompt)

### Params

[`HermesPrompt`](/docs/reference/api/types#hermesprompt)

| Field | Type | Required | Description |
|---|---|---|---|
| `chat` | [`ChatId`](/docs/reference/api/types#chatid) | yes |  |
| `input_id` | integer (uint64) | yes | A second call with the same id sends nothing. |
| `text` | string | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `hermes.cancel` {#hermes-cancel}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh hermes cancel`](/docs/reference/cli/commands/hermes#hermes-cancel)

### Params

[`HermesChatRef`](/docs/reference/api/types#hermeschatref)

| Field | Type | Required | Description |
|---|---|---|---|
| `chat` | [`ChatId`](/docs/reference/api/types#chatid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `hermes.answer` {#hermes-answer}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh hermes answer`](/docs/reference/cli/commands/hermes#hermes-answer)

### Params

[`HermesAnswer`](/docs/reference/api/types#hermesanswer)

| Field | Type | Required | Description |
|---|---|---|---|
| `chat` | [`ChatId`](/docs/reference/api/types#chatid) | yes |  |
| `option` | string | yes | The `id` of one of its options. |
| `request` | string | yes | The `request` of the approval item. |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `hermes.set` {#hermes-set}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh hermes set`](/docs/reference/cli/commands/hermes#hermes-set)

### Params

[`HermesSet`](/docs/reference/api/types#hermesset)

| Field | Type | Required | Description |
|---|---|---|---|
| `chat` | [`ChatId`](/docs/reference/api/types#chatid) | yes |  |
| `mode` | string \| `null` |  | The id of a mode of the chat. |
| `model` | string \| `null` |  | The id of a model of the chat. |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `hermes.retry` {#hermes-retry}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh hermes retry`](/docs/reference/cli/commands/hermes#hermes-retry)

### Params

[`HermesChatRef`](/docs/reference/api/types#hermeschatref)

| Field | Type | Required | Description |
|---|---|---|---|
| `chat` | [`ChatId`](/docs/reference/api/types#chatid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `hermes.close` {#hermes-close}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh hermes close`](/docs/reference/cli/commands/hermes#hermes-close)

### Params

[`HermesChatRef`](/docs/reference/api/types#hermeschatref)

| Field | Type | Required | Description |
|---|---|---|---|
| `chat` | [`ChatId`](/docs/reference/api/types#chatid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.
