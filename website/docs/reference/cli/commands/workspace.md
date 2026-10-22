---
title: "nh workspace"
description: "Workspaces"
sidebar_position: 8
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

Workspaces

```text
Workspaces

Usage: nh workspace [OPTIONS] <COMMAND>

Commands:
  create  Make a workspace with one tab
  list    List the workspaces
  show    Show a workspace with its tabs
  rename  Rename a workspace
  close   Close a workspace
  move    Move a workspace to a group or before another

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh workspace create` {#workspace-create}

Make a workspace with one tab

**API method:** [`workspace.create`](/docs/reference/api/methods/workspace#workspace-create)

```text
Make a workspace with one tab

Usage: nh workspace create [OPTIONS] [NAME]

Arguments:
  [NAME]  

Options:
      --command <JSON>     What the first pane runs.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --group <TEXT>       
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --root <TEXT>        The root path. It is the cwd of the first pane.
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh workspace list` {#workspace-list}

List the workspaces

**API method:** [`workspace.list`](/docs/reference/api/methods/workspace#workspace-list)

```text
List the workspaces

Usage: nh workspace list [OPTIONS]

Options:
      --group <TEXT>       
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh workspace show` {#workspace-show}

Show a workspace with its tabs

**API method:** [`workspace.show`](/docs/reference/api/methods/workspace#workspace-show)

```text
Show a workspace with its tabs

Usage: nh workspace show [OPTIONS] <WORKSPACE>

Arguments:
  <WORKSPACE>  A reference to a workspace: its ULID, or its short handle.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh workspace rename` {#workspace-rename}

Rename a workspace

**API method:** [`workspace.rename`](/docs/reference/api/methods/workspace#workspace-rename)

```text
Rename a workspace

Usage: nh workspace rename [OPTIONS] <WORKSPACE> <NAME>

Arguments:
  <WORKSPACE>  A reference to a workspace: its ULID, or its short handle.
  <NAME>       

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh workspace close` {#workspace-close}

Close a workspace

**API method:** [`workspace.close`](/docs/reference/api/methods/workspace#workspace-close)

```text
Close a workspace

Usage: nh workspace close [OPTIONS] <WORKSPACE>

Arguments:
  <WORKSPACE>  A reference to a workspace: its ULID, or its short handle.

Options:
      --force              
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh workspace move` {#workspace-move}

Move a workspace to a group or before another

**API method:** [`workspace.move`](/docs/reference/api/methods/workspace#workspace-move)

```text
Move a workspace to a group or before another

Usage: nh workspace move [OPTIONS] <WORKSPACE>

Arguments:
  <WORKSPACE>  A reference to a workspace: its ULID, or its short handle.

Options:
      --before <TEXT>      Put it before this workspace. Without it the workspace goes to the end.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --group <TEXT>       The group to put it in. Without it the workspace has no group.
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
