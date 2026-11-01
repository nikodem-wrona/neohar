# neohar Hermes chat plan

Oct 31, 2026 · @Nikodem Wrona

neohar supervises [Hermes Agent](https://hermes-agent.nousresearch.com/docs) (Nous Research) in a chat view of its own. Hermes does not run in a terminal pane. The core speaks the Agent Client Protocol (ACP) to `hermes acp`, and the window paints what the core sends. The model provider of Hermes is the ChatGPT (Codex) subscription of the user.

## Decisions

| Question | Decision |
| --- | --- |
| The rule "Claude Code and Codex, and no other harness, no generic agent protocol" | Removed (user, 2026-10-31). Each harness has an adapter in the core. No plugin system, no adapter outside the core. |
| How neohar drives Hermes | ACP over stdio: `hermes acp`. Not the TUI in a pane, not the HTTP API server, not the WebSocket protocol of the gateway (it has no documentation). |
| The chat view | A new view only for Hermes. It does not use the chat view of PR #10 (`feat/chat-view`), which reads transcripts of PTY runs. |
| Where Hermes runs | On this Mac (user installs it). A remote machine comes later. |
| Model provider | `openai-codex` in Hermes. neohar does not touch credentials. |
| Where the view shows | It covers the panes, as the Git view and the Workflows view do. My choice; see the open questions. |

## What Hermes gives

Facts from the docs and from `acp_adapter/` of Hermes v0.22.5 (2026-10-23, MIT):

- `hermes acp` (also `hermes-acp`, `python -m acp_adapter`) is an ACP agent on stdio. Stdout has only JSON-RPC; the logs go to stderr. `hermes acp --version` and `hermes acp --check` exit at once.
- **The gateway owns the work.** `hermes acp` is only a viewer of the profile gateway daemon. It starts the daemon if no daemon runs (`connect_gateway` → `ensure_gateway_runtime`). The session continues in the daemon when the viewer stops. The daemon ends an ACP session only when its last viewer is gone and its queue is empty.
- **Reattach.** `session/load` and `session/resume` do the same thing: the viewer sends the live history again as `session/update` messages, and then sends again each approval that waits (`snapshot["prompts"]`). A turn that the daemon lost in a restart becomes `unknown`. Hermes never sends it a second time.
- **Methods:** `initialize`, `authenticate`, `session/new`, `session/load`, `session/resume`, `session/fork`, `session/list`, `session/prompt`, `session/cancel`, `session/set_mode`, `session/set_model`, `session/set_config_option`. Hermes runs the SDK with `use_unstable_protocol=True`.
- **Updates:** `agent_message_chunk`, `agent_thought_chunk`, `tool_call`, `tool_call_update` (with diffs and terminal output), `plan` (from the `todo` tool), `available_commands_update`, usage and session info.
- **Approvals:** `session/request_permission` with the options `allow_once`, `allow_session`, `allow_always` (Hermes keeps the allowlist), `deny` and `deny_always`. A timeout (`approvals.timeout`, 300 s by default) or an error denies.
- **Questions:** `clarify` arrives as `elicitation/create` (a form) when the client sets `elicitation.form` in `initialize`. Otherwise it arrives as a permission request with the choices and Skip.
- **Modes:** `default` (ask before edits), `accept_edits`, `dont_ask`.
- **Prompts during a turn:** Hermes puts them in a queue (`_drain_queued_prompts`), and `/steer` injects text into the running turn.
- **Files:** `~/.hermes/config.yaml`, `~/.hermes/.env`, `~/.hermes/auth.json`, `~/.hermes/skills/`, `~/.hermes/state.db` (SQLite with FTS5).

## Codex subscription

Do this one time, in a terminal, before the first chat:

1. Install Hermes on this Mac (the installer of the docs). Remove or rename the wrapper `~/.local/bin/hermes`: it runs `ssh -t` to the other Mac, and the PTY of `-t` breaks the JSON-RPC on stdio.
2. Run `hermes auth add openai-codex` (device code), or `hermes model` → "ChatGPT or Codex Subscription". The token goes to `~/.hermes/auth.json`.
3. Run `hermes acp --check`.

Hermes can import `~/.codex/auth.json`. Do not do that: a separate login keeps the refresh tokens of Codex CLI and Hermes apart (not verified that a shared one breaks, but nothing is gained). The use of Hermes counts against the same limits of the plan as the Codex panes. The docs do not give the limits, the plans that can use it, or the terms of service.

## How it works

### One process for each run

The core starts `hermes acp` as a child process, one for each Hermes run, with the cwd of the run and the login environment of the user (`login_env`), as for a pane. No holder: the gateway daemon of Hermes keeps the turn alive when the app quits, crashes or updates.

- **Start:** `initialize` with `clientCapabilities: { elicitation: { form: {} } }` and no `fs` or `terminal` capability (the tools of Hermes run on the gateway host). Then `session/new { cwd, mcpServers: [] }`.
- **Relaunch:** for each Hermes run that the last core left open, start `hermes acp` and send `session/resume { sessionId, cwd }`. The history and the open approvals come back as updates.
- **The child ends:** the run is `degraded`, and the core starts the child again with a backoff, then resumes. After three failures in one minute the run is `disconnected` until a person clicks Retry.
- **Stop of the run:** `session/cancel`, then close stdin of the child. The session stays in `state.db` of Hermes.

### The chat comes from the core

neohar keeps no copy of the conversation. The core keeps the items of each live Hermes run in memory and publishes them as a new projection `chat:<run>`. After a relaunch, `session/resume` fills it again.

- The core joins the chunks of a message. It renders the markdown with `nh-rich` (sanitized HTML), at most every 100 ms for a message that grows, and publishes the changed item only.
- **A chunk that repeats the message.** A viewer that did not send the prompt gets the deltas and then one chunk with the full text (spike). When a chunk starts with the text that the core already has for that message, the chunk replaces the text.
- **Item ids are neohar ids.** The `messageId` of Hermes is different for each viewer, and a replay has none (spike). The core gives each item an id in the order of arrival, and a resume builds the list again from the start.
- **Tool lines.** An edit approval has a tool call of its own (`edit-approval-N`) next to the real call (`call_…`). The chat shows only the real call; the approval is the card.
- Each item has an `id`, a `kind` (`prompt`, `assistant`, `thought`, `tool`, `plan`, `notice`), the HTML or the fields of the tool (`title`, `status`, `kind`, the diff, the folded output) and a `turn`.
- The window does not join chunks and does not parse markdown (the thin-presentation rule).

### Send, stop, steer

- `agent.prompt` for a Hermes run sends `session/prompt` with one text block. The `input_id` of the call goes into the delivery record: a call with the same id does nothing. When the core cannot know if Hermes got the prompt (the child ended before the answer), the state is `delivery_unknown`, and the core does not send it again.
- While a turn works, a send goes to the queue of Hermes. The chat shows it as "queued". `/steer <text>` goes as text; the chat offers it as a second button.
- `agent.interrupt` sends `session/cancel`.
- A slash command is text. The core keeps the list from `available_commands_update`, and `hermes.commands` gives it, filtered and sorted by the core (no file reads).

### Approvals and questions

These are interactions, as for the other harnesses, with the native channel only (`answer_keystroke: false`).

| ACP | neohar |
| --- | --- |
| `session/request_permission` | `approval`. The payload gets the options of the request (a new field `options`, appended). The card shows the buttons of those options. The answer is the JSON-RPC result `{ outcome: { outcome: "selected", optionId } }`. |
| `elicitation/create` | Later. The `clarify` tool is not in the ACP toolset of Hermes 0.22.5 (spike), so no question comes. When it comes: `question`, the `enum` as options, `multi_select` from an `array` schema, free text when there is no `enum`; the answer is `{ action: "accept", content: { answer } }`. |
| the request ends (timeout, cancel, new viewer) | `resolved_elsewhere` or `cancelled`. |

- **What asks.** File edits ask in the mode `default` (`edit_approval_policy: ask`), with the diff (`path`, `oldText`, `newText`) and the options `allow_once` and `deny`. A dangerous shell command asks only when `approvals.mode` of Hermes is `manual`. With `smart` (the config on this Mac), a model of Hermes approves it and no request comes (spike: `rm -rf ./junk` ran). `hermes.check` reads the mode, and the view says when shell commands do not ask.
- **Deadline:** Hermes denies an approval after `approvals.timeout` (300 s by default), also with no viewer (spike). The core sets `deadline` of the interaction to the time of the request plus that timeout, and the card shows it. When the deadline goes by, the interaction becomes `cancelled` with the reason "Hermes denied it after the timeout".
- **Gate:** there is no TUI that could show a dialog, so the core always holds the request. The focus rules do not change for panes.
- **Delivery:** an answer goes in the recoverable delivery transaction, as today. The JSON-RPC result is the delivery. If the child ends before the result, the request comes again with the resume, with the same `toolCallId` (spike) and a new JSON-RPC id. The core matches it to the open interaction by `toolCallId` and `rawInput`, and delivers the recorded decision. If there is no match, the old interaction becomes `superseded` and the new one opens.
- **Policy:** the rules of `policy` do not answer Hermes approvals in v1. The card shows `policy_note` when a rule matches.
- Attention, the board, notifications and the phone use the interactions and the run, so they work with no change.

### The run

A Hermes run is a `Run` with `harness: hermes`, `pane: None`, `binding: bound`, the session id of Hermes in `sessions`, and these capabilities: `observe`, `gate`, `answer_native: [approval]`, `resume`, `steer`, `survive_disconnect`. The facets come from ACP: `execution` is `working` from `session/prompt` until its result, and `idle` after it (`error` for a failed turn, `unknown` for an `unknown` turn). The `liveness` follows the child process. The model and the mode come from `configOptions` of the session response; `session/set_config_option` changes them.

**A turn that a resume found.** ACP gives no signal of the start or the end of a turn to a viewer that did not send the prompt (spike). After a resume, when the replay ends in an open turn (a prompt or a tool call with no assistant message after it), `execution` is `working` with the source `heuristic` and a low confidence. It goes to `idle` when an assistant message is followed by a `usage_update` and no tool call starts for 3 s. The next `session/prompt` result of neohar sets it with full confidence again. Ask Hermes for a turn state in `session_info_update` (an issue upstream).

`Harness` has 487 uses in 94 files, and most of them are for PTY runs (manifests, screens, keystrokes, transcripts, hooks). A Hermes arm in each `match` either refuses with `harness_mismatch` or does nothing. `agent.start` and `agent.resume` with `harness: hermes` start a Hermes run; `placement` must be absent.

### The view

- `packages/ui/src/hermes/`: `HermesView.tsx` (it covers the panes, as `WorkflowsView` does), `SessionList.tsx`, `ChatThread.tsx`, `Composer.tsx`, and `model.ts` with its tests.
- Left: the Hermes runs of this machine, and "Earlier sessions" from `session/list` (one click resumes one as a new run). Middle: the thread. The composer has a select for the model, a select for the mode, Send, Steer and Stop.
- The cards of the open interactions show at the end of the thread with the components of `cards/`.
- shadcn/ui only: `scroll-area`, `textarea`, `select`, `button`, `badge`, `card`, `command` (for `/`, with `shouldFilter={false}`).
- The chord `hermes_open = "mod+shift+h"`, the palette command `hermes.open` ("Open the Hermes chat"), and a button in the top bar.
- The board shows a Hermes run as a strip. A click on it opens the view on that run.

## API changes

All are additions, so a `feat` (minor). Each change of `nh-proto` needs `mise run gen:api`, the insta snapshot and `bun run gen` in `website/` in the same commit.

- `Harness::Hermes`, `Transport::Acp` (in `nh-agents`).
- `chat:<run>` projection: `ChatItem { id, turn, at, body: ChatBody }`, `ChatBody` = `Prompt { html, queued }`, `Assistant { html, streaming }`, `Thought { html }`, `Tool { title, kind, status, diff?, output?, truncated }`, `Plan { entries: [{ text, status }] }`, `Notice { text }`.
- `ApprovalPayload.options: Vec<ApprovalOption { id, label, kind }>`, appended with `#[serde(default)]`. Empty for Claude Code and Codex.
- `interaction.answer` takes `option: String` for a Hermes approval.
- `hermes.commands { run, query? } -> { commands: [{ name, description, insert }] }` from `available_commands_update`.
- `hermes.sessions { cwd? } -> { sessions: [{ id, title, cwd, updated_at }] }` from `session/list`.
- `hermes.set { run, model?, mode? }` sends `session/set_config_option`.
- `hermes.check {} -> { found, version, ok, approvals_mode, message }` runs `hermes acp --version` and `--check`, and reads `approvals.mode` of `~/.hermes/config.yaml`. The Settings page and `nh doctor` show it.
- Config: `[hermes] enabled = true`, `command = "hermes"`, `args = ["acp"]`, `reconnect_limit = 3`.

## Work packages

0. **Rule.** Done in the working tree (2026-10-31): AGENTS.md, README.md, `website/docs/intro.md`, `.claude/skills/pr-review/SKILL.md`. Commit it as `docs:` with the plan.
1. **Spike.** Done on 2026-10-31; see "Spike results". The recorded frames are not in the repository yet: WP2 records them again into `tools/fixtures/hermes/*.jsonl` with the paths of a temporary folder.
2. **Codec.** `nh-agents/src/hermes/`: the ACP types that neohar uses (serde), the map from updates to chat items and facets, and from requests to interactions. No I/O. Unit tests that read the fixtures.
3. **Core.** `nh-core/src/hermes/`: the child process task (spawn, stdio framing, request ids, backoff, resume), the run, the `chat:<run>` projection, the delivery of answers, the calls, the config. Tests in `crates/nh/tests/hermes.rs` with a fake ACP agent (`tools/fake-acp`, a small Rust binary that plays a fixture), so CI runs them without Hermes.
4. **UI.** `packages/ui/src/hermes/` as above, the chord, the palette command, the strip on the board.
5. **Live suite and guide.** `mise run test:hermes`, gated by `NEOHAR_HERMES=1`, with the real `hermes acp` (local only, never in CI). A guide page `website/docs/agents/hermes.md`. Update the "supports Claude Code and Codex" sentences in AGENTS.md and `website/docs/agents/overview.md`.

## What v1 does (2026-11-01)

The implementation is smaller than the plan below. These are the differences:

| Plan | v1 |
| --- | --- |
| A Hermes chat is a `Run` with `harness: hermes`, on the board, in `attention` and with interactions | A module of its own, `nh-core/src/hermes/`, as the executor of the workflows is. `Harness` has 487 uses in 94 files, and most are for PTY runs. The chats are in `hermes_chats` and `hermes_chat:<chat>`, and an approval is an item of the chat, answered with `hermes.answer`. The status bar marks the **Hermes** switch while a chat waits. The board, `attention`, notifications and the phone come later. |
| One `hermes acp` for each chat | One `hermes acp` for every open chat: ACP names the session in each message. It starts with the first chat and stops after the last one closes. |
| `elicitation/create` for questions | Not sent: the `clarify` tool is not in the ACP toolset. The core does not offer `elicitation` in `initialize`. |
| `hermes.commands`, `hermes.sessions` | Not in v1. A slash command is text. |
| The codec in `nh-agents/src/hermes/` | `nh-core/src/hermes/thread.rs`: no I/O, unit tests in `tests.rs`. |
| `session/set_config_option` | `session/set_model` and `session/set_mode`. |

The calls: `hermes.check`, `hermes.start`, `hermes.prompt`, `hermes.cancel`, `hermes.answer`, `hermes.set`, `hermes.retry`, `hermes.close`. The table `hermes_chats` (migration 0021) keeps the open chats and their session ids. `crates/nh/tests/hermes.rs` drives the core with a bash `hermes` that speaks ACP; `mise run test:hermes` drives it with the real Hermes.

## Spike results

On 2026-10-31, with Hermes 0.22.5+3133 (`openai-codex`, `gpt-6.1-sol`) and a Python ACP client that recorded each frame. Each scenario used an empty temporary folder.

| Question | Result |
| --- | --- |
| Start time | `initialize` 0.55 s, `session/new` 0.35 to 0.9 s, `session/resume` 0.4 s. A plain answer took 3 to 4 s. A short "starting" state is enough. |
| The viewer is killed during a turn | The turn continues in the gateway. The resume sends the prompt and the open tool call again, and then the live updates come (the tool result, the answer). No signal tells the new viewer that the turn ended. |
| The viewer is killed during an approval | The resume sends the same request again, with the same `toolCallId` (`edit-approval-1`). A deny from the second viewer reached Hermes, and the file was not written. |
| An approval with no viewer for 330 s | Denied. After the timeout of 300 s Hermes denied the edit (its text says "denied by ACP client"), the file was not written, and the turn continued. The resume sent no request. |
| Two viewers | Both get the live answer of a turn. The viewer that did not send the prompt does not get the prompt itself (no `user_message_chunk`), and it gets the full text again in the last chunk. |
| `session/list` | It gives only the sessions that ACP made. A session of `hermes chat` in the same folder is not in the list. |
| Shell approvals | Not sent with `approvals.mode: smart`. The model of Hermes approved `rm -rf ./junk`. |
| Edit approvals | Sent in the mode `default`, with the diff. The options are `allow_once` and `deny`. |
| `clarify` | Not in the ACP toolset. The model searched for the tool and did not find it. |
| Plan | The `todo_list` tool sends `plan` updates with `content`, `priority` and `status`. |
| Usage | `usage_update { size, used }` is the context window. The `session/prompt` result has the tokens of the turn. |
| Model and mode | `configOptions` of the session: `edit_approval_policy` (ask, workspace_session, session) and the model, with each provider that has a login. |

## Open questions

- Placement: should a Hermes chat also be able to sit in a tab next to terminal panes? That needs a pane kind with no PTY, which is a large change of the layout. Not in v1.
- Should neohar ask the user to set `approvals.mode: manual`, so that shell commands ask in the chat? It changes Hermes on all its surfaces.
- Should the user raise `approvals.timeout` (for example to 3600 s), so that an approval waits while the person is away? Hermes denies after 300 s, also when neohar is closed. A longer timeout also holds the turn of Hermes for that time.
- An issue upstream for a turn state, and for the repeated last chunk.

## Not in v1

- Hermes on a remote machine. Later: `ssh <host> hermes acp` (no `-t`) through the link of `nh-remote`.
- Hermes nodes in workflows.
- MCP servers that neohar gives to Hermes (`mcpServers` in `session/new`).
- Images in prompts, edit or rewind of messages, fork.
- Other ACP agents. The codec is Hermes-only, and it accepts no other command.

## Risks

- Hermes changes fast: the repository gets commits each day, and it uses the unstable part of ACP. The core reads `hermes acp --version`, and a version range in the code gives the capabilities, as for the other harnesses. A version outside the range is `unvalidated_version`.
- The terms of OpenAI for the use of a Codex login by a third-party client are not documented.
