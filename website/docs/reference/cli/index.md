---
title: Command line (nh)
description: The nh command line - how it finds its core, output and exit codes, how commands map to API methods, and the commands for common jobs.
sidebar_position: 1
---

`nh` is the command line of neohar. It is a client of the API socket of a core and calls the same JSON-RPC methods as the window, with no logic of its own. The [command pages](/docs/reference/cli/commands) have the help text of every verb.

```text
nh [--session NAME] [--socket PATH] [--machine LABEL] [--json] [--timeout SECONDS] <COMMAND>
```

## Which core

The CLI takes the socket from, in order: `--socket`, `--session` (`<runtime>/<NAME>/api.sock`), `$NEOHAR_SOCKET` (every pane has it), `$NEOHAR_SESSION`, else the session `default`.

`--machine studio` runs the command on the machine `studio` of `config.toml` over the ssh connection of its link. A handle with a machine prefix does the same: `nh pane show studio/w3:p5`. See [Remote machines](/docs/guides/remote-machines).

The CLI never sends the pane token, so a command typed in a pane has the scope of the CLI, not of the pane.

In a neohar pane, some commands are refused with `permission_denied`. The core finds the holder of the pane among the parent processes of the client, and it refuses the calls that answer approvals or weaken them:

- `interaction.answer` and `interaction.dismiss`.
- `policy.add` and `policy.remove`.
- `phone.pair_begin` and `phone.publish`.
- `config.set` of a key in `approvals`, `policy`, `phone` or `machines`, and of `agents.shims` and `events.audit_retention`.

Run these commands in a terminal outside neohar, or use the app. Other commands work in a pane, for example `nh config set ui.font_size 14` and `nh integration install`. This check is not a sandbox: [SECURITY.md](https://github.com/nikodem-wrona/neohar/blob/main/SECURITY.md) tells its limits.

## Output and exit codes

On a terminal `nh` prints text for people; piped, it prints the result of the method as one line of JSON. `--json` on a terminal prints indented JSON, which is the way to read a nested result. Errors go to stderr.

| Code | Meaning |
|---|---|
| 0 | OK, or `nh doctor` found only warnings |
| 1 | An error: the core refused the call, neohar is not running, the call timed out, or `nh doctor` found a failure |
| 2 | Usage: bad arguments or bad JSON. Checked before the CLI connects |
| 3 | `not_found` |
| 4 | `already_answered`: someone answered the interaction first |

## Commands follow the contract

Every API method with the `cli` scope has a command `nh <noun> <verb>`, listed in [`crates/nh/src/cli/verbs.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh/src/cli/verbs.rs). The flags come from the schema of the params, so the command line cannot lag behind the API: a boolean is `--name` (or `--no-name` when its default is true), a list is a repeated flag, an object is `--name JSON`, and `_` becomes `-`. References take the ULID or the handle (`w3`, `w3:t2`, `w3:p5`, `i42`).

A few commands are built in: `nh api call`, `nh api schema`, `nh session info`, `nh doctor`, `nh debug bundle`, `nh events tail`, `nh config edit` and `nh completions <shell>`. The roles `nh hold`, `nh hook`, `nh daemon` and `nh bridge` are for the app and the harnesses, not for people.

## Find your way around

```bash
nh session info
nh workspace list
nh pane list --workspace w1
nh agent list
nh pane read w1:p2                          # the visible screen as text
nh search query "panicked at" --pane w1:p2  # the history of the panes
nh fs list .                                # the files and folders of a folder
```

## Type into a pane

```bash
nh pane send-keys w1:p2 "cargo test" Enter  # text, then a key name
nh pane send-keys w1:p2 C-c
nh pane paste w1:p2 "a line of text"
```

`send-keys` joins its words without spaces. Key names: `Enter`, `Tab`, `Escape`, `Space`, `BSpace`, `Up`, `Down`, `Left`, `Right`, `Home`, `End`, `PageUp`, `PageDown`, `C-<letter>`, `M-<char>`.

## Panes and layout

```bash
nh pane split w1:p1 right
nh pane split w1:p1 down --ratio 0.3 --command '{"argv": ["htop"]}'
nh layout equalize w1:t1
nh pane close w1:p3 --force   # without --force: confirm_required while a program runs
```

## Interactions

```bash
nh interaction list
nh interaction next
nh interaction answer i42 allow
nh interaction answer i42 deny --message "Use the test database"
nh interaction answer i42 allow-always
nh interaction answer i43 - --decision '{"kind": "plan_review", "approve": true}'
```

An answer goes through the same delivery transaction as a click on a card. A question or a plan review needs `--decision JSON`; the choice word is then ignored.

## Agents

```bash
nh agent start claude --new-tab w1 --prompt "Run the tests and fix what fails"
nh agent start codex --split w1:p1 --direction down
nh agent wait reviewer idle --timeout-ms 600000 --timeout 610
nh agent prompt reviewer "Now update the changelog"
nh agent interrupt reviewer
```

`agent start` takes exactly one of `--pane`, `--split` and `--new-tab`. `agent wait` waits until the run is `idle`, has `exited`, or needs `attention`; give the core limit (`--timeout-ms`) and the CLI limit (`--timeout`).

## Events, configuration and policy

```bash
cursor=$(nh events tail | jq -c .cursor)   # the end of the log
nh events tail --since "$cursor" --kind 'interaction.*'
nh events tail --follow --kind 'agent.*'
nh config get approvals --json
nh config set approvals.mode gate
nh policy suggest
nh policy add '{"decision": "allow", "tool": "Bash", "command": "cargo test*"}'
```

## Hooks, doctor and bug reports

```bash
nh integration install all
nh integration status
nh doctor                              # works without a running core
nh debug bundle --out ~/Desktop/neohar-bug.zip
```

The debug bundle redacts secrets; read it before you send it.

## Completions

```bash
nh completions zsh > ~/.zfunc/_nh
nh completions fish > ~/.config/fish/completions/nh.fish
```
