---
title: "policy.*"
description: "Rules that decide approvals before a person is asked."
sidebar_position: 22
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

Rules that decide approvals before a person is asked.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`policy.list`](#policy-list) | `ui` `cli` `phone` | [`PolicyList`](/docs/reference/api/types#policylist) | [`PolicyListResult`](/docs/reference/api/types#policylistresult) |
| [`policy.add`](#policy-add) | `ui` `cli` `phone` | [`PolicyAdd`](/docs/reference/api/types#policyadd) | [`PolicyAdded`](/docs/reference/api/types#policyadded) |
| [`policy.remove`](#policy-remove) | `ui` `cli` `phone` | [`PolicyRemove`](/docs/reference/api/types#policyremove) | [`Empty`](/docs/reference/api/types#empty) |
| [`policy.suggest`](#policy-suggest) | `ui` `cli` `phone` | [`PolicySuggest`](/docs/reference/api/types#policysuggest) | [`PolicySuggestResult`](/docs/reference/api/types#policysuggestresult) |

## `policy.list` {#policy-list}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh policy list`](/docs/reference/cli/commands/policy#policy-list)

### Params

[`PolicyList`](/docs/reference/api/types#policylist)

An object with no fields: `{}`.

### Result

[`PolicyListResult`](/docs/reference/api/types#policylistresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `rules` | [`PolicyRuleValue`](/docs/reference/api/types#policyrulevalue)[] | yes |  |

## `policy.add` {#policy-add}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh policy add`](/docs/reference/cli/commands/policy#policy-add)

### Params

[`PolicyAdd`](/docs/reference/api/types#policyadd)

| Field | Type | Required | Description |
|---|---|---|---|
| `rule` | [`PolicyRule`](/docs/reference/api/types#policyrule) | yes |  |

### Result

[`PolicyAdded`](/docs/reference/api/types#policyadded)

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes |  |

## `policy.remove` {#policy-remove}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh policy remove`](/docs/reference/cli/commands/policy#policy-remove)

### Params

[`PolicyRemove`](/docs/reference/api/types#policyremove)

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `policy.suggest` {#policy-suggest}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh policy suggest`](/docs/reference/cli/commands/policy#policy-suggest)

### Params

[`PolicySuggest`](/docs/reference/api/types#policysuggest)

An object with no fields: `{}`.

### Result

[`PolicySuggestResult`](/docs/reference/api/types#policysuggestresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `suggestions` | [`PolicySuggestion`](/docs/reference/api/types#policysuggestion)[] | yes |  |
