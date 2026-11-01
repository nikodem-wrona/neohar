---
title: "nh hermes"
description: "Chats with Hermes Agent, as the Hermes view shows them"
sidebar_position: 20
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

Chats with Hermes Agent, as the Hermes view shows them

```text
Chats with Hermes Agent, as the Hermes view shows them

Usage: nh hermes [OPTIONS] <COMMAND>

Commands:
  check   Check that Hermes Agent runs here, and read its approvals.mode
  start   Start a chat with Hermes in a folder (an absolute path)
  prompt  Send a prompt to a chat; during a turn it waits for the end of the turn
  cancel  Stop the turn of a chat
  answer  Answer an approval of a chat with one of its options
  set     Change the model or the mode of a chat
  retry   Start hermes acp again for a chat that is disconnected
  close   Close a chat; the session stays in Hermes

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh hermes check` {#hermes-check}

Check that Hermes Agent runs here, and read its approvals.mode

**API method:** [`hermes.check`](/docs/reference/api/methods/hermes#hermes-check)

```text
Check that Hermes Agent runs here, and read its approvals.mode

Usage: nh hermes check [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh hermes start` {#hermes-start}

Start a chat with Hermes in a folder (an absolute path)

**API method:** [`hermes.start`](/docs/reference/api/methods/hermes#hermes-start)

```text
Start a chat with Hermes in a folder (an absolute path)

Usage: nh hermes start [OPTIONS] <CWD>

Arguments:
  <CWD>  The absolute path of the folder in which Hermes works.

Options:
      --prompt <TEXT>      The first prompt.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh hermes prompt` {#hermes-prompt}

Send a prompt to a chat; during a turn it waits for the end of the turn

**API method:** [`hermes.prompt`](/docs/reference/api/methods/hermes#hermes-prompt)

```text
Send a prompt to a chat; during a turn it waits for the end of the turn

Usage: nh hermes prompt [OPTIONS] <CHAT> <TEXT>

Arguments:
  <CHAT>  The ULID of a hermes chat, as 26 characters.
  <TEXT>  

Options:
      --input-id <NUMBER>  A second call with the same id sends nothing.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh hermes cancel` {#hermes-cancel}

Stop the turn of a chat

**API method:** [`hermes.cancel`](/docs/reference/api/methods/hermes#hermes-cancel)

```text
Stop the turn of a chat

Usage: nh hermes cancel [OPTIONS] <CHAT>

Arguments:
  <CHAT>  The ULID of a hermes chat, as 26 characters.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh hermes answer` {#hermes-answer}

Answer an approval of a chat with one of its options

**API method:** [`hermes.answer`](/docs/reference/api/methods/hermes#hermes-answer)

```text
Answer an approval of a chat with one of its options

Usage: nh hermes answer [OPTIONS] <CHAT> <REQUEST> <OPTION>

Arguments:
  <CHAT>     The ULID of a hermes chat, as 26 characters.
  <REQUEST>  The `request` of the approval item.
  <OPTION>   The `id` of one of its options.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh hermes set` {#hermes-set}

Change the model or the mode of a chat

**API method:** [`hermes.set`](/docs/reference/api/methods/hermes#hermes-set)

```text
Change the model or the mode of a chat

Usage: nh hermes set [OPTIONS] <CHAT>

Arguments:
  <CHAT>  The ULID of a hermes chat, as 26 characters.

Options:
      --mode <TEXT>        The id of a mode of the chat.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --model <TEXT>       The id of a model of the chat.
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh hermes retry` {#hermes-retry}

Start hermes acp again for a chat that is disconnected

**API method:** [`hermes.retry`](/docs/reference/api/methods/hermes#hermes-retry)

```text
Start hermes acp again for a chat that is disconnected

Usage: nh hermes retry [OPTIONS] <CHAT>

Arguments:
  <CHAT>  The ULID of a hermes chat, as 26 characters.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh hermes close` {#hermes-close}

Close a chat; the session stays in Hermes

**API method:** [`hermes.close`](/docs/reference/api/methods/hermes#hermes-close)

```text
Close a chat; the session stays in Hermes

Usage: nh hermes close [OPTIONS] <CHAT>

Arguments:
  <CHAT>  The ULID of a hermes chat, as 26 characters.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
