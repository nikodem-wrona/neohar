---
title: "nh integration"
description: "The hooks of the harnesses"
sidebar_position: 21
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

The hooks of the harnesses

```text
The hooks of the harnesses

Usage: nh integration [OPTIONS] <COMMAND>

Commands:
  list          List the integrations
  install       Install the hooks of a harness
  status        Show what is installed
  uninstall     Remove the hooks of a harness
  doctor        Check a harness with a live round trip
  capabilities  Show what each harness version can do

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh integration list` {#integration-list}

List the integrations

**API method:** [`integration.list`](/docs/reference/api/methods/integration#integration-list)

```text
List the integrations

Usage: nh integration list [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh integration install` {#integration-install}

Install the hooks of a harness

**API method:** [`integration.install`](/docs/reference/api/methods/integration#integration-install)

```text
Install the hooks of a harness

Usage: nh integration install [OPTIONS] <TARGET>

Arguments:
  <TARGET>  Which integration. `All` means both harnesses.

Options:
      --dry-run              
      --session <NAME>       The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --project-root <TEXT>  The repository for the scope `project`. The CLI gives its working directory.
      --socket <PATH>        The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>      Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --scope <TEXT>         
      --json                 Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>    How long to wait for the core to answer, in seconds [default: 30]
  -h, --help                 Print help
```

## `nh integration status` {#integration-status}

Show what is installed

**API method:** [`integration.status`](/docs/reference/api/methods/integration#integration-status)

```text
Show what is installed

Usage: nh integration status [OPTIONS] [TARGET]

Arguments:
  [TARGET]  

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh integration uninstall` {#integration-uninstall}

Remove the hooks of a harness

**API method:** [`integration.uninstall`](/docs/reference/api/methods/integration#integration-uninstall)

```text
Remove the hooks of a harness

Usage: nh integration uninstall [OPTIONS] <TARGET>

Arguments:
  <TARGET>  Which integration. `All` means both harnesses.

Options:
      --project-root <TEXT>  The repository for the scope `project`. The CLI gives its working directory.
      --session <NAME>       The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --scope <TEXT>         
      --socket <PATH>        The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>      Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json                 Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>    How long to wait for the core to answer, in seconds [default: 30]
  -h, --help                 Print help
```

## `nh integration doctor` {#integration-doctor}

Check a harness with a live round trip

**API method:** [`integration.doctor`](/docs/reference/api/methods/integration#integration-doctor)

```text
Check a harness with a live round trip

Usage: nh integration doctor [OPTIONS] [TARGET]

Arguments:
  [TARGET]  

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh integration capabilities` {#integration-capabilities}

Show what each harness version can do

**API method:** [`integration.capabilities`](/docs/reference/api/methods/integration#integration-capabilities)

```text
Show what each harness version can do

Usage: nh integration capabilities [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
