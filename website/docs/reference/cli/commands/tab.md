---
title: "nh tab"
description: "Tabs"
sidebar_position: 10
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

Tabs

```text
Tabs

Usage: nh tab [OPTIONS] <COMMAND>

Commands:
  create  Make a tab in a workspace
  rename  Rename a tab
  close   Close a tab
  move    Move a tab
  select  Show a tab in a window

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh tab create` {#tab-create}

Make a tab in a workspace

**API method:** [`tab.create`](/docs/reference/api/methods/tab#tab-create)

```text
Make a tab in a workspace

Usage: nh tab create [OPTIONS] <WORKSPACE>

Arguments:
  <WORKSPACE>  A reference to a workspace: its ULID, or its short handle.

Options:
      --command <JSON>     
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --name <TEXT>        
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh tab rename` {#tab-rename}

Rename a tab

**API method:** [`tab.rename`](/docs/reference/api/methods/tab#tab-rename)

```text
Rename a tab

Usage: nh tab rename [OPTIONS] <TAB> <NAME>

Arguments:
  <TAB>   A reference to a tab: its ULID, or its short handle.
  <NAME>  

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh tab close` {#tab-close}

Close a tab

**API method:** [`tab.close`](/docs/reference/api/methods/tab#tab-close)

```text
Close a tab

Usage: nh tab close [OPTIONS] <TAB>

Arguments:
  <TAB>  A reference to a tab: its ULID, or its short handle.

Options:
      --force              
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh tab move` {#tab-move}

Move a tab

**API method:** [`tab.move`](/docs/reference/api/methods/tab#tab-move)

```text
Move a tab

Usage: nh tab move [OPTIONS] <TAB>

Arguments:
  <TAB>  A reference to a tab: its ULID, or its short handle.

Options:
      --index <NUMBER>     The place among the tabs, counted from 0. Without it the tab goes to the end.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --workspace <TEXT>   The workspace to move it to. Without it the tab stays in its workspace.
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh tab select` {#tab-select}

Show a tab in a window

**API method:** [`tab.select`](/docs/reference/api/methods/tab#tab-select)

```text
Show a tab in a window

Usage: nh tab select [OPTIONS] <TAB>

Arguments:
  <TAB>  A reference to a tab: its ULID, or its short handle.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --window <TEXT>      
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
