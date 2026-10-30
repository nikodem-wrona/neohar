---
title: Concepts and glossary
description: The words that neohar uses, each with a short meaning and the page that explains it.
sidebar_position: 2
---

Every page of this site uses these words with the same meaning.

## How the things nest

A **machine** runs a core. The core serves one **session**. A session has **workspaces**, which can nest in **groups**. A workspace has **tabs**, and a tab has a layout of **panes**. Each pane has one **holder** and the program in it. When that program is a harness, the pane has an **agent run**, and the run can have open **interactions**. A **window** is a view: it shows a tab, or one pane that popped out.

## Places

| Word | Meaning | More |
|---|---|---|
| Machine | A host that runs a neohar core: this Mac, or another one that the app reaches over ssh. | [Remote machines](/docs/guides/remote-machines) |
| Session | One isolated core on a machine, `default` unless `--session` or `NEOHAR_SESSION` names another. A debug build uses `dev`. Each session has its own data and sockets; the config is shared. | [Architecture](/docs/architecture/overview) |
| Workspace | A group of tabs, usually for one repository. | [Workspaces and panes](/docs/guides/workspaces-and-panes) |
| Tab | One layout of panes: a split tree of tiled panes, plus floating panes. A hidden tab holds the agent of a workflow node, and the tab bar does not show it until you open it. | [Workspaces and panes](/docs/guides/workspaces-and-panes) |
| Pane | A terminal: a PTY and the program in it. Tiled, floating, or popped out into its own window. | [Workspaces and panes](/docs/guides/workspaces-and-panes) |
| Handle | The short name of an object that a person can type: `w3` (workspace), `w3:t2` (tab), `w3:p5` (pane), `i42` (interaction). A handle never changes. | [Command line](/docs/reference/cli) |
| Drawer | The right column of a window, with the tabs Inbox, Timeline, Files and Changes. | [Desktop UI](/docs/desktop/ui) |

## Processes

| Word | Meaning | More |
|---|---|---|
| App | `neohar.app`: the core plus windows, webviews and native integrations. | [Desktop app](/docs/desktop/overview) |
| Core | The Rust library `nh-core`: the state actor, the VT engines, the adapters and the API. | [The core](/docs/architecture/core) |
| Holder | `nh hold`, one per pane. It owns the PTY and journals the output, so the core can restore the screen. It outlives the app. | [Holders](/docs/architecture/holders) |
| Daemon | `nh daemon`: the core without windows. It serves a remote machine, or an app on the same Mac that runs as its client. | [Remote machines](/docs/guides/remote-machines) |
| Link | The connection of a client app to a core through `link.sock`. Over ssh, `nh bridge` carries it. | [Remote machines](/docs/guides/remote-machines) |
| Hook shim | `nh hook`, which the hooks of a harness call. It forwards the hook to the core and can hold it until there is an answer. Budget: 5 ms. | [Agents](/docs/agents/overview) |

## Agents

| Word | Meaning | More |
|---|---|---|
| Harness | A coding agent CLI: `claude` or `codex`. | [Claude Code and Codex](/docs/agents/harnesses) |
| Manifest | A TOML file that says how to detect a harness, which versions are validated, and what it can do. | [Claude Code and Codex](/docs/agents/harnesses) |
| Adapter | Code that turns the signals of a harness into neohar events, and delivers answers back. | [Agents](/docs/agents/overview) |
| Agent run | One live instance of a harness in a pane. | [Agents](/docs/agents/overview) |
| Facet | One independent part of the state of a run: process, execution, pending interactions, adapter health, read state. Each has a `source` and a `confidence`. | [Agents](/docs/agents/overview) |
| Interaction | Something an agent needs from a person: an approval, a question, a plan review or a notice. | [Interactions](/docs/agents/interactions) |
| Gate mode | `observe` (the harness shows its own dialog), `gate` (the core holds the hook until an answer) or `auto` (gate only when the pane does not have the focus). | [Interactions](/docs/agents/interactions) |
| Delivery | The recoverable transaction that brings an answer to the harness: `delivered`, `resolved_elsewhere`, `delivery_unknown` or `delivery_failed`. | [Interactions](/docs/agents/interactions) |
| Policy rule | A `[[policy.rule]]` block that decides an approval before a person is asked. | [Agents and approvals](/docs/guides/agents-and-approvals) |
| Workflow | A graph of nodes in `~/.config/neohar/workflows/`: a manual trigger, agent nodes and Report nodes. The last message of the agent of a node is its report. | [Workflows](/docs/guides/workflows) |
| Execution | One start of a workflow in a folder, with the state and the report of each node, for example "Review 3". It is not an agent run. | [Workflows](/docs/guides/workflows) |

## Data

| Word | Meaning | More |
|---|---|---|
| Event outbox | The `events` table, written in the same transaction as each change. Clients follow it with a cursor. | [The core](/docs/architecture/core) |
| Projection | A read model in the memory of the core: a snapshot, then keyed patches. The UI holds one store per projection and derives no facts. A pure view mapper only shapes them for the view. | [API concepts](/docs/reference/api/concepts) |
| Transport | How the UI reaches the core: Tauri IPC in the app, a WebSocket in tests. | [Desktop app](/docs/desktop/overview) |
| Frame | A binary update of the screen of one pane: a keyframe or a delta of packed cells. | [Rendering and input](/docs/architecture/rendering-and-input) |
| Peek | A slow render subscription for a pane that is not on screen, for the context of a card. Looking never releases a held approval. | [Rendering and input](/docs/architecture/rendering-and-input) |
| Job | A long command (`agent.start`, `agent.resume`) that returns a job id at once. Its progress is in the `jobs` projection. | [API concepts](/docs/reference/api/concepts) |
