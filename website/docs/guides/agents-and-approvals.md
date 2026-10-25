---
title: Agents and approvals
description: "Run Claude Code or Codex in a pane, read the state that neohar shows, control agents, answer approvals from cards, notifications or the CLI, and write policy rules."
sidebar_position: 3
---

Run Claude Code or Codex in a pane as in any terminal. neohar changes nothing in how the harness
looks or works. It shows the state of each agent, holds approvals when you look elsewhere, and lets
you answer them from anywhere. Install the hooks first ([Install the app](/docs/guides/install)).
How it works inside: [Agents](/docs/agents/overview) and [Interactions](/docs/agents/interactions).

## Run an agent

Type `claude` or `codex` in a pane. Within a second neohar finds the harness in the process tree of
the pane. The run is **suggested** until the first hook arrives with the token of the pane. Then it
is **bound**, and neohar knows the exact state and can answer for it. Claude Code sends a hook at
once. Codex sends its first hook with your first prompt.

In a pane, `codex` runs a small shim of neohar that adds `--disable daemon_auto_start --no-daemon`.
This keeps Codex in its own process, so its hooks reach neohar also when the shared Codex daemon
runs. Your arguments pass unchanged.
`CODEX_NEOHAR_SHIM=0` or `agents.shims = false` turns the shim off.

## What the window shows

The board under the top row has a strip for each agent. A strip has a badge, the state in four
letters and the work (the title of its pane), and under them the
harness and the cost. Its left edge is green for `WORK`, amber for `WAIT` and red for `ERR`.

| Badge | Code | Meaning |
|---|---|---|
| `◌` | `INIT` | Starting |
| `●` | `WORK` | Working |
| `○` | `IDLE` / `DONE` | Idle. `DONE`: it finished and this window has not looked at it yet |
| any | `WAIT` | It waits for you: an approval, a question or a plan |
| `▲` | `ERR` | Error |
| `◷` | `RATE` | Rate limited |
| `×` | `END` | Ended |

A `~` before the badge means the state is a guess from the screen or the process list: install the
hooks. A dim `?` after the name of the harness in the strip of a pane means neohar has not
validated this version of the harness, so it only observes. The **Timeline** tab of the drawer shows the turns, tool calls, tokens and cost of the
agent in the focused pane.

## Control agents

The palette has "Start Claude Code beside this pane" and "Start Codex beside this pane", and
commands that interrupt, stop, resume or link the agent of the focused pane. The CLI has the same
and more:

```bash
nh agent start claude --split w3:p5 --prompt "Fix the failing test" --name fix-tests
nh agent start claude --new-tab w3 --model opus --effort high
nh agent choices                        # the models and effort levels a start can use
nh agent prompt fix-tests "Now run the whole suite"
nh agent reply fix-tests "Now run the whole suite"   # only after its turn ended; never twice
nh agent interrupt fix-tests            # Esc
nh agent stop fix-tests                 # Ctrl+C twice, then SIGTERM
nh agent wait fix-tests idle --timeout-ms 600000 --timeout 610
nh agent resume fix-tests               # claude --resume / codex resume
nh agent link w3:p5                     # bind a suggested run that you know is there
```

A key from neohar goes only to a bound run. If you type in the pane at the same moment, the call
fails with `busy`: you win.

## How a request reaches you

**If you look at the pane, the agent shows its own dialog. If you do not, neohar holds the request
for you.** Looking means that the window has the focus and the pane has the keyboard.

- A held request gives a card in the Inbox, a count in the attention line and the Dock, a tray
  entry and a notification.
- Click into the pane and neohar lets the request go, so the dialog of the harness is what you
  answer. Claude Code draws its dialog also while neohar holds the request. Codex shows "Running
  hook" until neohar answers or lets go.
- A request that waits 30 minutes (`approvals.gate_timeout`) goes back to the dialog in the pane.
- An agent that ends its turn while you do not look at its pane waits too: a line in the Inbox, a
  count, and a notification (`agents.turn_end_notice`). A look at the pane or a key ends the wait.

`approvals.mode` is `auto` (the rule above), `gate` (always hold) or `observe` (never hold).

## Answer from a card

`⌘⇧I` shows the Inbox. `⌘⇧A` shows the most urgent card that you can answer.

| Card | Answers |
|---|---|
| Approval | **Allow**, **Allow always** (a session rule too), **Deny** (type a reason first if you want), **Deny and stop**. Keys `1` to `4` pick them |
| Question | Choose the options, or type your own answer, then **Answer** |
| Plan | **Approve**, or **Request changes** with feedback |

The card shows the command or the diff, its risk and the last lines of the pane. At a **high**
risk, Deny starts selected, a number key only selects, and Enter on the button answers. "Display
only" means the agent is not linked to its pane: answer in the pane, or run `nh agent link`.
Invisible characters, such as the direction control U+202E, show as escapes like `<U+202E>`,
and they make the risk high.

Under the card, neohar says what became of your answer: delivered, answered in the pane, or
"Couldn't confirm your answer reached …". In that last case, look at the pane: neohar never sends
an answer twice. The first answer wins, from any card, notification or client.

From the command line:

```bash
nh interaction list
nh interaction answer i42 allow          # allow, allow-always, deny
nh interaction answer i42 deny --message "use the test database"
```

## Policy rules

A rule answers an approval before it reaches you. Add one with `nh config edit`:

```toml
[[policy.rule]]
id = "pnpm-test"
decision = "allow"        # or "deny"
tool = "Bash"             # or "Edit", "Write", "*", an MCP tool
command = "pnpm test:*"   # or paths = ["src/**"] for file tools
reason = "tests are safe"
```

- `pnpm test:*` matches `pnpm test` and `pnpm test:unit`, not `pnpm testing`. A `*` is allowed only
  at the end.
- An allow matches only a plain line: letters, digits, `_./:=@%+,-`, spaces, and quoted strings
  of the same characters. Any other character (`;`, `|`, `&`, `>`, `$`, `(`, `*`, `~`, a new line,
  a character that is not ASCII) sends the request to you.
- An allow never decides a request of high risk. The card shows the rule in a note.
- A deny matches if any part matches. It also compares each command after it removes the quotes,
  the path of the program and wrappers such as `env`, `sudo` and `timeout`.
- A deny always wins over an allow.
- A rule decides only for a bound run of a validated version that neohar holds. A rule is not a
  sandbox: a harness that asks for nothing (`--dangerously-skip-permissions`, `-a never`) sends
  nothing to deny.

```bash
nh policy list        # the rules of config.toml, then the added and session rules
nh policy suggest     # rules for approvals that you gave 5 times and never denied
nh policy add '{"id":"cargo-check","decision":"allow","tool":"Bash","command":"cargo check:*"}'
nh policy remove cargo-check
```
