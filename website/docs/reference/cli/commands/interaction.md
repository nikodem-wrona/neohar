---
title: "nh interaction"
description: "What agents ask: approvals, questions, plans"
sidebar_position: 15
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

What agents ask: approvals, questions, plans

```text
What agents ask: approvals, questions, plans

Usage: nh interaction [OPTIONS] <COMMAND>

Commands:
  list     List the interactions
  show     Show an interaction
  answer   Answer an interaction: allow, allow-always or deny for an approval
  dismiss  Close an interaction without an answer
  next     Show the interaction that is next
  reveal   Show an interaction in a window

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh interaction list` {#interaction-list}

List the interactions

**API method:** [`interaction.list`](/docs/reference/api/methods/interaction#interaction-list)

```text
List the interactions

Usage: nh interaction list [OPTIONS]

Options:
      --limit <NUMBER>     
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --no-open-only       Only the open ones. It is the default.
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --pane <TEXT>        
      --json               Print JSON. This is the default when stdout is not a terminal
      --run <TEXT>         
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh interaction show` {#interaction-show}

Show an interaction

**API method:** [`interaction.show`](/docs/reference/api/methods/interaction#interaction-show)

```text
Show an interaction

Usage: nh interaction show [OPTIONS] <INTERACTION>

Arguments:
  <INTERACTION>  A reference to an interaction: its ULID, or its short handle.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh interaction answer` {#interaction-answer}

Answer an interaction: allow, allow-always or deny for an approval

**API method:** [`interaction.answer`](/docs/reference/api/methods/interaction#interaction-answer)

```text
Answer an interaction: allow, allow-always or deny for an approval

Usage: nh interaction answer [OPTIONS] <INTERACTION> <CHOICE>

Arguments:
  <INTERACTION>  A reference to an interaction: its ULID, or its short handle.
  <CHOICE>       

Options:
      --message <message>    The text for a deny: the model sees it
      --session <NAME>       The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>        The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --stop                 With deny: the turn ends
      --machine <LABEL>      Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --persist              With allow-always: write the rule to the settings of the harness
      --decision <decision>  The whole decision as JSON, for a question or a plan review
      --json                 Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>    How long to wait for the core to answer, in seconds [default: 30]
  -h, --help                 Print help
```

## `nh interaction dismiss` {#interaction-dismiss}

Close an interaction without an answer

**API method:** [`interaction.dismiss`](/docs/reference/api/methods/interaction#interaction-dismiss)

```text
Close an interaction without an answer

Usage: nh interaction dismiss [OPTIONS] <INTERACTION>

Arguments:
  <INTERACTION>  A reference to an interaction: its ULID, or its short handle.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh interaction next` {#interaction-next}

Show the interaction that is next

**API method:** [`interaction.next`](/docs/reference/api/methods/interaction#interaction-next)

```text
Show the interaction that is next

Usage: nh interaction next [OPTIONS]

Options:
      --after <TEXT>       
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh interaction reveal` {#interaction-reveal}

Show an interaction in a window

**API method:** [`interaction.reveal`](/docs/reference/api/methods/interaction#interaction-reveal)

```text
Show an interaction in a window

Usage: nh interaction reveal [OPTIONS] [INTERACTION]

Arguments:
  [INTERACTION]  

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --window <TEXT>      The window that shows the card. The default is the window with the OS focus, else the
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
