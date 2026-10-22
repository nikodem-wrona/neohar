# Harness recordings

Recordings of the real Claude Code and Codex binaries, made offline by `tools/nh-golden` against the mock model (`tools/mock-llm`). The adapters in `crates/nh-agents` replay these files in their tests. A capability of a harness is granted only when a recording in this directory shows it (`crates/nh-agents/golden-coverage.toml`).

## Layout

```
scenarios/<harness>/<name>.toml          the script of one recording
<harness>/<version>/<name>/
  session.cast                           asciicast v2: the output of the harness, with the time it arrived
  session.jsonl                          (instead of session.cast, in pipe mode) every line in and out, with its time and the peer
  signals.jsonl                          the hooks, as the shim wrote them (NEOHAR_HOOK_TEE)
  expected.jsonl                         what the adapter makes of those hooks: for each hook the event, the reply to the shim and the signals (`crates/nh-agents/tests/golden_replay.rs`; NEOHAR_BLESS=1 writes it)
  llm-requests.jsonl                     every request that the harness sent to the mock model, without its body and headers
  screens/<step>.txt                     the screens that the scenario marks with `snapshot`, and `final`
  processes/<step>.txt                   the process lists that the scenario marks with `processes`
  transcripts/transcript-<n>.jsonl       the transcript files that the harness wrote (fixtures for the adapters)
  core/interactions.json, core/events.jsonl  (scenarios with a `[core]`) the interactions of the core at the end, and the steps that the scenario did
  scenario.toml                          a copy of the script that made the recording
  meta.toml                              harness version, operating system, hash of the script, counts
```

## Scenarios

| Harness | Scenario | What it shows |
|---|---|---|
| Claude Code | `startup` | Start, prompt, one text answer, exit |
| Claude Code | `permission-allow`, `permission-deny`, `permission-deny-interrupt` | A `PermissionRequest` hook allows, denies with a message, or denies and stops the turn |
| Claude Code | `allow-always`, `allow-always-wrong-fields` | `updatedPermissions` with `addRules`: the rule takes effect, and wrong field names are rejected |
| Claude Code | `question-native`, `question-hook` | `AskUserQuestion` answered in the dialog, and answered by a `PreToolUse` hook (`updatedInput`) |
| Claude Code | `plan-deny`, `plan-allow` | `ExitPlanMode` in plan mode: a hook denies with feedback, and a hook allow does not close the dialog |
| Claude Code | `marker` | An extra key in every hook object |
| Claude Code | `elicitation-hook`, `elicitation-hook-decline`, `elicitation-native` | An MCP server asks for input; a hook answers (accept, decline), or the user answers in the dialog |
| Claude Code, headless | `headless-cost` | Two turns; `total_cost_usd` is a running total |
| Claude Code, headless | `headless-allow`, `headless-deny`, `headless-deny-interrupt`, `headless-allow-always` | `can_use_tool` requests answered with allow, deny, deny and interrupt, and allow with a rule |
| Claude Code, headless | `headless-question`, `headless-plan`, `headless-plan-allow` | `AskUserQuestion` and `ExitPlanMode` as `control_request`s |
| Claude Code, headless | `headless-hook-first`, `headless-interrupt` | `control_cancel_request` after a hook answered first, and a client `interrupt` |
| Claude Code | `pretool-deny`, `pretool-ask`, `pretool-allow-skips-dialog` | A `PreToolUse` hook denies, asks, or allows (and `PermissionRequest` does not fire) |
| Claude Code | `resume` | `claude --continue` after the `startup` run (`after`) |
| Codex | `startup` | Start, trust the hooks, prompt, one text answer, exit |
| Codex | `embedded`, `embedded-disable-flag` | The process list at the prompt: one process; `--disable daemon_auto_start` changes nothing |
| Codex | `yolo-hooks` | `approval_policy = "never"` and `danger-full-access`: the hooks that fire when a command runs |
| Codex | `approval-native`, `approval-hook-allow`, `approval-hook-deny` | `on-request` with an escalated command: the dialog, and a `PermissionRequest` hook that allows or denies |
| Codex | `pretool-deny`, `pretool-allow` | A `PreToolUse` hook for Bash |
| Codex | `resume` | `codex resume --last` after the `startup` run (`after`) |
| Claude Code, Codex | `reply-after-turn` | A turn ends, and a bracketed paste of two lines and Enter start the next turn with one prompt: what `agent.reply` writes. Recorded with Claude Code 2.1.303 and Codex 0.163.0. Codex runs with `--no-daemon`, because the sandbox refuses the `ps` of its shared daemon |
| Claude Code | `delivery-core-killed`, `delivery-shim-killed`, `delivery-two-answers`, `delivery-answered-in-tui`, `delivery-deadline` | The five delivery cases: a real core holds the `PermissionRequest` hook; the core ends, the shim ends, two clients answer, the user answers in the dialog, the deadline passes |
| Claude Code | `delivery-dialog-while-held` | The dialog of Claude Code is on the screen while the hook is held |
| Codex | `delivery-held`, `delivery-release` | A held `PermissionRequest` hook shows no dialog, and a release opens it |
| Codex app-server | `observer-answer`, `observer-second-answer`, `observer-read` | A second WebSocket client joins a running thread, answers an approval, is too late with an answer, or only reads |

## Commands

```bash
mise run record:golden   # record every scenario of each installed harness into this directory
mise run test:harness    # record each harness again and compare with the committed recordings
```

Both need macOS and the harnesses on `PATH`. The recorder runs the harness under `sandbox-exec`, so the harness cannot write outside a scratch tree, cannot read or write the real `~/.claude`, `~/.claude.json` or `~/.codex`, and cannot use the network except the loopback. The format of a scenario is in `tools/nh-golden/src/scenario.rs`. The isolation recipe is in `tools/nh-golden/src/recipe.rs`.

## Rules

- A recording is made by the recorder and never edited by hand.
- Text that changes from run to run (a clock, a session id, a verb that the harness picks at random) is hidden by a `[[mask]]` of the scenario, only when two recordings are compared. The files keep what the harness showed.
- A new version of a harness gets a new directory. Keep the old one while the adapter supports that version.
- The recordings hold no text of the vendor of a harness. The recorder removes the body and the headers of each request and the attachments of a Claude Code transcript (the system prompt, the tool descriptions, the lists of skills and agents). `nh-golden scrub <dir>` does the same to recordings that are already there.
- The recordings hold no user name and no path of the user. The working directory of every harness is `/private/tmp/nhg-<harness>/work`.
