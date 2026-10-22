---
title: "nh favorite"
description: "Favorite folders for new tasks"
sidebar_position: 17
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

Favorite folders for new tasks

```text
Favorite folders for new tasks

Usage: nh favorite [OPTIONS] <COMMAND>

Commands:
  list    List the favorite folders, the one used last first
  add     Add the repository of a folder, or of the current directory, to the favorites
  remove  Remove a folder, or the current directory, from the favorites

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh favorite list` {#favorite-list}

List the favorite folders, the one used last first

**API method:** [`favorite.list`](/docs/reference/api/methods/favorite#favorite-list)

```text
List the favorite folders, the one used last first

Usage: nh favorite list [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh favorite add` {#favorite-add}

Add the repository of a folder, or of the current directory, to the favorites

**API method:** [`favorite.add`](/docs/reference/api/methods/favorite#favorite-add)

```text
Add the repository of a folder, or of the current directory, to the favorites

Usage: nh favorite add [OPTIONS] [PATH]

Arguments:
  [PATH]  An absolute path or a path under `~`, or a workspace (`w3`) for its root.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh favorite remove` {#favorite-remove}

Remove a folder, or the current directory, from the favorites

**API method:** [`favorite.remove`](/docs/reference/api/methods/favorite#favorite-remove)

```text
Remove a folder, or the current directory, from the favorites

Usage: nh favorite remove [OPTIONS] [PATH]

Arguments:
  [PATH]  

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
