---
title: "nh machine"
description: "This machine and the machines it connects to"
sidebar_position: 7
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

This machine and the machines it connects to

```text
This machine and the machines it connects to

Usage: nh machine [OPTIONS] <COMMAND>

Commands:
  info         Show the machine
  list         List this machine and the machines of config.toml with their links
  connect      Open the link to a machine of config.toml
  disconnect   End the link to a machine. Its programs keep running
  open-window  Open a window of a machine, and connect it first

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh machine info` {#machine-info}

Show the machine

**API method:** [`machine.info`](/docs/reference/api/methods/machine#machine-info)

```text
Show the machine

Usage: nh machine info [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh machine list` {#machine-list}

List this machine and the machines of config.toml with their links

**API method:** [`machine.list`](/docs/reference/api/methods/machine#machine-list)

```text
List this machine and the machines of config.toml with their links

Usage: nh machine list [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh machine connect` {#machine-connect}

Open the link to a machine of config.toml

**API method:** [`machine.connect`](/docs/reference/api/methods/machine#machine-connect)

```text
Open the link to a machine of config.toml

Usage: nh machine connect [OPTIONS] <MACHINE>

Arguments:
  <MACHINE>  

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh machine disconnect` {#machine-disconnect}

End the link to a machine. Its programs keep running

**API method:** [`machine.disconnect`](/docs/reference/api/methods/machine#machine-disconnect)

```text
End the link to a machine. Its programs keep running

Usage: nh machine disconnect [OPTIONS] <MACHINE>

Arguments:
  <MACHINE>  

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh machine open-window` {#machine-open-window}

Open a window of a machine, and connect it first

**API method:** [`machine.open_window`](/docs/reference/api/methods/machine#machine-open-window)

```text
Open a window of a machine, and connect it first

Usage: nh machine open-window [OPTIONS] <MACHINE>

Arguments:
  <MACHINE>  

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
