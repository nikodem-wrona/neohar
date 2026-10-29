# neohar chat view plan

Oct 28, 2026 · @Nikodem Wrona

One chat interface for Claude Code and Codex. You send messages, start skills and answer questions in a view that looks the same for both harnesses. One key switches the pane back to the real TUI, at any time, also in the middle of a turn.

## Decisions

| Question | Decision |
| --- | --- |
| How neohar drives the harness | The uniform PTY path for both harnesses. The TUI keeps running in its pane. The chat reads the transcript and the hooks, and it sends text as a paste. The Codex app-server comes later, after a spike. |
| Where the chat lives | A toggle inside the pane: Chat or Terminal. Not a drawer tab, not a separate panel. |
| Default view of a new agent pane | Terminal. |
| Scope | KISS and YAGNI. No edit or rewind of earlier messages, no fork, no token streaming. |

## One process, two views

The chat is a second view of the same run, not a second way to run it. The holder, the PTY and the harness process do not change when you switch. Headless modes (`claude -p --input-format stream-json`, `codex exec`) are not used: each switch would stop the process and resume it, only when the agent is idle, with a restart of the MCP servers.

## What the MVP does

| Area | MVP | Later |
| --- | --- | --- |
| Read | Your prompts and the full assistant messages as markdown, tool calls as one line with status and duration, tool output folded, diffs, subagents, compaction marks, a "working" line with the current tool | Codex token streaming through the app-server |
| Send | Enter sends when the agent is idle and has no open interaction; Shift+Enter makes a new line | Queue or steer while the agent works |
| Interrupt | A Stop button, and Esc in an empty composer, while the agent works | |
| Skills | `/` at the start of the composer opens a list of the skills and custom commands of the harness; a pick inserts the right text | Built-in commands with their own menus |
| Questions, plans, approvals | The cards of the open interactions of the run, at the end of the chat, answered as the cards answer today | Codex questions through the app-server |
| Switch | A button in the pane title bar, a palette command and a reserved chord | |
| Phone | The same toggle and view, without phone-specific work | A layout for the phone |

## How it works

### Read

The chat follows `timeline:<run>`, as the Timeline tab does. An item of the projection has a preview of 300 characters only. A new call `timeline.texts` gives the full text of up to 100 `prompt` and `assistant` items as sanitized HTML, with one read of the transcript file. Today `timeline.item` reads the whole file for one item, which is too slow for a chat of many messages. The chat asks again for an item when its preview changes, as `LastMessage.tsx` does. neohar still keeps no copy of the transcript.

Claude Code writes its transcript once for each message block, so a Claude message shows when the block is complete. The "working" line comes from the execution facet and the last running tool item.

### Send

The composer calls `agent.reply` with a new `input_id`. This call exists: it pastes the text and presses Enter at most once for an id, and it refuses with `not_idle` when the agent works or waits for an answer. The composer shows that state and sends nothing then. `agent.interrupt` stops a turn.

Any text goes as it is, so `/compact` or `/clear` work from the composer. A command that opens a menu of the TUI (`/model`, `/resume`, `/mcp`) needs the terminal. The composer says so under the field.

### Skills

A new call `agent.skills { run, query }` gives the skills and custom commands that the harness of the run can start in the folder of the run. The core reads the files, filters by `query` and sorts. Each row has a `name`, a `description` (from the front matter of `SKILL.md` or of the command file), a `source` (user, project or plugin) and the `insert` text.

| Harness | Where the core looks | `insert` |
| --- | --- | --- |
| Claude Code | `<CLAUDE_CONFIG_DIR>/skills/*/SKILL.md`, `<CLAUDE_CONFIG_DIR>/commands/*.md`, `.claude/skills/*/SKILL.md` and `.claude/commands/*.md` in the folder of the run and its parents up to the git root, the `skills/` and `commands/` of each enabled plugin as `plugin:name` | `/name ` |
| Codex | `<CODEX_HOME>/skills/**/SKILL.md` (with `.system`), `~/.agents/skills/*/SKILL.md`, `.agents/skills/*/SKILL.md` in the folder of the run and its parents up to the git root | `$name ` |

The discovery is a module of `nh-agents` with no I/O in its parsers, as the transcript module is. The core does the reads.

### Questions, plans and approvals

No change in the core. The gate holds an interaction only for a pane that is not focused. A pane counts as focused only when the DOM focus is inside its `.pane-surface` (`packages/ui/src/focus/report.ts`). In chat view the surface is not there, and the composer has the focus. So the core holds a question, a plan review or an approval, and the chat shows its card. The card answers through the native channel: for Claude Code, `AskUserQuestion` is answered in the hook `PreToolUse`. When you switch to the terminal, the surface gets the focus, the gate releases the hook, and the dialog of the harness shows, as today.

Codex sends no question through its hooks. In the uniform path a Codex question shows only in the TUI, or as a notice of the screen engine. The chat shows such a notice with a "Terminal" button.

### Switch

- The view of each pane is window state in `app/view.ts`, next to the state of the Git view: a set of the panes that show the chat, and the draft of each composer. It is not business state, and a relaunch shows the terminal again.
- In chat view, `PaneBox` shows `ChatView` in place of `Surface`. The Git view already removes and adds the surfaces of all panes, so the renderer handles this. When the surface comes back, the next frame asks for a keyframe, and the screen comes back.
- The PTY keeps its size: no surface means no resize.
- The button shows only for a pane with a run. The palette command is "Show the chat or the terminal of this pane" (`pane.view_toggle`, a command of the page). The reserved chord is `pane_view_toggle = "mod+shift+t"`.
- A path that drops on a pane in chat view goes into the field, not into the TUI.

## API changes

Both are additions, so they make a minor release (`feat`). Each change of `nh-proto` needs `mise run gen:api`, the insta snapshot and `bun run gen` in `website/` in the same commit (see AGENTS.md).

- `timeline.texts { run, items: [string] } -> { texts: [{ item, html, truncated }] }`: at most 100 items, the same scope as `timeline.item`. An item that is not in the file is not in the answer.
- `agent.skills { run, query? } -> { skills: [{ name, description, source, insert }] }`: the same scope as `agent.choices`.

## Work packages

1. **Core.** `timeline.texts` in `crates/nh-core/src/transcripts/calls.rs`, with one scan of the file for all the items. `agent.skills` with the discovery module in `nh-agents` and the call in `crates/nh-core/src/agent_calls.rs`. Unit tests for the parsers, and an API test under `crates/nh/tests/` with skills in a temporary `CLAUDE_CONFIG_DIR` and `CODEX_HOME`.
2. **UI.** `packages/ui/src/chat/`: `ChatView.tsx`, `Composer.tsx`, `SkillList.tsx`, and `model.ts` with its tests. The chat uses the helpers of `timeline/model.ts` and the cards of `cards/`. The toggle in `PaneBox.tsx`, the state in `app/view.ts`, the palette command and the chord, and `styles/chat.css`.
3. **Check on the real harnesses and the guide.** The checks below in `mise run app:dev` with Claude Code 2.1.311 and Codex 0.164. A short section in the guide.

## Checks before the merge

- A pasted `$name args` starts the skill in Codex 0.164. If not, find the text that does, and change `insert`.
- A pane goes Terminal → Chat → Terminal during a turn: the screen comes back, the size does not change, and the agent does not notice.
- Claude `AskUserQuestion` with multi-select: the card shows in the chat, and its answer reaches Claude. After a switch to the terminal, the dialog of the TUI shows.
- `ExitPlanMode`: the plan card shows and its answer reaches Claude.
- The skill list of a Claude pane has the same skills and commands as the `/` menu of its TUI.

## Known limits of the MVP

- Text that you typed in the TUI and did not send joins the text of a send from the chat.
- A Claude message shows when its block is complete, not word by word.
- No send while the agent works.
- A Codex question shows only in the terminal.
- The core sends frames for a pane in chat view, and the window decodes them. The cost is small. A later change can tell the core which surfaces are not shown.

## Later: the Codex app-server

`codex app-server --listen unix://PATH` with the TUI started as `codex --remote unix://PATH`. neohar is the second client of the same thread. That gives `item/agentMessage/delta`, `turn/start` with `clientUserMessageId`, `turn/steer`, `skills/list` and a `skill` input, and `item/tool/requestUserInput` with `serverRequest/resolved`. A server for each pane, started with the environment of the pane, also removes the hazard of hooks bound to the wrong pane. `Transport::AppServer` already has a name in `crates/nh-agents/src/adapter/mod.rs`. Open before a decision: does a second client get the events of a thread that the TUI started; which client gets the requests of the server; how the server survives a quit of the app (it must live under a holder); the protocol is marked experimental.
