---
title: Agents overview
description: How neohar follows Claude Code and Codex in its panes — transports, the five facets of a run, binding, capabilities, the hook shim, discovery, the hook installer and the transcripts.
sidebar_position: 1
---

neohar supervises coding agents that run in its panes. It does not wrap them: the person types
`claude` or `codex` in a pane (or neohar types it for them), and the harness runs its own TUI.
neohar learns what the harness does from several channels, merges them into one state per **run**,
turns approvals and questions into objects that any client can answer, and sends each answer back
through a channel that the harness honours. v1 supports Claude Code and Codex only.

[Claude Code and Codex](/docs/agents/harnesses) covers each harness and the manifests;
[Interactions](/docs/agents/interactions) covers approvals, gating, delivery and policy.

## Transports

| Transport | How it works | Status |
|---|---|---|
| Hooks | The harness runs `nh hook <harness> <event>`. The shim sends the raw payload to the core with the pane token | Built for both harnesses |
| Screen | Screen manifest rules over the VT grid of the pane | Built |
| Process | A walk of the process tree below the foreground group of the PTY | Built |
| Transcript | A tail of the session file of the harness: usage, cost, limits, timeline | Built |
| Headless protocol | `claude -p --output-format stream-json`, `codex app-server`, with neohar as the client | Not built |

## The state of a run

The state is five independent **facets**, not one enum.

| Facet | Values | Main source |
|---|---|---|
| `liveness` | `starting`, `alive`, `exited {code?}` | The holder and the process walk |
| `execution` | `starting`, `working`, `idle`, `error`, `rate_limited {resets_at?}`, `exited`, `unknown` | Hooks, then the screen, then the process |
| `pending` | Open interaction ids | The interactions table |
| `adapter_health` | `healthy`, `degraded`, `disconnected`, `unvalidated_version` | The adapter host and the version probe |
| `read_state` | `seen`, `unseen`, per window | Focus and peek reports |

Liveness, execution and health carry a `Provenance {source, confidence, at}`. The source is
`structured` (a hook), `screen`, `process`, `holder`, `user`, `transcript` or `derived`.
"Needs approval" is derived from `pending`, and "done" is `idle` plus `unseen` for one window.

One pure state machine, the `Arbiter`, is the only writer of the facets. Its rules, in short:

- **Process death overrides at once.** It cancels every open interaction.
- **Open interactions stay open until resolved**: by an answer, the end of the turn, the session
  or the process. The screen closes only what the screen opened.
- **Silence is not staleness.** No timer marks a long tool call as stale.
- **Precedence.** Structured signals beat the screen, and the screen beats the process. A screen
  value that differs from an active hook channel is only a counted disagreement.
- **Unknown versions cap confidence** at 0.8 and set health to `unvalidated_version`.

The badge on the board shows `~` for a state inferred from the screen or the process and `?` for an
unvalidated version.

### Binding

A run is bound to its pane only by deterministic evidence.
Only a `bound` run can be answered, prompted, typed into or decided by policy.

| Binding | How a run gets it |
|---|---|
| `bound` | neohar launched it, a hook carried the pane's `NEOHAR_PANE_TOKEN`, or the person linked it (`agent.link`) |
| `suggested {confidence}` | The process walk found the harness in the pane. Display only |
| `unlinked` | The person ran `agent.unlink`. The next hook with the token binds it again |

The identity of a run is `(pane, harness, session id)`. A new session id from the same process
(`/clear`, `/resume`) keeps the run. A new run starts when the harness process changes.

## Capabilities

What neohar may do with a run is resolved per harness, version, mode and interaction kind, and
published as `run.capabilities`: `observe`, `gate`,
`answer_native` (per kind), `answer_keystroke`, `resume`, `survive_disconnect`; `reconcile` and
`steer` are never granted yet.

A capability needs all of these: the manifest range of the version names it, a row of
[`golden-coverage.toml`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/golden-coverage.toml)
names it with recordings that show it (a test enforces this), and for `gate` and the native
answers the run is bound and has hooks. Today the coverage is:

| Harness / mode | Versions | Recorded | Native answers |
|---|---|---|---|
| claude / tui | `>=2.1.0, <2.2.0` | 2.1.292 | `approval`, `plan_review` (reject only) |
| codex / tui | `>=0.156.0, <0.157.0` | 0.156.1 | `approval` |

Any other version gets `observe` only, plus `answer_keystroke` while its screen manifest still
matches the live screen. The native answer to a question is not granted for either harness. The
Codex manifest withholds `survive_disconnect`, so Codex runs resolve it to `false`.

## The life of a run

1. **Detection.** A watcher per pane walks the tree below the foreground group (depth 6, at most
   256 processes, about 40 µs warm on macOS) and matches it against the manifests. A match starts a
   `suggested` run with `agent.detected {via: "process"}`, and a thread probes the version.
2. **Binding by hook.** A hook with a valid pane token binds the run of its harness in that pane,
   adds `hooks` as a transport and resolves the capabilities again. A pane without a run gets one
   from the hook.
3. **A launch by neohar.** `agent.start`, `agent.resume` and a reboot resume type the
   manifest's command line into the pane's shell, so the shim and the person's `PATH`
   apply. The run that appears adopts the launch: it is `bound`, and Claude Code gets
   `--session-id` in advance.
4. **Adapters.** One supervised task per run reads each hook (`ClaudeHooks`, `CodexHooks`) and
   emits signals. A panic degrades only that run.
5. **The end.** The holder reports the child gone, the walk finds the process gone, another harness
   replaces it, or the pane closes. The row stays in `agent_runs` as history.

### The pane environment

Every pane gets `NEOHAR=1`, `NEOHAR_SOCKET`, `NEOHAR_PANE_ID` (`w3:p5`), `NEOHAR_PANE_ULID`,
`NEOHAR_WORKSPACE_ID`, `NEOHAR_TAB_ID`, `NEOHAR_SESSION`, `NEOHAR_BIN` (`~/.local/bin/nh`) and a
fresh `NEOHAR_PANE_TOKEN`. With
`agents.shims = true` (default), the shims directory is first in `PATH` and in `NEOHAR_SHIMS`, and a
zsh pane gets a `ZDOTDIR` that keeps it first (see [Harnesses](/docs/agents/harnesses)).
Before the login shell runs, the core removes variables that a harness gave the app, such as
`CLAUDECODE`. The token binds; it does not contain: any process of the user can read it.

### The hook shim

`nh hook <harness> <event>` ([`shim.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh/src/shim.rs))
has a 5 ms budget, so it has no async runtime and no config. It reads stdin (at most 4 MiB) and
sends one `adapter.signal`. For `PermissionRequest`, the only held event, it calls `adapter.gate`
and waits for a decision ([Interactions](/docs/agents/interactions)). It always exits 0, and the
only text it prints is hook JSON that the core built, so a crash is never read as a decision. Its
p50 is 1.3 to 1.4 ms through the symlink. The `adapter.*` methods need pane scope (a pane
token) and cannot answer an interaction.

## Discovery and login state

The core of each machine finds each harness on the `PATH` of the login environment, probes its
version and runs the manifest's `[auth] status` (`claude auth status --json`, `codex login status`)
to learn `logged_in`, `logged_out` or `unknown`. It runs at the start, every 30 minutes, on
`harness.refresh` (`nh harness refresh`) and before a start. The projection `harnesses` has one
row per manifest. A harness that is not
installed or not logged in gets a `health` warning, and `agent.start` and `agent.resume` fail
with `harness_unavailable`. neohar never runs a login and stores no credential.

## Installing the hooks

`nh integration install <claude|codex|all>` writes neohar's hooks into the harness settings
([CLI](/docs/reference/cli/commands/integration)).
`status`, `list`, `uninstall` and `capabilities` report and undo it. `nh integration doctor` is not
built (it answers `unsupported`; use `nh doctor`).

| | Claude Code | Codex |
|---|---|---|
| File | `~/.claude/settings.json` (or `CLAUDE_CONFIG_DIR`); project scope `.claude/settings.local.json` | `~/.codex/hooks.json` (or `CODEX_HOME`); project scope `.codex/hooks.json` |
| Events | 17, one group each | 12, one group each |
| Timeouts | 1800 s for `PreToolUse`, `PermissionRequest`, `Elicitation` | 1800 s for the two that wait, 3 s for `SessionEnd` and `Interrupt`, else 10 s |
| Trust | None | The person trusts the hooks in Codex. neohar never writes the trust; `status` reads it |

The command is `<nh> hook <harness> <Event>` with the absolute path of `~/.local/bin/nh`, a symlink
the app maintains, so an update never breaks a hook. neohar finds its own hooks **by that command**:
there is no marker key, because Claude Code 2.1.301 drops unknown keys when it saves the file and
the installer then added a copy at each install. Hooks that still have the old `_neohar` key
have the same command, so they count as neohar's and are rewritten without the key. The installer edits the text, not a reparsed JSON value, so the
user's formatting and hooks stay byte for byte. It refuses a file that is not JSON, makes a
`<file>.neohar-bak-<seconds>` backup before a change, writes atomically, and `--dry-run` prints the
diff. A second install gives the same bytes. Claude Code reads hooks when a session starts, so start
a new session after an install.

## Transcripts and usage

Both harnesses write a JSONL transcript. The path comes from the hooks (`transcript_path`), so a run
without hooks has none. One service polls every run's file once a second, reads only new bytes, and
saves the offset with the usage rows in one transaction, so a restart neither skips nor counts twice.
The parsers are lenient: an
unknown line type gives nothing.

- **Usage.** A ledger counts each model response once (Claude repeats a message's usage on every
  line of it). Rows are per run, UTC day and model. With `billing = "api"` the cost comes from
  [`prices.toml`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/prices.toml),
  and Claude's own `cost-state` total replaces the estimate; `subscription` counts tokens only.
- **Limits.** The `limits` projection has the Codex rate-limit windows (`5h`, `weekly`) and the
  usage of today. The shape of Codex `rate_limits` is still unverified live.
- **Timeline.** `timeline:<run>` exists only while a window follows it, with the latest 500 items;
  `timeline.page` reads older items from the file. The file stays the source.
- A tool result in the transcript resolves an open approval of that call, like `PostToolUse`.

## Source map

| Topic | Code |
|---|---|
| Facets, arbiter, binding | [`nh-agents/src/arbiter.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/arbiter.rs), [`nh-agents/src/run.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/run.rs) |
| Capabilities | [`nh-agents/src/capabilities.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/capabilities.rs), [`golden_coverage.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/tests/golden_coverage.rs) |
| Adapter trait and kit | [`nh-agents/src/adapter/mod.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/adapter/mod.rs), [`kit.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/adapter/kit.rs) |
| Runs, host, watcher | [`nh-core/src/agents/runs.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/agents/runs.rs), [`host.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/agents/host.rs), [`watch.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/agents/watch.rs) |
| Discovery | [`nh-core/src/discovery/mod.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/discovery/mod.rs), [`nh-agents/src/auth.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/auth.rs) |
| Installer | [`nh-agents/src/integration/hooks.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/integration/hooks.rs), [`json.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/integration/json.rs) |
| Transcripts | [`nh-agents/src/transcript/mod.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/transcript/mod.rs), [`nh-core/src/transcripts/service.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/transcripts/service.rs) |
