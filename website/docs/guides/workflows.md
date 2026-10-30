---
title: Workflows
description: "Connect Claude Code and Codex agents on a canvas, execute the workflow in a folder, and read one report: the Workflows view, the executions and nh workflow."
sidebar_position: 5
---

A **workflow** is a graph of nodes that you connect on a canvas, as in n8n. An agent node gives
its prompt to Claude Code or Codex, and the last message of the agent is the report of the node.
A Report node writes one report with links to the reports of its inputs. An **execution** runs a
workflow in a folder. An example is the preset "Review": a code review, a business-logic review
and a security review of your branch, then one report.

## Open the view

To open the Workflows view, use one of these:

- Press `⌘⇧E`, or click **Workflows** in the status bar. Do the same, or press Escape, to close it.
- Type "Workflows" in the palette.
- Right-click a strip of the board, and click **Execute a workflow here**. The view opens on the
  folder of the pane of that agent.

The view takes the middle of the window, as the Git view does. The side columns fold to their
rails, and the panes continue to run. The phone app has no Workflows view.

| Part | What it shows |
|---|---|
| Top bar | The **Folder** menu with the branch, the name of the workflow (click it to rename the workflow), **Undo**, **Redo**, **▶ Execute workflow** and **Close** |
| Left column | **Workflows**: the list, **+ New**, **Duplicate** and **Delete**. **Executions**: the executions in **This folder** or in **All folders**, with their state and time |
| Middle | The canvas of the workflow or of the execution that you selected |
| Right panel | The execution that you selected. See [The execution panel](#the-execution-panel) |

The folder is the folder of the window: the folder of the pane with the focus, else the root of
the workspace. The Folder menu also lists the folders of the panes and the roots of the
workspaces. Type an absolute path in its filter to use another folder.

## Build a workflow

Click **+ New**, type a name, and select **Only the trigger** or the preset "Review". The
workflows are personal: each folder shows the same list.

| Node | What it does |
|---|---|
| Manual trigger | The start. Each workflow has exactly one. It gives the facts of the execution to the nodes after it |
| Agent | Claude Code or Codex does its prompt. The last message of its turn is the report of the node |
| Report | It writes a report from the reports of its inputs. **Summary by** Claude Code or Codex adds a summary at the top |

A node starts when each of its inputs is done or skipped. Nodes without a connection between them
run at the same time, at most `workflows.max_agents` (3) agents for each execution.

On the canvas:

- Drag from the output port of a card to another card to connect them. Drag to an empty area to
  open the node picker. The new node connects to that port.
- Press Tab, right-click the canvas, or click **+ Add** in the toolbar to open the node picker. It
  has **Claude Code agent**, **Codex agent**, **Report** and
  the presets **Code review**, **Business-logic review** and **Security review**. Type to filter,
  use the arrow keys, and press Enter.
- Drag a card to move it. Select a card or a connection, then press Delete to remove it. You
  cannot remove the trigger.
- Drag an empty area, or scroll, to pan. Pinch, or scroll with `⌘`, to zoom. The toolbar has
  zoom buttons, **Fit** (`⌘0`) and **Tidy up**, which puts the nodes in columns.
- Press `⌘Z` and `⌘⇧Z` to undo and redo. The core keeps the last 50 versions in memory.
- Double-click a card to open its details.

Each change saves at once, and there is no Save button. The core finds the errors, and the canvas
shows them. You can save a workflow with errors, but you cannot execute it.

### Node details

| Column | What it shows |
|---|---|
| Input | The inputs of the node, with their reports from the last execution in this folder. Click a chip to put its reference into the prompt |
| Settings | The title, the harness, the model, the effort, **Can edit files**, the time limit and the prompt. A Report node has **Summary by** and an optional summary prompt |
| Output | The report of the node from the last execution in this folder, and **Execute this node** |

A prompt can contain these references. The core puts in the values when the node starts.

| Reference | Value |
|---|---|
| `{{start.folder}}`, `{{start.branch}}`, `{{start.base}}`, `{{start.head}}` | The facts of the execution |
| `{{start.notes}}` | The notes of the execution |
| `{{<node>.report}}` | The absolute path of the report of an input node |

The core adds a footer to each prompt: the paths of the input reports and the rules. The agent
must not ask questions, and it must end its turn with its full report. The last line of the
report is "Verdict: pass", "Verdict: warn" or "Verdict: fail". Without **Can edit files**, the
footer also tells the agent not to edit files.

**Execute this node** executes only that node. Its inputs are the reports of the last execution in
this folder. If an input has no report, execute the full workflow first.

## Execute a workflow

Click **▶ Execute workflow**. The dialog has these fields:

- **Base of the changes.** The default is the default branch of `origin`, for example
  `origin/main`, else `main` or `master`.
- **Notes.** What you built and why. **Take the notes from the prompts of an agent** fills the
  field with the prompts of an agent of the board.

neohar reads the branch, the `HEAD` commit, the base and a hash of the changes only in a git
repository that you trust ([Trust a repository](/docs/guides/files-and-changes#trust-a-repository)).

The agent of each node runs in a hidden tab of a workspace. The tab bar does not show that tab.
The board shows the agent only while it needs you: an approval, a question, a plan review, an
error or a rate limit. The end of its turn is not a wait and gives no notification.

| Node setting | What the agent gets |
|---|---|
| Read-only (the default) | Claude Code: `--disallowedTools Edit,Write,NotebookEdit`. A Bash command can still write files. Codex: `--sandbox read-only` |
| **Can edit files** | The settings of the harness, as they are. Its edits change the files of the folder |

Claude Code also gets `--add-dir` with the folder of the reports, so it can read the input
reports. A turn without a "Verdict:" line gets one reminder. After the next turn, the core takes
the message as it is, with the verdict "unknown". When a node is done, the core closes its hidden
tab, which stops the agent.

- **A failure.** A node fails when its agent does not start, ends before its report, stops with
  an error or a rate limit, or works for longer than its time limit (`workflows.time_limit_min`, 60 minutes). The
  other branches continue. When no other node works or can start, the execution pauses.
- **A restart.** If the app quits during an execution, the agents continue in their holders. At
  the next start, the core reads the turns that ended from the transcripts and continues the
  execution.
- **Notices.** When an execution is done or pauses and no window has the focus, a native
  notification shows. **Workflows** in the status bar has a dot while an execution is paused or
  waits for you.

## The execution panel

Click an execution in the left column. The canvas shows the state of each node, and the panel
shows the execution.

| Action | What it does |
|---|---|
| **Cancel** | Stops each agent of the execution that is in a hidden tab. An agent in a tab that you opened stays. The reports stay |
| **Show in Finder** | Shows the folder of the execution |
| **Copy to folder** | Copies the reports into a folder under `workflow-reports/` in the folder of the execution |
| **Retry** | Starts a new agent for a failed node, with the same prompt |
| **Skip** | Skips a node that failed or waits. The nodes after it can start, and their footer tells that the input has no report |
| **Open as tab** | Shows the hidden tab of an agent that works or needs you in the tab bar. The tab stays after the node is done |
| **Continue in a tab** | Starts the agent of a node again with its conversation, in a new tab |

Click a node to see its report. While its agent works, the panel shows the screen of the agent as
text.

The reports are in the data directory, for example
`~/Library/Application Support/dev.neohar.app/default/workflows/executions/20261030-141205-review-3/`.
That folder has `graph.toml` (the workflow at the start), one `<node>.md` for each node, and
`report.md` for the Report node of the preset. The core keeps the last `workflows.keep_executions`
(100) executions that ended, and removes the older ones with their files.

## The files

Each workflow is a TOML file in `~/.config/neohar/workflows/`, for example `code-review.toml`.
The view saves each change through the core. The core watches the folder, so an edit by hand
shows at once. The core refuses an unknown key and names its line.

```toml
name = "Code review"

[[node]]
id = "start"
kind = "manual"
pos = [0, 140]

[[node]]
id = "code"
kind = "agent"
title = "Code review"
harness = "codex"
inputs = ["start"]
pos = [300, 0]
prompt = "Review the changes of {{start.branch}} against {{start.base}}."
```

## The command line

```bash
nh workflow list
nh workflow execute review --base main --notes "Add the refund flow" --wait
nh workflow executions --folder "$PWD"
nh workflow show <execution>
nh workflow report <execution> --node code
nh workflow retry <execution> code
nh workflow skip <execution> code
nh workflow cancel <execution>
```

`review` is the name of the file without `.toml`. `execute` uses the current directory, or
`--folder`. `--wait` waits for the end and prints the path of `report.md`. A paused or cancelled
execution makes it fail. `<execution>` is the id that `nh workflow executions` gives.
