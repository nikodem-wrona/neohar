---
title: "nh group"
description: "Groups of workspaces"
sidebar_position: 9
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

Groups of workspaces

```text
Groups of workspaces

Usage: nh group [OPTIONS] <COMMAND>

Commands:
  create  Make a group
  list    List the groups
  show    Show a group
  rename  Rename a group
  close   Close a group
  move    Move a group

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh group create` {#group-create}

Make a group

**API method:** [`group.create`](/docs/reference/api/methods/group#group-create)

```text
Make a group

Usage: nh group create [OPTIONS] <NAME>

Arguments:
  <NAME>  

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh group list` {#group-list}

List the groups

**API method:** [`group.list`](/docs/reference/api/methods/group#group-list)

```text
List the groups

Usage: nh group list [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh group show` {#group-show}

Show a group

**API method:** [`group.show`](/docs/reference/api/methods/group#group-show)

```text
Show a group

Usage: nh group show [OPTIONS] <GROUP>

Arguments:
  <GROUP>  A reference to a group: its ULID.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh group rename` {#group-rename}

Rename a group

**API method:** [`group.rename`](/docs/reference/api/methods/group#group-rename)

```text
Rename a group

Usage: nh group rename [OPTIONS] <GROUP> <NAME>

Arguments:
  <GROUP>  A reference to a group: its ULID.
  <NAME>   

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh group close` {#group-close}

Close a group

**API method:** [`group.close`](/docs/reference/api/methods/group#group-close)

```text
Close a group

Usage: nh group close [OPTIONS] <GROUP>

Arguments:
  <GROUP>  A reference to a group: its ULID.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh group move` {#group-move}

Move a group

**API method:** [`group.move`](/docs/reference/api/methods/group#group-move)

```text
Move a group

Usage: nh group move [OPTIONS] <GROUP>

Arguments:
  <GROUP>  A reference to a group: its ULID.

Options:
      --before <TEXT>      Put it before this group. Without it the group goes to the end.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
