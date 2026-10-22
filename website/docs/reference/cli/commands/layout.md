---
title: "nh layout"
description: "The layout of the panes of a tab"
sidebar_position: 11
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

The layout of the panes of a tab

```text
The layout of the panes of a tab

Usage: nh layout [OPTIONS] <COMMAND>

Commands:
  set-ratios  Set the shares of a split
  equalize    Give the panes of a split equal shares
  zoom        Zoom a pane, or show all panes

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh layout set-ratios` {#layout-set-ratios}

Set the shares of a split

**API method:** [`layout.set_ratios`](/docs/reference/api/methods/layout#layout-set-ratios)

```text
Set the shares of a split

Usage: nh layout set-ratios [OPTIONS] <TAB> <PATH>

Arguments:
  <TAB>   A reference to a tab: its ULID, or its short handle.
  <PATH>  The split node, as the indices of the children from the root. `[]` is the root.

Options:
      --ratios <JSON>      
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh layout equalize` {#layout-equalize}

Give the panes of a split equal shares

**API method:** [`layout.equalize`](/docs/reference/api/methods/layout#layout-equalize)

```text
Give the panes of a split equal shares

Usage: nh layout equalize [OPTIONS] <TAB>

Arguments:
  <TAB>  A reference to a tab: its ULID, or its short handle.

Options:
      --path <JSON>        
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh layout zoom` {#layout-zoom}

Zoom a pane, or show all panes

**API method:** [`layout.zoom`](/docs/reference/api/methods/layout#layout-zoom)

```text
Zoom a pane, or show all panes

Usage: nh layout zoom [OPTIONS] <TAB>

Arguments:
  <TAB>  A reference to a tab: its ULID, or its short handle.

Options:
      --pane <TEXT>        The pane to zoom. Without it the zoom ends.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
