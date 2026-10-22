---
title: "nh api"
description: "Call the API directly"
sidebar_position: 1
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

Call the API directly

```text
Call the API directly

Usage: nh api [OPTIONS] <COMMAND>

Commands:
  call    Call a method and print its result as JSON
  schema  Print the contract of the API as one JSON document

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh api call` {#api-call}

Call a method and print its result as JSON

```text
Call a method and print its result as JSON.

The params are a JSON object, in the argument or, with `-`, on stdin. Without them the call has no params.

Usage: nh api call [OPTIONS] <METHOD> [PARAMS]

Arguments:
  <METHOD>
          The name of the method, for example `session.info`

  [PARAMS]
          The params as JSON, or `-` to read them from stdin

Options:
      --session <NAME>
          The session to talk to. The default is $NEOHAR_SESSION, or `default`

      --socket <PATH>
          The API socket to use. It wins over --session and $NEOHAR_SOCKET

      --machine <LABEL>
          Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same

      --json
          Print JSON. This is the default when stdout is not a terminal

      --timeout <SECONDS>
          How long to wait for the core to answer, in seconds
          
          [default: 30]

  -h, --help
          Print help (see a summary with '-h')
```

## `nh api schema` {#api-schema}

Print the contract of the API as one JSON document

```text
Print the contract of the API as one JSON document

Usage: nh api schema [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
