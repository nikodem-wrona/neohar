---
title: "nh workflow"
description: "Workflows of agents and their executions, as the Workflows view shows them"
sidebar_position: 19
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

Workflows of agents and their executions, as the Workflows view shows them

```text
Workflows of agents and their executions, as the Workflows view shows them

Usage: nh workflow [OPTIONS] <COMMAND>

Commands:
  list        List the workflow files of ~/.config/neohar/workflows
  execute     Execute a workflow in a folder, or in the current directory. --wait waits for the end
  executions  List the executions of the workflows, the newest first
  show        Show an execution: the state, the verdict and the report of each node
  report      Show a report of an execution: report.md, or the report of one node with --node
  retry       Start a node of an execution that failed again
  skip        Skip a node of an execution that failed or waits; the nodes after it go on
  cancel      Cancel an execution and stop its agents

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh workflow list` {#workflow-list}

List the workflow files of ~/.config/neohar/workflows

**API method:** [`workflow.list`](/docs/reference/api/methods/workflow#workflow-list)

```text
List the workflow files of ~/.config/neohar/workflows

Usage: nh workflow list [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh workflow execute` {#workflow-execute}

Execute a workflow in a folder, or in the current directory. --wait waits for the end

**API method:** [`workflow.execute`](/docs/reference/api/methods/workflow#workflow-execute)

```text
Execute a workflow in a folder, or in the current directory. --wait waits for the end

Usage: nh workflow execute [OPTIONS] <WORKFLOW>

Arguments:
  <WORKFLOW>  

Options:
      --base <TEXT>        The base of the changes. Without it, the default of `workflow.facts`.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --folder <TEXT>      The absolute path of the folder in which the agents work.
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --node <TEXT>        Execute only this node, with the reports of the last execution in this folder as inputs.
      --json               Print JSON. This is the default when stdout is not a terminal
      --notes <TEXT>       What the person built and why. `{{start.notes}}` gives it to a prompt.
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
      --wait               Wait for the end of the execution, then print the path of report.md
  -h, --help               Print help
```

## `nh workflow executions` {#workflow-executions}

List the executions of the workflows, the newest first

**API method:** [`workflow.executions`](/docs/reference/api/methods/workflow#workflow-executions)

```text
List the executions of the workflows, the newest first

Usage: nh workflow executions [OPTIONS]

Options:
      --folder <TEXT>      Only the executions in this folder.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh workflow show` {#workflow-show}

Show an execution: the state, the verdict and the report of each node

**API method:** [`workflow.show`](/docs/reference/api/methods/workflow#workflow-show)

```text
Show an execution: the state, the verdict and the report of each node

Usage: nh workflow show [OPTIONS] <EXECUTION>

Arguments:
  <EXECUTION>  The ULID of a workflow execution, as 26 characters.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh workflow report` {#workflow-report}

Show a report of an execution: report.md, or the report of one node with --node

**API method:** [`workflow.report`](/docs/reference/api/methods/workflow#workflow-report)

```text
Show a report of an execution: report.md, or the report of one node with --node

Usage: nh workflow report [OPTIONS] <EXECUTION>

Arguments:
  <EXECUTION>  The ULID of a workflow execution, as 26 characters.

Options:
      --node <TEXT>        The node. Without it, `report.md`.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh workflow retry` {#workflow-retry}

Start a node of an execution that failed again

**API method:** [`workflow.retry`](/docs/reference/api/methods/workflow#workflow-retry)

```text
Start a node of an execution that failed again

Usage: nh workflow retry [OPTIONS] <EXECUTION> <NODE>

Arguments:
  <EXECUTION>  The ULID of a workflow execution, as 26 characters.
  <NODE>       

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh workflow skip` {#workflow-skip}

Skip a node of an execution that failed or waits; the nodes after it go on

**API method:** [`workflow.skip`](/docs/reference/api/methods/workflow#workflow-skip)

```text
Skip a node of an execution that failed or waits; the nodes after it go on

Usage: nh workflow skip [OPTIONS] <EXECUTION> <NODE>

Arguments:
  <EXECUTION>  The ULID of a workflow execution, as 26 characters.
  <NODE>       

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh workflow cancel` {#workflow-cancel}

Cancel an execution and stop its agents

**API method:** [`workflow.cancel`](/docs/reference/api/methods/workflow#workflow-cancel)

```text
Cancel an execution and stop its agents

Usage: nh workflow cancel [OPTIONS] <EXECUTION>

Arguments:
  <EXECUTION>  The ULID of a workflow execution, as 26 characters.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
