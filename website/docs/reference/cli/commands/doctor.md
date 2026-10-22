---
title: "nh doctor"
description: "Check the install and the core: the symlink, the sockets, the integrations of the harnesses, the storage. It works also when neohar does not run"
sidebar_position: 2
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

Check the install and the core: the symlink, the sockets, the integrations of the harnesses, the storage. It works also when neohar does not run

```text
Check the install and the core: the symlink, the sockets, the integrations of the harnesses, the storage. It works also when neohar does not run

Usage: nh doctor [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
