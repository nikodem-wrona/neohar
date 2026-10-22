---
title: "favorite.*"
description: "The favorite folders that the form of a new task offers."
sidebar_position: 17
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The favorite folders that the form of a new task offers.

| Method | Scopes | Params | Result |
|---|---|---|---|
| [`favorite.list`](#favorite-list) | `ui` `cli` `phone` | [`FavoriteList`](/docs/reference/api/types#favoritelist) | [`FavoriteListResult`](/docs/reference/api/types#favoritelistresult) |
| [`favorite.add`](#favorite-add) | `ui` `cli` `phone` | [`FavoriteAdd`](/docs/reference/api/types#favoriteadd) | [`FavoriteAdded`](/docs/reference/api/types#favoriteadded) |
| [`favorite.remove`](#favorite-remove) | `ui` `cli` `phone` | [`FavoriteRemove`](/docs/reference/api/types#favoriteremove) | [`Empty`](/docs/reference/api/types#empty) |

## `favorite.list` {#favorite-list}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh favorite list`](/docs/reference/cli/commands/favorite#favorite-list)

### Params

[`FavoriteList`](/docs/reference/api/types#favoritelist)

An object with no fields: `{}`.

### Result

[`FavoriteListResult`](/docs/reference/api/types#favoritelistresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `favorites` | [`FavoriteValue`](/docs/reference/api/types#favoritevalue)[] | yes |  |

## `favorite.add` {#favorite-add}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh favorite add`](/docs/reference/cli/commands/favorite#favorite-add)

Adds a folder to the favorites. The core keeps the root of its git repository. A favorite that is there already stays as it is.

### Params

[`FavoriteAdd`](/docs/reference/api/types#favoriteadd)

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | An absolute path or a path under `~`, or a workspace (`w3`) for its root. |

### Result

[`FavoriteAdded`](/docs/reference/api/types#favoriteadded)

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | The root of the repository that the core keeps: the key of its row in `favorites`. |

## `favorite.remove` {#favorite-remove}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh favorite remove`](/docs/reference/cli/commands/favorite#favorite-remove)

Removes a folder from the favorites: the path of a favorite, or a folder in its repository.

### Params

[`FavoriteRemove`](/docs/reference/api/types#favoriteremove)

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.
