# neohar workflows spec

Oct 30, 2026 · @Nikodem Wrona

neohar gets a Workflows view: a canvas, as in n8n, where you connect nodes into a workflow. An agent node gives an instruction to Claude Code or Codex. Its report goes to each node that it connects to. A Report node at the end writes one report with links to the reports of the nodes before it.

The example of this spec is the workflow "Review". When your development is complete, you execute it. It does a code review, a business-logic review and a security review, and then writes one report.

![Workflows view · the "Review" workflow during an execution · sample data](workflows/canvas.svg)

## Decisions

| Question | Decision | Why |
| --- | --- | --- |
| Builder | A canvas of nodes and connections, as in n8n | You see the order. You draw a chain or parallel branches for each workflow |
| Order | A node starts when all of its inputs are done. Branches without a connection between them run in parallel | The connections are the only rule for the order |
| Node types in v1 | Manual trigger, Agent (Claude Code or Codex), Report | KISS. Human check, If and Loop come later |
| Workflow files | Personal only: `~/.config/neohar/workflows/<name>.toml` | Your decision. Nothing goes into the repositories |
| Where an execution runs | The folder of the focused pane, or a folder that you select in the view | The view follows the current path, as the Git view does |
| The agent of a node | A real interactive pane in a hidden tab. "Show pane" opens it | Your decision. The holder, the cards and the restore stay. Headless `claude -p` and `codex exec` skip them |
| The report of a node | The last message of the agent at the end of its turn. The core writes it to a file | It works in a read-only sandbox. The agent cannot forget to write the file |
| Where the reports go | The data directory of neohar, not the worktree | Your decision. A report in the worktree shows in `git status` for the next reviewer |
| Edits | Agent nodes are read-only by default. "Can edit files" changes this for one node | A review must not change the code that it reviews |
| The name of one execution | "Execution", as in n8n | "Run" is already the agent run of the glossary |

## Scope

| Area | v1 | Later | Out of scope |
| --- | --- | --- | --- |
| Nodes | Manual trigger, Agent, Report | Human check, If on the verdict, Loop (fix and review again, at most N times), triggers on a commit and on a turn end | Shell nodes, HTTP nodes, other harnesses |
| Canvas | Add, connect, move, delete, pan, zoom, fit, tidy up, undo | Multi-select, copy and paste, groups, notes on the canvas | A plugin system for node types |
| Executions | The full workflow, or one node with the inputs of the last execution. Retry, skip, cancel | Executions on a remote machine, a schedule | Execution in a cloud |
| Reports | One file for each agent node. `report.md` with links. An optional summary by an agent | A PR comment from `report.md` |  |
| Clients | The desk app and the CLI | The phone shows the executions and the reports |  |

A workflow is not a task workspace. It makes no worktree and binds no agent to a folder. Each execution runs in a folder that exists.

## Where it lives

The Workflows view takes the middle of the window, as the Git view does. The command line, the board and the status bar stay.

- **Open and close.** `mod+shift+e` (key `workflows_open`), "Workflows" in the palette, or a Workflows key in the status bar. A board strip gets "Execute a workflow here", which opens the view on the folder of that agent. Esc or the same key goes back to the same tab.
- **Full width.** The left column and the drawer fold to their rails while the view shows. The Git view and the Workflows view never show at the same time.
- **The panes continue.** The window reports that it draws no pane (`panes_hidden` of `window.focus_report`), as for the Git view.
- **A mode of the window.** The view is window state in `app/view.ts`, next to `gitOpen`. It holds no business state.
- **The current path.** The view opens on the folder of the focused pane. The top bar shows that folder, its repository and its branch. A folder menu selects another known folder: the repositories of the Git view and the folders of the panes.

The layout of the view:

- **Top bar.** The folder menu, the branch, and the button "Execute workflow" on the right, as in n8n.
- **Left column.** Workflows: the list, "New", rename, duplicate and delete. Executions: one row for each execution with its state, name, number, time and duration. The executions of the current folder show first. "All folders" shows all of them.
- **Middle.** The canvas of the selected workflow, or of the selected execution.
- **Right panel.** It opens when you click a node of an execution. It shows the report and the actions of the node.

Workflows are personal. Thus the list of workflows is the same in each folder.

## The canvas

### Nodes

| Node | Inputs | Settings | Output |
| --- | --- | --- | --- |
| Manual trigger | None. Each workflow has exactly one | None | The facts of the execution: `folder`, `branch`, `base`, `head`, `notes` |
| Agent | One or more | Title, harness, model, effort, "Can edit files" (off), time limit (60 min), prompt | The report: one Markdown file |
| Report | One or more | Title, "Summary by" (none, Claude Code or Codex), summary prompt (optional) | `report.md`. It is also a report, so another node can use it as an input |

Each node has one input port on the left and one output port on the right. An input port accepts many connections. The ports have no types.

A node is a card in the glass look. The card has the icon of its kind, the title, and a second line such as "Codex · gpt-5.5 · high · read-only". During an execution, the card also shows the state and the duration.

### Build a workflow

- **Add a node.** Drag from an output port to an empty area of the canvas. Then select an item in the node picker. The new node connects to that port. Tab, a right click on the canvas or the "+ Add" button of the toolbar also opens the picker.
- **The node picker.** It has a filter field and keys (type to filter, ↑ ↓, Enter). The items are "Claude Code agent", "Codex agent" and "Report". The presets are "Code review", "Business-logic review" and "Security review". A preset is an agent node with a harness and a prompt. The core keeps the presets.
- **Connect.** Drag from an output port to a node or to its input port.
- **Disconnect.** Click a connection, then push Delete. A connection also shows an × at its middle when the pointer is on it.
- **Move.** Drag a node. Positions snap to a grid of 20 units.
- **Delete.** Select a node, then push Delete or Backspace. You cannot delete the trigger.
- **Pan and zoom.** Drag an empty area, or scroll with two fingers, to pan. Pinch or `mod`+scroll to zoom from 25 % to 200 %. `mod+0` fits the workflow in the view.
- **Tidy up.** A button asks the core for a new layout: one column for each depth, and an order in each column that keeps the crossings few.
- **Undo.** `mod+z` and `mod+shift+z` move through the last 50 saved versions of the workflow. The core keeps them.

### Node details

Double-click a node to open its details. As in n8n, the details have three columns:

- **Input.** The nodes that connect to this node, with their reports from the last execution in this folder. A click on an input chip puts its reference into the prompt at the cursor.
- **Settings.** The settings of the node. The prompt editor uses the terminal font. Below it, the footer that the core adds shows as read-only text.
- **Output.** The report of this node from the last execution in this folder, with "Execute this node".

### References in a prompt

A prompt can contain references. The core puts the values in when the node starts.

| Reference | Value |
| --- | --- |
| `{{start.folder}}`, `{{start.branch}}`, `{{start.base}}`, `{{start.head}}` | The facts of the execution |
| `{{start.notes}}` | The notes of the execution dialog |
| `{{<node>.report}}` | The absolute path of the report of an input node |

`start` is the id of the trigger. A reference to a node that is not an input is an error. A path is better than the text of a report: the agent reads the report when it needs it, and the prompt stays short.

### The footer of a prompt

The core adds this footer to the prompt of each agent node. The user cannot change it.

```text
---
neohar workflow "Review", node "Business-logic review", execution 3.
Inputs. Read them first:
- Code review (done, verdict warn): /…/workflows/executions/20261030-141205-review-3/code.md
Rules:
- Do not ask questions. Write your assumptions in the report.
- Do not edit files.
- End your turn with the full report in Markdown as your last message.
- Make the last line "Verdict: pass", "Verdict: warn" or "Verdict: fail".
```

The rule "Do not edit files" is not in the footer of a node with "Can edit files". An input that was skipped shows as "(skipped, no report)".

### Validation

The core validates each save and shows the errors on the nodes. You can save a workflow with errors. You cannot execute it.

- Exactly one trigger. No cycles.
- Each node other than the trigger has at least one input, and the trigger can reach it.
- Each agent node has a harness and a prompt. The model and the effort are among the values of `agent.choices`.
- Each reference names the trigger or an input node.
- The ids of the nodes are unique and match `[a-z][a-z0-9_]*`.

### Saves

Each change saves at once: there is no Save button. The UI keeps only the drag in progress and the text of the prompt editor. It saves that text 500 ms after the last key, and on blur.

The core is the only writer of the files. It watches the folder of the workflows, so it sees an edit by hand. A save with an old revision is refused, and the UI loads the workflow again.

## Executions

### Start an execution

"Execute workflow" opens a dialog:

- **Folder.** The current folder of the view. You cannot change it in the dialog.
- **Base.** The default is the default branch of the upstream remote (`origin/HEAD`), for example `origin/main`. Without a remote, it is `main` or `master`. Without git, or in a repository that you do not trust, there is no base.
- **Notes.** Free text, for example what you built and why. "Take from agent" fills the field with the prompts of an agent run in this folder. The business-logic review needs this intent.

The CLI does the same:

```bash
nh workflow execute review --cwd . --base main --notes "Add the refund flow" --wait
```

At the start, the core records the graph of the workflow, the `HEAD` commit and a hash of the changes in the worktree. An edit of the workflow does not change an execution that runs.

### The order

The core starts each node when all of its inputs are done. At most `workflows.max_agents` (3) agents of one execution run at the same time. When more nodes are ready, they start by column, and then from top to bottom.

### The agent of a node

The core starts each agent node with `agent.start`:

- **Placement.** A new tab with the flag `hidden` in a workspace: the workspace with a pane in the folder, else the first one. The tab bar, the palette and the selection of a tab do not show a hidden tab. Something that selects it shows it: "Open as tab" in the panel (`workflow.reveal`), or a click on the card of an interaction. The placement is internal: `agent.start` does not take it. The panel shows the screen of a working agent as text (`pane_text:<pane>`).
- **Arguments.** The folder, the harness, the model and the effort of the node. A read-only node gets the read-only arguments of the manifest (next section). Claude Code also gets `--add-dir` with the folder of the execution, so it can read the input reports.
- **Prompt.** The prompt of the node with its references filled in, and the footer.
- **Tags.** The agent run records its execution and its node.

These agent runs show on the board only when they need you. The core does not raise `TurnEnded` for them and sends no turn-end notice. Questions, approvals and plan reviews work as today: cards, notifications and the board. The node shows "Needs you" while an interaction is open.

At the end of a turn without an open interaction, the core reads the last message of the agent. It takes it from the `last_assistant_message` of the hook. If it has no hook event, it reads the transcript. Today the adapters keep only a preview of that field, so they must keep the full text for these runs.

- **A report.** The message contains a `Verdict:` line. The core writes it to `<node id>.md`, and the node is done.
- **No report.** The message is empty, or it does not contain a `Verdict:` line. The core sends one prompt with `agent.reply`: "Write your full report as your last message. End it with the Verdict line." At the next end of a turn, the core writes the message as it is. Without a `Verdict:` line, the verdict is "unknown".

When a node is done, the core stops its agent and keeps its resume handle. "Continue in a tab" on the node starts it again with `agent.resume` in a visible tab. Thus you can ask the reviewer a question about its report, and no idle agents use memory.

### States

| Node state | Meaning |
| --- | --- |
| `waiting` | An input is not done |
| `queued` | Its inputs are done, but `workflows.max_agents` agents run |
| `working` | Its agent works |
| `needs_you` | Its agent has an open interaction |
| `done` | Its report exists |
| `failed` | The start failed, the agent exited, an error, a rate limit or the time limit. The reason shows on the card |
| `skipped` | You skipped it |
| `cancelled` | You cancelled the execution |

An execution is `running`, `paused`, `done` or `cancelled`.

- **A failure stops only its branch.** The other branches continue. The nodes after the failed node wait. When no node can start and a node failed, the execution is `paused`.
- **Retry.** It starts a new agent for the failed node, with the same prompt.
- **Skip.** The node becomes `skipped`. The nodes after it can start, and their footer tells them that the input has no report.
- **Cancel.** It stops each agent of the execution. The reports that exist stay.
- **Time limit.** When a node works for longer than its limit, the core interrupts its agent, and the node fails.

### A restart of the app

The state of each execution is in SQLite, in the same transaction as its events. The executor reacts to the changes of agent state. It is not a loop that waits. If you quit the app during an execution, the agents continue in their holders. When the app opens again, the core restores the agent runs. It finds the turns that ended while it was closed, reads their reports from the transcripts and starts the next nodes.

### The Report node

The Report node writes `<node id>.md` from the reports of its inputs. The node of the preset "Review" has the id `report`, so its file is `report.md`:

```markdown
# Review 3 · neohar · feat/refunds @ 1a2b3c4

2026-10-30 14:12 · 14 min · base `main` · the tree did not change during the execution

## Summary

(Only when "Summary by" is set: the text of that agent.)

## Results

| Node | Agent | Verdict | Time | Report |
| --- | --- | --- | --- | --- |
| Code review | Codex · gpt-5.5 · high | warn | 6 min | [code.md](code.md) |
| Business-logic review | Claude Code · opus | pass | 5 min | [logic.md](logic.md) |
| Security review | Claude Code · opus | fail | 8 min | [security.md](security.md) |

## Code review

(The first paragraph of the report.) [Full report](code.md)
```

The core writes all of it except the summary. If "Summary by" is set, the core starts that agent after the inputs are done. The prompt tells the agent to merge the reports, to sort the findings by severity and to link each finding to its source report. A custom summary prompt replaces the first part of that text. The footer stays.

If the `HEAD` commit or the hash of the changes is different at the end, the header says "The tree changed during the execution".

### Execute one node

"Execute this node" makes an execution with only that node. Its inputs are the reports of the last execution in this folder that has them. The core refuses it when an input has no report. Use it to tune a prompt without the full workflow.

### Notices

- When an execution is done or paused and no window has focus, a native notification shows, for example "Review 3 is done · 1 fail · 1 warn".
- The Workflows key in the status bar shows a mark while an execution is paused or has a node in `needs_you`.
- An execution is not an agent run, so it does not count in the badge of the board in v1.

### Files

```text
<data dir>/workflows/executions/20261030-141205-review-3/
  graph.toml     the workflow at the start of the execution
  code.md
  logic.md
  security.md
  report.md
```

The data directory is the app-support directory of the platform, for example `~/Library/Application Support/dev.neohar.app/default/` on macOS. The view renders the reports with `nh-rich`, as the Files tab renders Markdown. The view also gives the actions "Show in Finder" and "Copy to folder". "Copy to folder" copies the folder of the execution into the current folder, for example to keep the reports in the repository.

The executor removes the oldest executions that ended, and their files, after `workflows.keep_executions` (100).

## Read-only nodes

An agent node is read-only unless "Can edit files" is on.

- **Codex.** `--sandbox read-only`. A flag of the command line has priority over `sandbox_mode` in `config.toml` of Codex.
- **Claude Code.** `--disallowedTools Edit,Write,NotebookEdit`: the help of the flag permits a list with commas. Bash can still write files: Claude Code has no full read-only mode. The footer tells the agent not to edit. `--add-dir` lets the agent read the reports in the folder of the execution.
- **The manifests.** The harness manifests get `read_only_args` and `add_dir_arg`, as they have `effort_arg`. A new version of a harness changes the manifest, not the code.

A node with "Can edit files" uses the settings of the harness as they are. Its edits change the worktree in which you work. The header of `report.md` tells it.

## The file of a workflow

`~/.config/neohar/workflows/review.toml`:

```toml
name = "Review"

[[node]]
id = "start"
kind = "manual"
pos = [0, 200]

[[node]]
id = "code"
kind = "agent"
title = "Code review"
harness = "codex"
model = "gpt-5.5"
effort = "high"
inputs = ["start"]
pos = [300, 40]
prompt = """
Review the changes of {{start.branch}} against {{start.base}}.
Look for bugs, missing tests and code that is hard to read.
"""

[[node]]
id = "logic"
kind = "agent"
title = "Business-logic review"
harness = "claude"
inputs = ["start"]
pos = [300, 200]
prompt = """
The author describes the change so: {{start.notes}}
Check that the changes of {{start.branch}} against {{start.base}} do this, and only this.
"""

[[node]]
id = "security"
kind = "agent"
title = "Security review"
harness = "claude"
inputs = ["start"]
pos = [300, 360]
prompt = "/security-review"

[[node]]
id = "report"
kind = "report"
title = "Review report"
summary_by = "claude"
inputs = ["code", "logic", "security"]
pos = [640, 200]
```

- `inputs` holds the connections, so the file stays easy to read in a diff.
- `pos` is the position of the node on the canvas, in canvas units.
- A prompt is text, so it can start a skill of the harness, such as `/security-review`.
- The core refuses unknown keys and tells which key and which line.

For a chain, connect the nodes one after the other: `logic` gets `inputs = ["code"]`, and `security` gets `inputs = ["logic"]`.

## API

The methods go into `nh-proto` first, in a new `workflow` module, with the three generated outputs of AGENTS.md.

| Method | Use |
| --- | --- |
| `workflow.list` | The workflows, with the state of the last execution in a folder |
| `workflow.show` | One workflow: its nodes, its connections, its revision and its validation errors |
| `workflow.create` | A new workflow, empty or from a preset workflow ("Review") |
| `workflow.save` | Writes the graph. It takes the revision that it changes and refuses an old one |
| `workflow.rename`, `workflow.duplicate`, `workflow.delete` | The file operations |
| `workflow.undo`, `workflow.redo` | Moves through the saved versions |
| `workflow.layout` | The positions of "Tidy up" |
| `workflow.presets` | The node presets of the picker |
| `workflow.execute` | Starts an execution: workflow, folder, base, notes, and an optional node for "Execute this node" |
| `workflow.notes_from` | The prompts of an agent run, as text for the notes |
| `workflow.executions` | The executions, for one folder or for all |
| `workflow.execution` | One execution: the graph, the state of each node, the facts |
| `workflow.report` | One report as `rich::Markdown` |
| `workflow.retry`, `workflow.skip`, `workflow.cancel` | The actions on an execution |
| `workflow.continue` | "Continue in a tab": `agent.resume` of a node in a visible tab |

The projections `workflows` and `workflow_executions` send the changes to the windows. `workflow.facts` gives the facts of a folder for the dialog, `workflow.reveal` shows the hidden tab of a node, and `workflow.copy_to_folder` copies the reports. The events are `workflow.execution_started`, `workflow.node_changed` and `workflow.execution_ended`.

The CLI: `nh workflow list`, `nh workflow execute <name> [--folder <path>] [--base <ref>] [--notes <text>] [--node <id>] [--wait]`, `nh workflow executions`, `nh workflow show <execution>`, `nh workflow report <execution> [--node <id>]`, `nh workflow retry|skip <execution> <node>` and `nh workflow cancel <execution>`. Without `--folder`, the folder is the current directory. `--wait` makes `execute` wait for the end and print the path of `report.md`.

The config:

```toml
[workflows]
# The agents of one execution that run at the same time.
max_agents = 3
# The time limit of an agent node, in minutes, when the node has none.
time_limit_min = 60
# The executions to keep, with their reports.
keep_executions = 100
```

## How to build the canvas

- **No new dependency.** React Flow works only with React. React and React Flow together use much of the 300 KB budget of `scripts/check-bundle.ts`. The UI stays Solid.
- **HTML nodes, SVG connections.** The nodes are HTML cards in one container. The pan and the zoom move and scale that container, so the text stays sharp. The connections are Bézier paths in one SVG layer below the cards.
- **Pointer events.** Drags use `app/drag.ts`, because HTML5 drags never reach the page in `neohar.app`.
- **A lazy chunk.** The canvas and the node details load when the view opens, as the art of the Files tab does. The target is at most 20 KB gzipped.
- **The thin-presentation rule.** The core validates, sorts, lays out, writes the files and holds the states. The canvas geometry is view math in `packages/ui/src/workflows/model.ts`, with unit tests: port positions, connection paths, hit tests, zoom about the pointer and the grid snap.

## Work packages

| WP | Content | Done when |
| --- | --- | --- |
| 1 Model | The file format, validation, revisions, layout, presets, the `workflow.*` methods without executions, the generated outputs | A workflow file goes through save and load with no change, and the validation tests pass |
| 2 Executor | The store rows, the state machine, `Placement::Hidden`, the footer, the report capture, the prompt for a missing report, time limits, retry, skip, cancel, the Report node, the restart | A test with fake agents runs the "Review" workflow, with a restart in the middle |
| 3 Canvas | The view mode, the canvas, the picker, the node details, the left column | You can build the "Review" workflow with the pointer and the keys only |
| 4 Executions in the view | The states on the canvas, the right panel, the dialog, "Execute this node", "Continue in a tab", the notices | An execution of "Review" shows live in the view |
| 5 CLI and live test | The `nh workflow` commands, the manifest arguments, a live test, the guide page | The live test passes with Claude Code and Codex on this Mac |

## Tests

- **Rust.** Validation (cycles, references, nodes that the trigger cannot reach), the layout, the file round trip, and the executor with fake agents. Also a restart in the middle of an execution, insta snapshots of the footer and of `report.md`, and the time limit.
- **UI.** `bun test` of `workflows/model.ts`, and a Playwright spec (`.pw.ts`) that adds, connects and deletes nodes.
- **Live.** A local suite, gated by an environment variable: a workflow of two nodes, one Claude Code and one Codex, in a temporary repository. CI does not run it.

## Answers to the open items

The implementation answered the open items of the first version of this spec.

| Item | Answer |
| --- | --- |
| The hidden tab | The layout requires a tab for each pane, so a hidden pane is in a tab with the flag `hidden` (column `tabs.hidden`, migration 0019). The `tree` projection, `workspace.list`, the palette and the choice of the next selected tab skip it. `tab.select` shows it. The restore keeps the flag. |
| Hook events while the core is closed | `nh hook` drops the event when the socket does not answer, and no hook comes again. At its start, the executor reads the transcript of each run of a working node: a turn that ended gives its last message. The test `a_turn_that_ended_while_the_core_was_away_comes_from_the_transcript` does a crash and a restart. |
| The read-only flags | Claude Code 2.1.314: `--disallowedTools Edit,Write,NotebookEdit` and `--add-dir <folder of the execution>`. Codex 0.166.0: `--sandbox read-only`, which has priority over `sandbox_mode` of `config.toml`. They come from `read_only_args` and `add_dir_arg` of the manifests. A live run with the settings of this Mac is still to do. |
| The key | `mod+shift+e` is free in `defaults.toml`. The app takes a chord with `mod` before a pane, so no harness gets it. |
| The badge | An execution does not count in the badge. A node whose agent needs the person counts, because its run shows on the board and in `attention`. The Workflows switch of the status bar has a mark while an execution is paused or waits. |

## Later

- A live suite with Claude Code and Codex, gated by an environment variable, as `test:harness` is.
- Human check, If and Loop nodes, and the triggers of the scope table.
