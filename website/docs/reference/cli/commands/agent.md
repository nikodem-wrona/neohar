---
title: "nh agent"
description: "Agents: the runs of Claude Code and Codex"
sidebar_position: 13
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

Agents: the runs of Claude Code and Codex

```text
Agents: the runs of Claude Code and Codex

Usage: nh agent [OPTIONS] <COMMAND>

Commands:
  list       List the agents
  show       Show an agent
  start      Start an agent in a new pane, a new tab or an idle shell
  choices    List the models and the effort levels that a start can choose
  stop       Ask an agent to end
  interrupt  Interrupt the turn of an agent
  prompt     Type a prompt into an agent
  reply      Send the next prompt to an agent whose turn ended
  resume     Start an agent that ended with its session
  wait       Wait until an agent is idle, has ended, or needs you
  link       Bind a pane to its agent
  unlink     Break the link of an agent
  move       Move an agent to another place on the board

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh agent list` {#agent-list}

List the agents

**API method:** [`agent.list`](/docs/reference/api/methods/agent#agent-list)

```text
List the agents

Usage: nh agent list [OPTIONS]

Options:
      --harness <TEXT>     
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --pane <TEXT>        
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --workspace <TEXT>   
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh agent show` {#agent-show}

Show an agent

**API method:** [`agent.show`](/docs/reference/api/methods/agent#agent-show)

```text
Show an agent

Usage: nh agent show [OPTIONS] <AGENT>

Arguments:
  <AGENT>  A reference to an agent run: its ULID, or its unique name.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh agent start` {#agent-start}

Start an agent in a new pane, a new tab or an idle shell

**API method:** [`agent.start`](/docs/reference/api/methods/agent#agent-start)

```text
Start an agent in a new pane, a new tab or an idle shell

Usage: nh agent start [OPTIONS] <HARNESS>

Arguments:
  <HARNESS>  The two harnesses of v1.

Options:
      --args <VALUE>           Extra arguments for the harness.
      --session <NAME>         The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --cwd <TEXT>             
      --socket <PATH>          The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --effort <TEXT>          The effort level (`high`). Without it, the harness uses the level of its own settings.
      --machine <LABEL>        Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json                   Print JSON. This is the default when stdout is not a terminal
      --model <TEXT>           The model, as the harness names it (`opus`, `gpt-5.5`). Without it, the harness uses
      --name <TEXT>            A unique name for the run. A call can use it instead of the id.
      --timeout <SECONDS>      How long to wait for the core to answer, in seconds [default: 30]
      --prompt <TEXT>          
      --task <TEXT>            
      --pane <pane>            Start in this pane, which must be an idle shell
      --split <split>          Start in a new pane beside this pane
      --direction <direction>  With --split: left, right, up or down (default: right)
      --new-tab <new-tab>      Start in a new tab of this workspace
  -h, --help                   Print help
```

## `nh agent choices` {#agent-choices}

List the models and the effort levels that a start can choose

**API method:** [`agent.choices`](/docs/reference/api/methods/agent#agent-choices)

```text
List the models and the effort levels that a start can choose

Usage: nh agent choices [OPTIONS] [HARNESS]

Arguments:
  [HARNESS]  

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh agent stop` {#agent-stop}

Ask an agent to end

**API method:** [`agent.stop`](/docs/reference/api/methods/agent#agent-stop)

```text
Ask an agent to end

Usage: nh agent stop [OPTIONS] <AGENT>

Arguments:
  <AGENT>  A reference to an agent run: its ULID, or its unique name.

Options:
      --force              
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh agent interrupt` {#agent-interrupt}

Interrupt the turn of an agent

**API method:** [`agent.interrupt`](/docs/reference/api/methods/agent#agent-interrupt)

```text
Interrupt the turn of an agent

Usage: nh agent interrupt [OPTIONS] <AGENT>

Arguments:
  <AGENT>  A reference to an agent run: its ULID, or its unique name.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh agent prompt` {#agent-prompt}

Type a prompt into an agent

**API method:** [`agent.prompt`](/docs/reference/api/methods/agent#agent-prompt)

```text
Type a prompt into an agent

Usage: nh agent prompt [OPTIONS] <AGENT> <TEXT>

Arguments:
  <AGENT>  A reference to an agent run: its ULID, or its unique name.
  <TEXT>   

Options:
      --no-submit          Press Enter after the text.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh agent reply` {#agent-reply}

Send the next prompt to an agent whose turn ended

**API method:** [`agent.reply`](/docs/reference/api/methods/agent#agent-reply)

```text
Send the next prompt to an agent whose turn ended

Usage: nh agent reply [OPTIONS] <RUN> <TEXT>

Arguments:
  <RUN>   A reference to an agent run: its ULID, or its unique name.
  <TEXT>  

Options:
      --input-id <NUMBER>  
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh agent resume` {#agent-resume}

Start an agent that ended with its session

**API method:** [`agent.resume`](/docs/reference/api/methods/agent#agent-resume)

```text
Start an agent that ended with its session

Usage: nh agent resume [OPTIONS] <AGENT>

Arguments:
  <AGENT>  A reference to an agent run: its ULID, or its unique name.

Options:
      --placement <JSON>   
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh agent wait` {#agent-wait}

Wait until an agent is idle, has ended, or needs you

**API method:** [`agent.wait`](/docs/reference/api/methods/agent#agent-wait)

```text
Wait until an agent is idle, has ended, or needs you

Usage: nh agent wait [OPTIONS] <AGENT> <UNTIL>

Arguments:
  <AGENT>  A reference to an agent run: its ULID, or its unique name.
  <UNTIL>  

Options:
      --session <NAME>       The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --timeout-ms <NUMBER>  
      --socket <PATH>        The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>      Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json                 Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>    How long to wait for the core to answer, in seconds [default: 30]
  -h, --help                 Print help
```

## `nh agent link` {#agent-link}

Bind a pane to its agent

**API method:** [`agent.link`](/docs/reference/api/methods/agent#agent-link)

```text
Bind a pane to its agent

Usage: nh agent link [OPTIONS] <PANE>

Arguments:
  <PANE>  A reference to a pane: its ULID, or its short handle.

Options:
      --harness <TEXT>     
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --session-id <TEXT>  
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --thread <TEXT>      The thread of a Codex run on the shared daemon.
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh agent unlink` {#agent-unlink}

Break the link of an agent

**API method:** [`agent.unlink`](/docs/reference/api/methods/agent#agent-unlink)

```text
Break the link of an agent

Usage: nh agent unlink [OPTIONS] <AGENT>

Arguments:
  <AGENT>  A reference to an agent run: its ULID, or its unique name.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh agent move` {#agent-move}

Move an agent to another place on the board

**API method:** [`agent.move`](/docs/reference/api/methods/agent#agent-move)

```text
Move an agent to another place on the board

Usage: nh agent move [OPTIONS] <AGENT> <INDEX>

Arguments:
  <AGENT>  A reference to an agent run: its ULID, or its unique name.
  <INDEX>  The place among the runs, counted from 0. A place after the last run is the end.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
