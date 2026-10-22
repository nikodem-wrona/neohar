---
title: "nh events"
description: "The log of events"
sidebar_position: 4
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

The log of events

```text
The log of events

Usage: nh events [OPTIONS] <COMMAND>

Commands:
  tail  Print events as JSON lines

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh events tail` {#events-tail}

Print events as JSON lines

```text
Print events as JSON lines

Usage: nh events tail [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --since <CURSOR>     Start at this cursor, as JSON. Without it the log starts at its end
  -f, --follow             Go on when the log is read to its end
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --kind <KIND>        Only this kind of event. A trailing `*` is a prefix, as in `interaction.*`
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --subject <ID>       Only events about this object
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
