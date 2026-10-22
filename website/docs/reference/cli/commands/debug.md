---
title: "nh debug"
description: "Tools for a bug report"
sidebar_position: 3
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

Tools for a bug report

```text
Tools for a bug report

Usage: nh debug [OPTIONS] <COMMAND>

Commands:
  bundle  Write a zip with the logs, the doctor, the configuration and the recent events. Secrets are removed from every text

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh debug bundle` {#debug-bundle}

Write a zip with the logs, the doctor, the configuration and the recent events. Secrets are removed from every text

**API method:** [`debug.bundle`](/docs/reference/api/methods/debug#debug-bundle)

```text
Write a zip with the logs, the doctor, the configuration and the recent events. Secrets are removed from every text

Usage: nh debug bundle [OPTIONS]

Options:
      --out <FILE>          The file to write. The default is neohar-debug-<time>.zip in this directory
      --session <NAME>      The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --include-scrollback  Add the text of the panes
      --socket <PATH>       The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>     Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json                Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>   How long to wait for the core to answer, in seconds [default: 30]
  -h, --help                Print help
```
