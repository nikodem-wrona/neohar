---
title: "git.*"
description: "The git methods."
sidebar_position: 16
toc_max_heading_level: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::



| Method | Scopes | Params | Result |
|---|---|---|---|
| [`git.follow`](#git-follow) | `ui` `phone` | [`GitFollow`](/docs/reference/api/types#gitfollow) | [`Empty`](/docs/reference/api/types#empty) |
| [`git.repo_add`](#git-repo-add) | `ui` `cli` `phone` | [`GitRepoAdd`](/docs/reference/api/types#gitrepoadd) | [`Empty`](/docs/reference/api/types#empty) |
| [`git.repos`](#git-repos) | `ui` `cli` `phone` | [`Empty`](/docs/reference/api/types#empty) | [`GitRepoList`](/docs/reference/api/types#gitrepolist) |
| [`git.status`](#git-status) | `ui` `cli` `phone` | [`GitPlace`](/docs/reference/api/types#gitplace) | [`GitStatus`](/docs/reference/api/types#gitstatus) |
| [`git.worktrees`](#git-worktrees) | `ui` `cli` `phone` | [`GitPlace`](/docs/reference/api/types#gitplace) | [`GitWorktreeList`](/docs/reference/api/types#gitworktreelist) |
| [`git.branches`](#git-branches) | `ui` `cli` `phone` | [`GitPlace`](/docs/reference/api/types#gitplace) | [`GitBranchList`](/docs/reference/api/types#gitbranchlist) |
| [`git.log`](#git-log) | `ui` `cli` `phone` | [`GitLog`](/docs/reference/api/types#gitlog) | [`GitLogResult`](/docs/reference/api/types#gitlogresult) |
| [`git.show`](#git-show) | `ui` `cli` `phone` | [`GitShow`](/docs/reference/api/types#gitshow) | [`GitShown`](/docs/reference/api/types#gitshown) |
| [`git.diff`](#git-diff) | `ui` `cli` `phone` | [`GitDiff`](/docs/reference/api/types#gitdiff) | [`FileDiff`](/docs/reference/api/types#filediff) |

## `git.follow` {#git-follow}

**Scopes:** `ui` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; a paired phone, over the listener of the phone.

`git.follow`: the Git view of a window shows the git worktree that holds `path`. The core fills `git_head:<window>`, `git_worktrees:<window>`, `git_branches:<window>` and `git_changes:<window>` while the window follows one of them, and looks again when the worktree or its git directory changes.

### Params

[`GitFollow`](/docs/reference/api/types#gitfollow)

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | An absolute path in the worktree: the folder of a pane, or a row of `git_worktrees`. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `git.repo_add` {#git-repo-add}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

`git.repo_add`: `git_repos` keeps the repository that holds `path`, also when no workspace and no pane is in it.

### Params

[`GitRepoAdd`](/docs/reference/api/types#gitrepoadd)

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes | An absolute path in a git worktree. |

### Result

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

## `git.repos` {#git-repos}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh git repos`](/docs/reference/cli/commands/git#git-repos)

An empty result or empty params: `{}`.

### Params

[`Empty`](/docs/reference/api/types#empty)

An object with no fields: `{}`.

### Result

[`GitRepoList`](/docs/reference/api/types#gitrepolist)

| Field | Type | Required | Description |
|---|---|---|---|
| `repos` | [`GitRepoValue`](/docs/reference/api/types#gitrepovalue)[] | yes |  |

## `git.status` {#git-status}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh git status`](/docs/reference/cli/commands/git#git-status)

The worktree of a read: the one that the Git view of `window` follows, or the one that holds `path`.

### Params

[`GitPlace`](/docs/reference/api/types#gitplace)

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string \| `null` |  | An absolute path in the worktree. The CLI gives the current directory. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  |  |

### Result

[`GitStatus`](/docs/reference/api/types#gitstatus)

| Field | Type | Required | Description |
|---|---|---|---|
| `changes` | [`ChangeRow`](/docs/reference/api/types#changerow)[] | yes |  |
| `head` | [`GitHead`](/docs/reference/api/types#githead) | yes |  |

## `git.worktrees` {#git-worktrees}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh git worktrees`](/docs/reference/cli/commands/git#git-worktrees)

The worktree of a read: the one that the Git view of `window` follows, or the one that holds `path`.

### Params

[`GitPlace`](/docs/reference/api/types#gitplace)

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string \| `null` |  | An absolute path in the worktree. The CLI gives the current directory. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  |  |

### Result

[`GitWorktreeList`](/docs/reference/api/types#gitworktreelist)

| Field | Type | Required | Description |
|---|---|---|---|
| `worktrees` | [`GitWorktreeValue`](/docs/reference/api/types#gitworktreevalue)[] | yes |  |

## `git.branches` {#git-branches}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh git branches`](/docs/reference/cli/commands/git#git-branches)

The worktree of a read: the one that the Git view of `window` follows, or the one that holds `path`.

### Params

[`GitPlace`](/docs/reference/api/types#gitplace)

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string \| `null` |  | An absolute path in the worktree. The CLI gives the current directory. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  |  |

### Result

[`GitBranchList`](/docs/reference/api/types#gitbranchlist)

| Field | Type | Required | Description |
|---|---|---|---|
| `branches` | [`GitBranchValue`](/docs/reference/api/types#gitbranchvalue)[] | yes |  |

## `git.log` {#git-log}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh git log`](/docs/reference/cli/commands/git#git-log)

`git.log`: the commits of `HEAD`, the newest first, in pages.

### Params

[`GitLog`](/docs/reference/api/types#gitlog)

| Field | Type | Required | Description |
|---|---|---|---|
| `limit` | integer (uint32) |  | The most commits of the page, at most 200. Default: `200`. |
| `path` | string \| `null` |  |  |
| `skip` | integer (uint32) |  | The commits to leave out at the start: the number that the pages before had. Default: `0`. |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  |  |

### Result

[`GitLogResult`](/docs/reference/api/types#gitlogresult)

| Field | Type | Required | Description |
|---|---|---|---|
| `commits` | [`GitCommit`](/docs/reference/api/types#gitcommit)[] | yes |  |
| `head` | string \| `null` |  | The commit of `HEAD` when the page was read. Another value than the page before means that the history changed: read it again from the start. |
| `more` | boolean | yes | More commits follow this page. |

## `git.show` {#git-show}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh git show`](/docs/reference/cli/commands/git#git-show)

`git.show`: one commit with its message and its files.

### Params

[`GitShow`](/docs/reference/api/types#gitshow)

| Field | Type | Required | Description |
|---|---|---|---|
| `commit` | string | yes | The full or the short hash of the commit. |
| `path` | string \| `null` |  |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  |  |

### Result

[`GitShown`](/docs/reference/api/types#gitshown)

| Field | Type | Required | Description |
|---|---|---|---|
| `body` | string | yes | The message after the subject, without the blank line between them. |
| `commit` | [`GitCommit`](/docs/reference/api/types#gitcommit) | yes |  |
| `email` | string | yes | The e-mail address of the author. |
| `files` | [`ChangeRow`](/docs/reference/api/types#changerow)[] | yes | The changed files, in the byte order of the path. `stamp` is 0. |
| `parents` | string[] | yes | The short hashes of the parents. A merge has two or more. The files are the changes from the first parent. |
| `stats` | [`DiffStats`](/docs/reference/api/types#diffstats) | yes | The lines of all files. |
| `truncated` | boolean |  | The commit has more than 1,000 files, and `files` has the first 1,000. Default: `false`. |

## `git.diff` {#git-diff}

**Scopes:** `ui` `cli` `phone` — the UI, over the `rpc` command of the Tauri host or the WebSocket; the CLI and any other client on the API socket; a paired phone, over the listener of the phone.

**Command line:** [`nh git diff`](/docs/reference/cli/commands/git#git-diff)

`git.diff`: the diff of one file. Without `commit`, the file is a row of the changes of the worktree, from `HEAD`. With `commit`, it is a file of that commit, from its first parent.

### Params

[`GitDiff`](/docs/reference/api/types#gitdiff)

| Field | Type | Required | Description |
|---|---|---|---|
| `commit` | string \| `null` |  |  |
| `file` | string | yes | The path of the file from the root of the worktree. |
| `path` | string \| `null` |  |  |
| `window` | [`WindowId`](/docs/reference/api/types#windowid) \| `null` |  |  |

### Result

[`FileDiff`](/docs/reference/api/types#filediff)

| Field | Type | Required | Description |
|---|---|---|---|
| `hunks` | [`Hunk`](/docs/reference/api/types#hunk)[] | yes |  |
| `path` | string | yes |  |
| `stats` | [`DiffStats`](/docs/reference/api/types#diffstats) | yes |  |
| `status` | [`FileStatus`](/docs/reference/api/types#filestatus) | yes |  |
| `truncated` | boolean |  | The diff of this file was cut at 2,000 lines. The UI offers "open full". Default: `false`. |
