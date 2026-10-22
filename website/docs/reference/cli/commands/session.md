---
title: "nh session"
description: "The session of the core"
sidebar_position: 6
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

The session of the core

```text
The session of the core

Usage: nh session [OPTIONS] <COMMAND>

Commands:
  stop-everything  Stop every program of every pane, as "Quit and stop everything" does
  shutdown         End the core of a daemon. The panes keep running
  info             Show the session and its core

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh session stop-everything` {#session-stop-everything}

Stop every program of every pane, as "Quit and stop everything" does

**API method:** [`session.stop_everything`](/docs/reference/api/methods/session#session-stop-everything)

```text
Stop every program of every pane, as "Quit and stop everything" does

Usage: nh session stop-everything [OPTIONS]

Options:
      --force              Skip the grace period: the agents get no time to end by themselves, and a program that
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh session shutdown` {#session-shutdown}

End the core of a daemon. The panes keep running

**API method:** [`session.shutdown`](/docs/reference/api/methods/session#session-shutdown)

```text
End the core of a daemon. The panes keep running

Usage: nh session shutdown [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh session info` {#session-info}

Show the session and its core

**API method:** [`session.info`](/docs/reference/api/methods/session#session-info)

```text
Show the session and its core

Usage: nh session info [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
