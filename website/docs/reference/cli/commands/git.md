---
title: "nh git"
description: "Git repositories, worktrees, branches and history, as the Git view shows them"
sidebar_position: 18
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

Git repositories, worktrees, branches and history, as the Git view shows them

```text
Git repositories, worktrees, branches and history, as the Git view shows them

Usage: nh git [OPTIONS] <COMMAND>

Commands:
  repos      List the git repositories that neohar knows: of the workspaces, the panes and the ones you added
  add        Add the repository of a folder, or of the current directory, to the Repository menu
  status     Show the branch, its upstream, the agents and the changed files of the worktree of the current directory
  worktrees  List the worktrees of the repository with their branches and agents
  branches   List the local and remote branches: the current one, the recent ones, then the others
  log        List the commits of HEAD, the newest first
  show       Show a commit with its message and its files
  diff       Show the diff of a changed file, or of a file of a commit with --commit
  switch     Switch the worktree to a branch. It asks first when an agent works in the worktree: --force switches anyway
  unstash    Put the changes of a stash that a switch left back into the worktree
  fetch      Fetch the remote of the branch, as a job
  pull       Move the branch to its upstream with a fast-forward only, as a job

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh git repos` {#git-repos}

List the git repositories that neohar knows: of the workspaces, the panes and the ones you added

**API method:** [`git.repos`](/docs/reference/api/methods/git#git-repos)

```text
List the git repositories that neohar knows: of the workspaces, the panes and the ones you added

Usage: nh git repos [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh git add` {#git-add}

Add the repository of a folder, or of the current directory, to the Repository menu

```text
Add the repository of a folder, or of the current directory, to the Repository menu

Usage: nh git add [OPTIONS] [PATH]

Arguments:
  [PATH]  An absolute path in a git worktree.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh git status` {#git-status}

Show the branch, its upstream, the agents and the changed files of the worktree of the current directory

**API method:** [`git.status`](/docs/reference/api/methods/git#git-status)

```text
Show the branch, its upstream, the agents and the changed files of the worktree of the current directory

Usage: nh git status [OPTIONS]

Options:
      --path <TEXT>        The worktree that holds this absolute path. The CLI gives the current directory.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --window <TEXT>      The worktree that the Git view of this window shows.
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh git worktrees` {#git-worktrees}

List the worktrees of the repository with their branches and agents

**API method:** [`git.worktrees`](/docs/reference/api/methods/git#git-worktrees)

```text
List the worktrees of the repository with their branches and agents

Usage: nh git worktrees [OPTIONS]

Options:
      --path <TEXT>        The worktree that holds this absolute path. The CLI gives the current directory.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --window <TEXT>      The worktree that the Git view of this window shows.
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh git branches` {#git-branches}

List the local and remote branches: the current one, the recent ones, then the others

**API method:** [`git.branches`](/docs/reference/api/methods/git#git-branches)

```text
List the local and remote branches: the current one, the recent ones, then the others

Usage: nh git branches [OPTIONS]

Options:
      --path <TEXT>        The worktree that holds this absolute path. The CLI gives the current directory.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --window <TEXT>      The worktree that the Git view of this window shows.
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh git log` {#git-log}

List the commits of HEAD, the newest first

**API method:** [`git.log`](/docs/reference/api/methods/git#git-log)

```text
List the commits of HEAD, the newest first

Usage: nh git log [OPTIONS]

Options:
      --limit <NUMBER>     The most commits of the page, at most 200.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --path <TEXT>        The worktree that holds this absolute path. The CLI gives the current directory.
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --skip <NUMBER>      The commits to leave out at the start: the number that the pages before had.
      --json               Print JSON. This is the default when stdout is not a terminal
      --window <TEXT>      The worktree that the Git view of this window shows.
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh git show` {#git-show}

Show a commit with its message and its files

**API method:** [`git.show`](/docs/reference/api/methods/git#git-show)

```text
Show a commit with its message and its files

Usage: nh git show [OPTIONS] <COMMIT>

Arguments:
  <COMMIT>  The full or the short hash of the commit.

Options:
      --path <TEXT>        The worktree that holds this absolute path. The CLI gives the current directory.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --window <TEXT>      The worktree that the Git view of this window shows.
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh git diff` {#git-diff}

Show the diff of a changed file, or of a file of a commit with --commit

**API method:** [`git.diff`](/docs/reference/api/methods/git#git-diff)

```text
Show the diff of a changed file, or of a file of a commit with --commit

Usage: nh git diff [OPTIONS] <FILE>

Arguments:
  <FILE>  The path of the file from the root of the worktree.

Options:
      --commit <TEXT>      
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --path <TEXT>        The worktree that holds this absolute path. The CLI gives the current directory.
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --window <TEXT>      The worktree that the Git view of this window shows.
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh git switch` {#git-switch}

Switch the worktree to a branch. It asks first when an agent works in the worktree: --force switches anyway

**API method:** [`git.switch`](/docs/reference/api/methods/git#git-switch)

```text
Switch the worktree to a branch. It asks first when an agent works in the worktree: --force switches anyway

Usage: nh git switch [OPTIONS] <BRANCH>

Arguments:
  <BRANCH>  A branch of `git_branches`: its reference (`refs/heads/main`) or its name (`main`,

Options:
      --changes <TEXT>     What happens to the changes of the worktree: `bring` or `leave`. A worktree with
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --force              Switch also when an agent works in the worktree.
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --path <TEXT>        The worktree that holds this absolute path. The CLI gives the current directory.
      --json               Print JSON. This is the default when stdout is not a terminal
      --window <TEXT>      The worktree that the Git view of this window shows.
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh git unstash` {#git-unstash}

Put the changes of a stash that a switch left back into the worktree

**API method:** [`git.unstash`](/docs/reference/api/methods/git#git-unstash)

```text
Put the changes of a stash that a switch left back into the worktree

Usage: nh git unstash [OPTIONS] <STASH>

Arguments:
  <STASH>  The commit of the stash, as `stash` of `git_head` gives it.

Options:
      --force              Put the changes back also when an agent works in the worktree.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --path <TEXT>        The worktree that holds this absolute path. The CLI gives the current directory.
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --window <TEXT>      The worktree that the Git view of this window shows.
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh git fetch` {#git-fetch}

Fetch the remote of the branch, as a job

**API method:** [`git.fetch`](/docs/reference/api/methods/git#git-fetch)

```text
Fetch the remote of the branch, as a job

Usage: nh git fetch [OPTIONS]

Options:
      --path <TEXT>        The worktree that holds this absolute path. The CLI gives the current directory.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --window <TEXT>      The worktree that the Git view of this window shows.
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh git pull` {#git-pull}

Move the branch to its upstream with a fast-forward only, as a job

**API method:** [`git.pull`](/docs/reference/api/methods/git#git-pull)

```text
Move the branch to its upstream with a fast-forward only, as a job

Usage: nh git pull [OPTIONS]

Options:
      --force              Pull also when an agent works in the worktree.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --path <TEXT>        The worktree that holds this absolute path. The CLI gives the current directory.
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --window <TEXT>      The worktree that the Git view of this window shows.
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
