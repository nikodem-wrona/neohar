---
title: "askpass.*"
description: "The askpass methods."
sidebar_position: 10
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::



| Method | Scopes | Params | Result |
|---|---|---|---|
| [`askpass.ask`](#askpass-ask) | `cli` | [`AskpassAsk`](/docs/reference/api/types#askpassask) | [`AskpassAnswered`](/docs/reference/api/types#askpassanswered) |

## `askpass.ask` {#askpass-ask}

**Scopes:** `cli` — the CLI and any other client on the API socket.

`askpass.ask`: a question of `ssh` that the app asks the person. `nh askpass` calls it with the text that ssh gave. Only the host in the process of the core answers.

### Params

[`AskpassAsk`](/docs/reference/api/types#askpassask)

| Field | Type | Required | Description |
|---|---|---|---|
| `machine` | string \| `null` |  | The label of the machine that the link is for, when the core knows it. |
| `prompt` | string | yes | The question as ssh wrote it: a host key, a passphrase or a password. |

### Result

[`AskpassAnswered`](/docs/reference/api/types#askpassanswered)

| Field | Type | Required | Description |
|---|---|---|---|
| `answer` | string \| `null` |  |  |
