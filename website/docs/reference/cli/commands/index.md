---
title: "Commands"
description: "Every command of nh, from its own help."
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

Every command below is `nh <noun> <verb>`. The global options (`--session`, `--socket`, `--json`, `--timeout`) work with every command; see [Command line (nh)](/docs/reference/cli).

```text
The command line of neohar. It talks to the core of a session over the API socket

Usage: nh [OPTIONS] <COMMAND>

Commands:
  api          Call the API directly
  doctor       Check the install and the core: the symlink, the sockets, the integrations of the harnesses, the storage. It works also when neohar does not run
  debug        Tools for a bug report
  events       The log of events
  completions  Print the completions of a shell
  session      The session of the core
  machine      This machine and the machines it connects to
  workspace    Workspaces
  group        Groups of workspaces
  tab          Tabs
  layout       The layout of the panes of a tab
  pane         Panes
  agent        Agents: the runs of Claude Code and Codex
  harness      Claude Code and Codex on this machine: installed, version, login
  interaction  What agents ask: approvals, questions, plans
  phone        Phones that supervise this machine
  fs           The file system of the machine
  git          Git repositories, worktrees, branches and history, as the Git view shows them
  config       The configuration
  policy       Rules that decide approvals
  integration  The hooks of the harnesses
  search       Search the scrollback
  jobs         Long commands

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
  -V, --version            Print version
```

| Command | What it does | Verbs |
|---|---|---|
| [`nh api`](/docs/reference/cli/commands/api) | Call the API directly | `call`, `schema` |
| [`nh doctor`](/docs/reference/cli/commands/doctor) | Check the install and the core: the symlink, the sockets, the integrations of the harnesses, the storage. It works also when neohar does not run |  |
| [`nh debug`](/docs/reference/cli/commands/debug) | Tools for a bug report | `bundle` |
| [`nh events`](/docs/reference/cli/commands/events) | The log of events | `tail` |
| [`nh completions`](/docs/reference/cli/commands/completions) | Print the completions of a shell |  |
| [`nh session`](/docs/reference/cli/commands/session) | The session of the core | `stop-everything`, `shutdown`, `info` |
| [`nh machine`](/docs/reference/cli/commands/machine) | This machine and the machines it connects to | `info`, `list`, `connect`, `disconnect`, `open-window` |
| [`nh workspace`](/docs/reference/cli/commands/workspace) | Workspaces | `create`, `list`, `show`, `rename`, `close`, `move` |
| [`nh group`](/docs/reference/cli/commands/group) | Groups of workspaces | `create`, `list`, `show`, `rename`, `close`, `move` |
| [`nh tab`](/docs/reference/cli/commands/tab) | Tabs | `create`, `rename`, `close`, `move`, `select` |
| [`nh layout`](/docs/reference/cli/commands/layout) | The layout of the panes of a tab | `set-ratios`, `equalize`, `zoom` |
| [`nh pane`](/docs/reference/cli/commands/pane) | Panes | `list`, `show`, `split`, `close`, `move`, `float`, `unfloat`, `popout`, `restart`, `paste`, `send-keys`, `scroll`, `copy`, `read` |
| [`nh agent`](/docs/reference/cli/commands/agent) | Agents: the runs of Claude Code and Codex | `list`, `show`, `start`, `choices`, `stop`, `interrupt`, `prompt`, `reply`, `resume`, `wait`, `link`, `unlink`, `move` |
| [`nh harness`](/docs/reference/cli/commands/harness) | Claude Code and Codex on this machine: installed, version, login | `refresh` |
| [`nh interaction`](/docs/reference/cli/commands/interaction) | What agents ask: approvals, questions, plans | `list`, `show`, `answer`, `dismiss`, `next`, `reveal` |
| [`nh phone`](/docs/reference/cli/commands/phone) | Phones that supervise this machine | `pair-begin`, `pair-cancel`, `remove`, `rename`, `publish`, `unpublish`, `push-test` |
| [`nh fs`](/docs/reference/cli/commands/fs) | The file system of the machine | `list` |
| [`nh git`](/docs/reference/cli/commands/git) | Git repositories, worktrees, branches and history, as the Git view shows them | `repos`, `add`, `status`, `worktrees`, `branches`, `log`, `show`, `diff`, `switch`, `unstash`, `fetch`, `pull` |
| [`nh config`](/docs/reference/cli/commands/config) | The configuration | `get`, `set`, `reload`, `edit` |
| [`nh policy`](/docs/reference/cli/commands/policy) | Rules that decide approvals | `list`, `add`, `remove`, `suggest` |
| [`nh integration`](/docs/reference/cli/commands/integration) | The hooks of the harnesses | `list`, `install`, `status`, `uninstall`, `doctor`, `capabilities` |
| [`nh search`](/docs/reference/cli/commands/search) | Search the scrollback | `query` |
| [`nh jobs`](/docs/reference/cli/commands/jobs) | Long commands | `cancel` |
