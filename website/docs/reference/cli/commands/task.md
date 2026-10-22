---
title: "nh task"
description: "Task workspaces"
sidebar_position: 16
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

Task workspaces

```text
Task workspaces

Usage: nh task [OPTIONS] <COMMAND>

Commands:
  create   Keep a draft of a task; with --start, make its worktree, ports and tab and start its agent
  update   Change a draft, or the title of a task that started
  start    Start a draft: its worktree, branch, ports, files, tab and agent
  delete   Remove a draft or a closed task
  list     List the tasks
  show     Show a task
  changes  Count the changes of a task now: its files from the merge base to the worktree
  diff     Show the diff of one changed file of a task
  close    Close a task

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh task create` {#task-create}

Keep a draft of a task; with --start, make its worktree, ports and tab and start its agent

**API method:** [`task.create`](/docs/reference/api/methods/task#task-create)

```text
Keep a draft of a task; with --start, make its worktree, ports and tab and start its agent

Usage: nh task create [OPTIONS] <TITLE>

Arguments:
  <TITLE>  

Options:
      --base <TEXT>        
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --branch <TEXT>      
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --effort <TEXT>      The effort level of the agent. It needs `harness`.
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --files <VALUE>      Files to attach, by path. The core reads them now.
      --json               Print JSON. This is the default when stdout is not a terminal
      --harness <TEXT>     
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
      --model <TEXT>       The model of the agent. It needs `harness`.
      --ports <NUMBER>     How many ports to lease.
      --prompt <TEXT>      
      --repo <TEXT>        
      --start              Start the task at once, as `task.start` does.
      --uploads <JSON>     Files to attach with their bytes: what the UI picked or pasted.
  -h, --help               Print help
```

## `nh task update` {#task-update}

Change a draft, or the title of a task that started

**API method:** [`task.update`](/docs/reference/api/methods/task#task-update)

```text
Change a draft, or the title of a task that started

Usage: nh task update [OPTIONS] <TASK>

Arguments:
  <TASK>  A reference to a task: its ULID, or its short handle.

Options:
      --base <TEXT>        
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --branch <TEXT>      
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --effort <TEXT>      
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --files <VALUE>      Files to attach, by path.
      --json               Print JSON. This is the default when stdout is not a terminal
      --harness <TEXT>     
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
      --model <TEXT>       
      --ports <NUMBER>     
      --prompt <TEXT>      
      --remove <VALUE>     The attachments to remove, by id.
      --repo <TEXT>        
      --title <TEXT>       
      --uploads <JSON>     Files to attach with their bytes.
      --clear <FIELD>      Clear a value: base, branch, harness, prompt, model, effort or ports
  -h, --help               Print help
```

## `nh task start` {#task-start}

Start a draft: its worktree, branch, ports, files, tab and agent

**API method:** [`task.start`](/docs/reference/api/methods/task#task-start)

```text
Start a draft: its worktree, branch, ports, files, tab and agent

Usage: nh task start [OPTIONS] <TASK>

Arguments:
  <TASK>  A reference to a task: its ULID, or its short handle.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh task delete` {#task-delete}

Remove a draft or a closed task

**API method:** [`task.delete`](/docs/reference/api/methods/task#task-delete)

```text
Remove a draft or a closed task

Usage: nh task delete [OPTIONS] <TASK>

Arguments:
  <TASK>  A reference to a task: its ULID, or its short handle.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh task list` {#task-list}

List the tasks

**API method:** [`task.list`](/docs/reference/api/methods/task#task-list)

```text
List the tasks

Usage: nh task list [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --status <TEXT>      
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh task show` {#task-show}

Show a task

**API method:** [`task.show`](/docs/reference/api/methods/task#task-show)

```text
Show a task

Usage: nh task show [OPTIONS] <TASK>

Arguments:
  <TASK>  A reference to a task: its ULID, or its short handle.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh task changes` {#task-changes}

Count the changes of a task now: its files from the merge base to the worktree

**API method:** [`task.changes`](/docs/reference/api/methods/task#task-changes)

```text
Count the changes of a task now: its files from the merge base to the worktree

Usage: nh task changes [OPTIONS] <TASK>

Arguments:
  <TASK>  A reference to a task: its ULID, or its short handle.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh task diff` {#task-diff}

Show the diff of one changed file of a task

**API method:** [`task.diff`](/docs/reference/api/methods/task#task-diff)

```text
Show the diff of one changed file of a task

Usage: nh task diff [OPTIONS] <TASK> <PATH>

Arguments:
  <TASK>  A reference to a task: its ULID, or its short handle.
  <PATH>  

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh task close` {#task-close}

Close a task

**API method:** [`task.close`](/docs/reference/api/methods/task#task-close)

```text
Close a task

Usage: nh task close [OPTIONS] <TASK>

Arguments:
  <TASK>  A reference to a task: its ULID, or its short handle.

Options:
      --force              
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --remove-worktree    
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
