---
title: "nh harness"
description: "Claude Code and Codex on this machine: installed, version, login"
sidebar_position: 14
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

Claude Code and Codex on this machine: installed, version, login

```text
Claude Code and Codex on this machine: installed, version, login

Usage: nh harness [OPTIONS] <COMMAND>

Commands:
  refresh  Find the harnesses again and ask each one for its login state

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh harness refresh` {#harness-refresh}

Find the harnesses again and ask each one for its login state

**API method:** [`harness.refresh`](/docs/reference/api/methods/harness#harness-refresh)

```text
Find the harnesses again and ask each one for its login state

Usage: nh harness refresh [OPTIONS] [HARNESS]

Arguments:
  [HARNESS]  The id of one manifest, for example `codex`. Without it, every harness.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
