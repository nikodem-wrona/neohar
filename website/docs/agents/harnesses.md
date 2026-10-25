---
title: Claude Code and Codex
description: What is specific to each harness — hooks, native answers, the Codex trust and its shared daemon, the PATH shim — and the harness and screen manifests that describe them.
sidebar_position: 2
---

Both harnesses run in their own TUI in a pane, and neohar reads them through the same transports
([Agents overview](/docs/agents/overview)). This page lists what differs, then the two kinds of
manifest. Everything here was recorded against Claude Code 2.1.292 and Codex 0.156.1, the versions
of the golden corpus; how to record a new version is on [Local suites](/docs/testing/local-suites).

## Claude Code

`ClaudeHooks` maps the 17 hooks of Claude Code. It decides nothing: answers come from the core.

| Hooks | What the adapter reports |
|---|---|
| `SessionStart`, `SessionEnd` | Session id, transcript path, model; `idle` for `startup`, `resume`, `clear`. `SessionEnd` with `reason = clear` keeps the run |
| `UserPromptSubmit`, `PreToolUse` | `working`. `PreToolUse` of `AskUserQuestion` opens a `question` |
| `PermissionRequest` | Opens an `approval`, or a `plan_review` for `ExitPlanMode`. **The only hook the shim holds** |
| `PostToolUse`, `PostToolUseFailure`, `PermissionDenied` | End the tool call and resolve its interaction |
| `Stop`, `StopFailure`, `Notification` | `idle`, `error` or `rate_limited` (a rate-limit kind, `quota_auto_resume_*`) |
| `SubagentStart/Stop`, `PreCompact/PostCompact`, `CwdChanged`, `Elicitation` | Subagents, compaction, nothing yet, an MCP `question` |

A `PermissionRequest` has no `tool_use_id`, so the adapter takes the id of the latest earlier
`PreToolUse` with the same tool and input. A hook's `permission_mode` of `bypassPermissions` or
`dontAsk`, or `--dangerously-skip-permissions` in the process tree, marks the run `yolo`; then
`PermissionRequest` never fires.

**Native answers**. The core builds the hook JSON in
[`output.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/interaction/output.rs):

- Allow: `decision.behavior = "allow"`. Allow always for a `Bash` command adds
  `updatedPermissions` with `addRules` and `{toolName, ruleContent}`, with the rule that the card
  shows. With wrong field names Claude Code silently ignores the whole reply. A rule always has
  `ruleContent`: a rule without it allows every call of the tool. For the other tools, Allow
  always is a plain allow plus a neohar session rule. A path rule of Claude Code reads its pattern
  from another directory and follows other links, so it is not the rule that the card shows.
  The destination is `session`, and `localSettings` only when the client asks to save **and**
  `approvals.claude.persist_always = true`.
- Deny: `behavior = "deny"` with a message. Deny and stop adds `interrupt: true`; no `Stop` hook
  follows it.
- Claude Code shows its own dialog **while the hook is held**. An answer there ends the interaction
  as `resolved_elsewhere`.
- **Plans**: reject (with feedback) is native; approve is not (Claude ignores an allow), so it goes
  by keystrokes. **Questions**: the `updatedInput` answer works, but it needs a held `PreToolUse`,
  which the shim does not hold, so questions are answered in the dialog or by keystrokes.
  **Elicitations**: no output format yet.

**Identity and resume.** `agent.start` passes `--session-id <uuid>`, so the run is identified before
its first hook. The resume handle is `claude --resume <id>`.

## Codex

`CodexHooks` maps the 12 hooks of Codex: the same lifecycle as
Claude Code plus `Interrupt`, with no `StopFailure`, `Notification` or `Elicitation`. Codex has no
hook for questions or plans, so it opens approvals only. Its hooks carry `tool_name: "Bash"`, and
`SessionStart` fires with the **first prompt**, not when the TUI starts.

- **Trust.** Codex runs a hook only after the person trusts its exact definition ("Trust all and
  continue" at start, or `/hooks`). neohar never writes the trust. `integration.status` reads
  `config.toml` and reports `hooks_trusted`. The installer keeps the definitions byte-stable, with
  no marker key, so the trust survives updates.
- **Answers.** Allow and deny as for Claude Code. Allow always is a plain allow plus a neohar
  session rule, because the Codex reply has no rules.
- **The held hook hides the dialog.** While a hook is held Codex shows `Running hook`; the dialog
  appears after the hook returns. This is why a focused pane releases the hold
  ([Interactions](/docs/agents/interactions)).
- **Yolo.** `--dangerously-bypass-approvals-and-sandbox`, `-a never`, `--ask-for-approval never`,
  `-s danger-full-access` or `--sandbox danger-full-access`. Under `-a never` every hook fires
  except `PermissionRequest`.
- **Resume.** Codex cannot pre-assign a session id, so a launched run is identified at its first
  prompt. The resume handle is `codex resume <id>`.

### The shared daemon and the shim

Codex can attach to a per-user app-server daemon. Its hooks then run in the **daemon's**
environment, with the token of whichever pane started the daemon, maybe one closed days ago.
Since Codex 0.162 the
daemon is on by default. neohar keeps every Codex of a pane on its own embedded server:

1. The core writes a `sh` shim to `<data>/shims/codex` at each start
   ([`codex.sh`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/shims/codex.sh)),
   and `agents.shims = true` puts its directory first in each pane's `PATH`.
2. The shim runs the next real `codex` after its own place with
   `--disable daemon_auto_start --no-daemon` and the person's arguments unchanged. The first flag
   only stops a **new** daemon: Codex 0.163 still attaches to one that runs, so `--no-daemon` is
   needed too. A Codex that does not know `--no-daemon` (checked with
   `codex --no-daemon --version`) and the daemon commands (`agents`, `queue`, `remote-control`,
   `app-server`, `--remote`) get the first flag only. `CODEX_NEOHAR_SHIM=0` or an explicit flag
   passes everything unchanged.
3. macOS `path_helper` and `brew shellenv` push the shims back in a login zsh. So a zsh pane starts
   with `ZDOTDIR=<data>/shims/.zdotdir`, whose `.zshenv` reads the person's own files and puts
   `$NEOHAR_SHIMS` first again before each prompt and command. For bash or fish, put
   `$NEOHAR_SHIMS` first yourself.
4. A Codex run whose process tree has no `--no-daemon` is `shared_daemon`. A Codex run that ends a
   turn with no hook gets a doctor note; for a `shared_daemon` run it says to start `codex` again
   in a pane. `nh doctor` also warns when an alias or a function hides the shim.

Claude Code needs no shim: its hooks always carry the pane. The display-only observer of a Codex on
the shared daemon is not built. Headless Codex (`codex app-server` owned by neohar) is not built.

## Harness manifests

A harness manifest is a TOML file that says how to find, launch, resume, version and probe a harness,
and which capabilities each version range has. The two built-ins are compiled in from
[`crates/nh-agents/manifests/`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-agents/manifests).

| Section | What it holds |
|---|---|
| `[detect]` | `priority` and `[[detect.process]]` rules: `exe_basename`, `interpreter` with `script_regex`, `exe_path_regex`, `argv_regex`, `argv0_basename`, `env` |
| `[version]` | The command and a regex with a group `v`. The probe is cached by path and modification time |
| `[auth]` | The login probe, the rules `logged_in` and `logged_out`, the method and a `login_hint` |
| `[launch]` | `argv`, `argv_with_session`, `model_arg`, `effort_arg`, the `models` and `efforts` that `agent.choices` offers, and the `ready` screen rule. Codex reads its models from `model_catalog = "codex"` |
| `[resume]` | `argv` with `{resume_id}`, `continue_argv`, `needs_cwd` |
| `[[capabilities]]` | A `semver` range, a mode and the capabilities it grants |
| `[yolo]`, `[screen]`, `[ui]` | Yolo flags and sequences, the screen manifest id, interrupt and quit keys |

Files in `~/.config/neohar/harnesses/*.toml` override a built-in with the same `id` by a deep merge
(tables merge, arrays replace), or describe a wrapper with `extends`:

```toml
schema = 1
id = "cc"
extends = "claude"
[detect]
priority = 200
[[detect.process]]
argv_regex = '(^|/)cc( |$)'
[launch]
argv = ["cc"]
```

A file that fails a check is left out and the manifest below it stays in force. The core reloads the
directory on change and appends `harness.manifest_loaded`. When several manifests match a process
tree, the higher priority wins, then a rule that names the program over an `env` rule, then the
process nearer the leader. A signed update channel for manifests is not built.

## Screen manifests

A screen manifest is data: rules over the text and style of some rows of the screen, run by a pure
detector ([`engine.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/screen/engine.rs)).
A rule reports a state (`working` or `idle`) or opens an interaction kind. It matches with
`any`, `all` and `not` regexes, a `style`, the cursor or the window title, and `hold_ms` debounces a
spinner. A rule that opens a dialog can describe its geometry for keystrokes: the option regex, the
pointer glyph, digit accelerators, and `labels` that name options (`allow`, `allow_always`, `deny`,
`other`) by text, because option numbers change between versions.

| Built-in | Rules |
|---|---|
| [`claude.toml`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/screens/claude.toml) | `working`, `input_box` (idle, and the `ready` signal of a launch), `approval`, `question`, `plan` |
| [`codex.toml`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/screens/codex.toml) | `working`, `working_title` (the spinner in the window title), `input_box`, `approval` |

A watcher per pane evaluates at most 10 times a second on damage, and copies only the rows the rules
read. One evaluation of a 200 by 60 screen takes about 70 µs against a 0.3 ms budget
(the `screen_bench` example of `nh-agents`). A screen match **adds** information and never overrides a hook: a state
is a low-precedence observation, and a dialog opens a **provisional** interaction only when no
interaction of that kind is open. A hook interaction of the same kind supersedes it. A framed list of
numbered options that no rule reads becomes a question with the hint "neohar can't read this
dialog", and keys never answer it.

## Source map

| Topic | Code |
|---|---|
| Hook adapters | [`claude/hooks.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/claude/hooks.rs), [`codex/hooks.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/codex/hooks.rs) |
| Installers and trust | [`integration/claude.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/integration/claude.rs), [`integration/codex.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/integration/codex.rs) |
| Shims | [`nh-core/src/shims.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/shims.rs) |
| Manifests | [`manifest/schema.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/manifest/schema.rs), [`manifest/loader.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/manifest/loader.rs), [`matcher.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/matcher.rs) |
| Screen detection | [`screen/dsl.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/screen/dsl.rs), [`nh-core/src/agents/screen.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/agents/screen.rs) |
| Recordings | [`tests/harness-golden`](https://github.com/nikodem-wrona/neohar/tree/main/tests/harness-golden) |
