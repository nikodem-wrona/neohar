---
title: Interactions and delivery
description: How an approval, a question or a plan review becomes an interaction, when the core holds the hook, how an answer is delivered and recovered, verified keystrokes, risk, policy rules and fingerprints.
sidebar_position: 3
---

An **interaction** is a request of an agent that waits for a person: an approval of a tool call, a
question, a plan review, or a core notice. The core opens it, renders its payload once, shows it on
every surface, and carries the answer back to the harness. Recording a decision and delivering it
are separate steps, and a crash, a lost connection or a person answering in the harness can come
between them, so delivery is a recoverable transaction.
How a person answers is on
[Agents and approvals](/docs/guides/agents-and-approvals); the cards are on [The UI](/docs/desktop/ui).

| Kind | Opened by | A client can decide |
|---|---|---|
| `approval` | `PermissionRequest` of either harness, or an approval dialog on the screen | allow, allow always (optionally saved), deny with a message, deny and stop |
| `question` | `AskUserQuestion`, an MCP elicitation, a question dialog on the screen | chosen labels or free text per question |
| `plan_review` | `ExitPlanMode`, a plan dialog on the screen | approve, or request changes with feedback |
| `notice` | The core, for example the offer to resume agents after a reboot | dismiss only |

Each has a handle such as `i42`, a `risk`, a `mode` (`gate` or `observe`), `answerable` and its
`channels` (`native`, `keystroke`). Open interactions are ordered by risk (`high`, `unknown`,
`medium`, `low`), then by age.

## Opening

When an adapter reports a request ([`apply.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/interaction/apply.rs)):

1. An open interaction of the same run with the same native ref (the `tool_use_id`) is the same
   interaction.
2. A run that is not bound opens it with `answerable: false` and no channels.
3. A policy rule may decide at once (below); otherwise a matching rule only adds a `policy_note`.
4. The core picks the mode: it **holds** the hook (`gate`) only when a shim waits, the run is bound
   and has `gate`, and the setting asks for it. Otherwise the shim gets `no_decision {observe}` at
   once and the harness shows its dialog.
5. The channel is `native` for a held hook, else `keystroke` when the run has `answer_keystroke`.
6. One transaction writes the interaction, its delivery record and `interaction.opened`. The
   `interactions` and `attention` projections change, and a notification goes out when the pane is
   not focused.

## Gate modes and focus

The rule: **the focused pane belongs to the agent**. A person who looks at the pane must see the
harness's own dialog; for anyone else the core holds the hook and shows a card, a notification and
a tray entry.

| `approvals.mode` | Behaviour |
|---|---|
| `auto` (default) | `gate` when the pane is not focused at the moment the request opens, else `observe` |
| `gate` | Always hold when possible. The hold ends at `approvals.gate_timeout` (30 minutes) with `failed {deadline}`, and the harness dialog takes over |
| `observe` | Never hold. Answers from elsewhere go by keystrokes only |

`approvals.claude.mode` and `approvals.codex.mode` override it per harness (`inherit` by default).
The mode is chosen once, when the request opens.

A pane is **focused** when it is the active element of a window that has the OS focus, is visible
and not minimized, and shows the pane. A peek or a card is not focus. A change counts only after it
stands for 500 ms, so a ⌘-Tab through apps releases nothing. The windows of every client count; a
machine with no connected client has no focused pane.

**Release on focus.** When a pane becomes focused, its held interactions without a decision switch
to `observe`: the shim gets `no_decision {focused}` and prints nothing, the interaction gains the
keystroke channel, and its notifications go. Claude Code shows its dialog during a hold anyway;
Codex shows `Running hook` and opens its dialog only on release. With `gate` on a pane that is
already focused, a Codex request stays on `Running hook`: answer from the Inbox, or click away and
back. `auto` avoids this.

## The delivery transaction

| State | Final | Meaning |
|---|---|---|
| `open` | no | Nobody decided |
| `decision_recorded` | no | A decision with a revision, a client, a `via` and an idempotency key |
| `delivering` | no | The decision went to the shim, or the keystrokes started |
| `delivery_unknown` | no | It may or may not have reached the harness |
| `delivered` | yes | Acknowledged, and a later signal (the tool ran) confirmed it |
| `resolved_elsewhere` | yes | The harness went on without our answer, for example the person answered in its dialog |
| `failed`, `dismissed`, `cancelled` | yes | Deadline or no channel; closed without an answer; turn, session or process ended |

The rules ([`model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/interaction/model.rs),
a pure state machine with a property test):

- **The first decision wins.** Another client gets `already_answered`; the same client may replace
  its decision while it is only recorded.
- **Never retransmit blindly.** From `delivery_unknown` the core sends nothing on its own; the end
  of the tool settles it.
- **The ack comes before the output.** The shim calls `adapter.delivery_ack`, then prints the hook
  JSON and exits 0.

The shim and the core speak `adapter.gate` (first reply `{wait, interaction, lease}` or
`{no_decision, reason}`), then `adapter.gate_result` with the output on the same connection. If the
connection drops, the shim reconnects for up to 15 s and sends `adapter.gate_resume` once; a decision
that exists goes to it with the same key. So when the app quits or crashes, a held hook survives a
restart that takes under 15 s, and the open interactions reload from `state.db`. After 15 s the shim
prints nothing and the harness shows its dialog. The shim, the hook timeout (1800 s) and the core
each stop at 30 minutes; only the core reads `approvals.gate_timeout`.

**Fail open.** Every failure of the gate means "print nothing", and the harness asks the person
itself. The fail-closed path for neohar-only denies on yolo runs needs a held `PreToolUse` and
`on_failure`; neither is built.

## Verified keystrokes

When no native channel reaches the harness, neohar answers its dialog by typing into the pane.
It is **best effort**: a
delivery that nothing confirms ends as `delivery_unknown`. Questions, plan approvals and released
requests use it.

- The decision becomes a named choice (`@allow`, `@deny`, an option label, text for `@other`). The
  verifier reads the option labels from the screen manifest, so a new harness version can renumber
  options safely.
- **The dialog must ask for the request of the card.** Before the first key, the verifier compares
  the dialog with the interaction: the same command (line breaks and runs of spaces do not count),
  the name of the file of an edit, or the header of a question. If the dialog asks for something
  else, no key goes in, and the interaction fails with `request_not_on_screen`. An approval with no
  command and no file fails with `request_not_verifiable`.
- While the run has two open interactions of the same kind, no key goes in
  (`several_requests_open`). The dialog on the screen can belong to either of them.
- An approval that the screen opens can be answered only when the rule read its command. The
  verifier reads the last dialog on the screen. A Claude Code dialog does not count while the input
  box shows, so text in the output of a tool that only looks like a dialog opens no approval.
- The screen gives a command only when its frame is sure. In Claude Code, the command is the box
  between two `╌` rows, with no other rule inside. A command with a `---` or `===` row is not
  read. In Codex, the command is the last paragraph before the options, and no other paragraph
  may start with `$ `. A command that is not read cannot be answered with keys.
- After each of these failures, the dialog stays in the harness. Answer it there.
- Each reading must show the same dialog (labels at 0.9 similarity or more). The verifier presses a
  digit accelerator when there is one, else moves one Up or Down at a time and reads the screen
  after each key (300 ms wait, two retries). It never wraps around the list.
- **Enter only when the pointer is on the target.** Any reading that does not fit ends the sequence
  with no more keys (`pointer_lost`, `dialog_changed`, `step_failed`). A sequence has 10 s at most.
- One sequence per pane holds `Pane::lock_keystrokes`. Other clients' input queues for at most 2 s,
  and **a person who types wins**: the sequence ends with `user_typed`.
- After Enter the dialog must go within 2 s, and the next hook or the screen confirms it. A sequence
  is never repeated.

## Risk, policy and fingerprints

**Risk**. `nh-rich` reads a command line
like a shell does, through quotes, wrappers (`env`, `xargs`), `sh -c`, `eval`, substitutions and
here-documents, and scores each command `low`, `unknown`, `medium` or `high`. The line takes the
highest. `high` covers `rm -r`, force pushes, `git reset --hard`, `curl … | sh`, `sudo`, writes
outside the workspace, secret files and hosts outside the allowlist (only this machine is on it, and
no setting fills it yet). `high` also covers git options that run a program (`-c core.fsmonitor=…`,
`--exec-path=…`) and invisible characters such as U+202E. An option that runs a program is `high`,
also for a program that only reads otherwise: `fd -x`, `rg --pre`, `bat --pager`, `find -exec`,
`tar --to-command`, `sed 's/a/b/e'`. A file that an option writes (`sort -o`, `git log --output`,
`curl -o`) has the checks of a write. The check of a path follows the symbolic links of the part of
the path that exists. Read-only commands and the project's own test, lint and
build scripts are `low`; anything unread is `unknown`. Risk orders the inbox, and Enter never
answers a high-risk card. Titles, commands and paths show invisible characters as escapes such as
`<U+202E>`.

**Policy rules**. Rules come from
`[[policy.rule]]` in `config.toml`, from `policy.add`, and from "Allow always" (a session rule that
ends with the core). A rule has `decision`, `harness`, `tool`, `command` (`pnpm test:*` fits the
command with any arguments) or `paths` (globs), `workspace` and `reason`. Any matching deny beats any
allow. An allow fits only a plain line: letters, digits, `_./:=@%+,-`, spaces and quoted strings of
the same characters. A deny also compares each command of the line after it removes the quotes and
the path of the program. A wrapper in the place of the program (`env`, `sudo`, `timeout`, `sh -c`,
`find -exec`) adds the command that it runs. A word in an argument is not a program, so a deny of
`rm:*` does not match `git commit -m "time to rm"`. A rule decides at once only for an approval whose
hook is held, on a bound run with `gate`, in a mode other than `observe`, and an allow never decides
a high risk. The decision has `decided_by: policy` and no notification. Otherwise the rule only
annotates the card. An unknown key makes a rule invalid.

**Fingerprints**. Each answered
approval stores a fingerprint of harness, tool, command prefix or path glob, and workspace.
`policy.suggest` proposes an allow rule for a fingerprint approved at least 5 times in 30 days with no
denial. High risk, lines that are not plain, destructive programs, programs that run other code
(shells, interpreters, wrappers, package runners), an option before the subcommand, secret paths, and
a rule that does not fit the request itself are never eligible. Nothing is applied: the person pastes
the `[[policy.rule]]` block.

**Allow always** needs an eligible fingerprint and the native channel. The interaction shows its rule
in `always_rule`, and the core refuses `always` without it. The session rule decides a later request
only up to the risk of the request that the person allowed. A notification or the tray can allow only
an open request at a low or medium risk whose title shows the whole command. A click there never
replaces a decision.

## Not built

- A held `PreToolUse` or `Elicitation`, so no native answer to questions and no fail-closed deny.
- Rules for questions and plan reviews, a setting for the network allowlist.
- Reconcile through the transcript or a protocol, and the headless lease holders.

## Source map

| Topic | Code |
|---|---|
| State machine, rows | [`interaction/model.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/interaction/model.rs), [`interaction/state.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/interaction/state.rs) |
| Gate, timers, hook JSON | [`interaction/gate.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/interaction/gate.rs), [`interaction/output.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/interaction/output.rs) |
| Focus | [`nh-core/src/focus.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/focus.rs) |
| Keystrokes | [`nh-agents/src/keystroke/verifier.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/keystroke/verifier.rs), [`nh-core/src/keystrokes.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/keystrokes.rs) |
| Risk | [`nh-rich/src/command/rules.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-rich/src/command/rules.rs), [`lex.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-rich/src/command/lex.rs) |
| Policy | [`policy/matcher.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/policy/matcher.rs), [`policy/fingerprint.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/policy/fingerprint.rs), [`policy/suggest.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/policy/suggest.rs) |
| Tests | [`crates/nh/tests/hook_gate.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh/tests/hook_gate.rs), [`tools/nh-golden/tests/delivery.rs`](https://github.com/nikodem-wrona/neohar/blob/main/tools/nh-golden/tests/delivery.rs) |
