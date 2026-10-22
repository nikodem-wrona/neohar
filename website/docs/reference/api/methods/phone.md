---
title: "phone.*"
description: "The phones that supervise this machine: the offer of a pairing, names and removal, and the mapping of tailscale serve."
sidebar_position: 19
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The phones that supervise this machine: the offer of a pairing, names and removal, and the mapping of tailscale serve.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`phone.pair_begin`](#phone-pair-begin) | `ui` `cli` | [`PhonePairBegin`](/docs/reference/api/types#phonepairbegin) | [`PhonePairStarted`](/docs/reference/api/types#phonepairstarted) |
| [`phone.pair_cancel`](#phone-pair-cancel) | `ui` `cli` | [`Empty`](/docs/reference/api/types#empty) | [`Empty`](/docs/reference/api/types#empty) |
| [`phone.remove`](#phone-remove) | `ui` `cli` `phone` | [`PhoneRemove`](/docs/reference/api/types#phoneremove) | [`Empty`](/docs/reference/api/types#empty) |
| [`phone.rename`](#phone-rename) | `ui` `cli` `phone` | [`PhoneRename`](/docs/reference/api/types#phonerename) | [`Empty`](/docs/reference/api/types#empty) |
| [`phone.publish`](#phone-publish) | `ui` `cli` | [`Empty`](/docs/reference/api/types#empty) | [`PhoneStatusValue`](/docs/reference/api/types#phonestatusvalue) |
| [`phone.unpublish`](#phone-unpublish) | `ui` `cli` | [`Empty`](/docs/reference/api/types#empty) | [`PhoneStatusValue`](/docs/reference/api/types#phonestatusvalue) |
| [`phone.push_subscribe`](#phone-push-subscribe) | `phone` | [`PhonePushSubscribe`](/docs/reference/api/types#phonepushsubscribe) | [`Empty`](/docs/reference/api/types#empty) |
| [`phone.push_unsubscribe`](#phone-push-unsubscribe) | `phone` | [`Empty`](/docs/reference/api/types#empty) | [`Empty`](/docs/reference/api/types#empty) |
| [`phone.push_test`](#phone-push-test) | `ui` `cli` `phone` | [`PhonePushTest`](/docs/reference/api/types#phonepushtest) | [`Empty`](/docs/reference/api/types#empty) |
| [`phone.visible`](#phone-visible) | `phone` | [`PhoneVisible`](/docs/reference/api/types#phonevisible) | [`Empty`](/docs/reference/api/types#empty) |
| [`phone.window`](#phone-window) | `phone` | [`Empty`](/docs/reference/api/types#empty) | [`PhoneWindowResult`](/docs/reference/api/types#phonewindowresult) |

## `phone.pair_begin` {#phone-pair-begin}

**Scopes:** `ui` `cli` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket.

**Command line:** [`nh phone pair-begin`](/docs/reference/cli/commands/phone#phone-pair-begin)

`phone.pair_begin`: a new offer of a pairing. It ends the offer before it.

### Params

[`PhonePairBegin`](/docs/reference/api/types#phonepairbegin)

An object with no fields: `{}`.

### Result

[`PhonePairStarted`](/docs/reference/api/types#phonepairstarted)

| Field | Type | Required | Description |
|---|---|---|---|
| `code` | string | yes |  |
| `expires_at` | [`Timestamp`](/docs/reference/api/types#timestamp) | yes |  |
| `url` | string | yes |  |

## `phone.pair_cancel` {#phone-pair-cancel}

**Scopes:** `ui` `cli` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket.

**Command line:** [`nh phone pair-cancel`](/docs/reference/cli/commands/phone#phone-pair-cancel)

An empty result or empty params: `{}`.

### Params

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `phone.remove` {#phone-remove}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh phone remove`](/docs/reference/cli/commands/phone#phone-remove)

`phone.remove`: removes a paired phone, closes its connections and its streams, and removes its phone windows.

### Params

[`PhoneRemove`](/docs/reference/api/types#phoneremove)

| Field | Type | Required | Description |
|---|---|---|---|
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `phone.rename` {#phone-rename}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh phone rename`](/docs/reference/cli/commands/phone#phone-rename)

`phone.rename`: the name of a paired phone.

### Params

[`PhoneRename`](/docs/reference/api/types#phonerename)

| Field | Type | Required | Description |
|---|---|---|---|
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) | yes |  |
| `name` | string | yes | 1 to 64 characters. |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `phone.publish` {#phone-publish}

**Scopes:** `ui` `cli` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket.

**Command line:** [`nh phone publish`](/docs/reference/cli/commands/phone#phone-publish)

An empty result or empty params: `{}`.

### Params

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

### Result

[`PhoneStatusValue`](/docs/reference/api/types#phonestatusvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `hint` | string \| `null` |  | What the person can do when the listener cannot serve a phone. `phone.pair_begin` refuses while there is a hint. |
| `listening` | boolean | yes | Whether the listener serves its port. |
| `origin` | string \| `null` |  | The HTTPS origin that a phone uses, for example `https://studio.example.ts.net:8443`. |
| `port` | integer (uint16) \| `null` |  | The port of `127.0.0.1`, when the listener has one. |
| `summary` | string |  | One line about the listener, for the settings window and the doctor, for example "Published with Tailscale at https://studio.example.ts.net:8443.". Default: `""`. |
| `tailscale` | [`TailscaleStatus`](/docs/reference/api/types#tailscalestatus) \| `null` |  | Tailscale. `null` when the listener does not use it: the listener is off, `phone.url` names the origin, or the run is a development run. |
| `vapid_key` | string \| `null` |  | The public VAPID key of this core: the uncompressed P-256 point, base64url. The phone app subscribes to push with it. `null` while the listener does not run. |

## `phone.unpublish` {#phone-unpublish}

**Scopes:** `ui` `cli` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket.

**Command line:** [`nh phone unpublish`](/docs/reference/cli/commands/phone#phone-unpublish)

An empty result or empty params: `{}`.

### Params

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

### Result

[`PhoneStatusValue`](/docs/reference/api/types#phonestatusvalue)

| Field | Type | Required | Description |
|---|---|---|---|
| `hint` | string \| `null` |  | What the person can do when the listener cannot serve a phone. `phone.pair_begin` refuses while there is a hint. |
| `listening` | boolean | yes | Whether the listener serves its port. |
| `origin` | string \| `null` |  | The HTTPS origin that a phone uses, for example `https://studio.example.ts.net:8443`. |
| `port` | integer (uint16) \| `null` |  | The port of `127.0.0.1`, when the listener has one. |
| `summary` | string |  | One line about the listener, for the settings window and the doctor, for example "Published with Tailscale at https://studio.example.ts.net:8443.". Default: `""`. |
| `tailscale` | [`TailscaleStatus`](/docs/reference/api/types#tailscalestatus) \| `null` |  | Tailscale. `null` when the listener does not use it: the listener is off, `phone.url` names the origin, or the run is a development run. |
| `vapid_key` | string \| `null` |  | The public VAPID key of this core: the uncompressed P-256 point, base64url. The phone app subscribes to push with it. `null` while the listener does not run. |

## `phone.push_subscribe` {#phone-push-subscribe}

**Scopes:** `phone` — a paired phone, over the listener of the phone.

`phone.push_subscribe`: the push subscription of the phone that calls. Only a phone calls it, for itself. A new subscription replaces the old one.

### Params

[`PhonePushSubscribe`](/docs/reference/api/types#phonepushsubscribe)

| Field | Type | Required | Description |
|---|---|---|---|
| `auth` | string | yes | The authentication secret of the browser (16 bytes), base64url. |
| `endpoint` | string | yes | The endpoint of the push service. It must be on the allowlist of push services. |
| `p256dh` | string | yes | The public key of the browser (P-256, uncompressed), base64url. |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `phone.push_unsubscribe` {#phone-push-unsubscribe}

**Scopes:** `phone` — a paired phone, over the listener of the phone.

An empty result or empty params: `{}`.

### Params

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `phone.push_test` {#phone-push-test}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh phone push-test`](/docs/reference/cli/commands/phone#phone-push-test)

`phone.push_test`: sends the push "Test notification" to a phone.

### Params

[`PhonePushTest`](/docs/reference/api/types#phonepushtest)

| Field | Type | Required | Description |
|---|---|---|---|
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `phone.visible` {#phone-visible}

**Scopes:** `phone` — a paired phone, over the listener of the phone.

`phone.visible`: the phone app of the phone that calls is open and visible, or not. The app calls it on each `visibilitychange`. A visible phone gets no push. The core keeps it in memory for the connection, and the end of the connection makes it false.

### Params

[`PhoneVisible`](/docs/reference/api/types#phonevisible)

| Field | Type | Required | Description |
|---|---|---|---|
| `visible` | boolean | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `phone.window` {#phone-window}

**Scopes:** `phone` — a paired phone, over the listener of the phone.

An empty result or empty params: `{}`.

### Params

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

### Result

[`PhoneWindowResult`](/docs/reference/api/types#phonewindowresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |
