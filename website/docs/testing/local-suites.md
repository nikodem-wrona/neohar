---
title: "Local suites"
description: "The suites outside mise run test: the UI shell, renderers and keyboard in browsers, real programs, chaos and restart, the budgets, the live harnesses and a second machine over ssh. What each one proves, the command, and what it needs."
sidebar_position: 2
---

These suites need a browser, a window, a GPU, a release build, an installed harness or a second
machine reachable over ssh. They are not part of `mise run test`, and CI does not run them. The budgets that they measure are on
[Architecture overview](/docs/architecture/overview), and the tools that they use are on
[Tools](/docs/code/tools).

**Before the browser suites.** Every Playwright suite needs `node` on the `PATH` (the Playwright
command line is a Node program) and the browsers of `mise run setup:browsers`. Arguments after `--`
go to `playwright test`. `NEOHAR_CHROMIUM` names a Chromium that is already on the machine.

## The UI shell: `test:shell`, `test:shell:link`, `test:transport`

**Proves** that the DOM follows the projections and that a click or a key sends the right command.
The real UI packages run in Playwright WebKit (the engine of the macOS webview) against a headless
`nh daemon --dev-ws` with real holders. Playwright cannot drive the WKWebView of the app, so the page
uses the dev WebSocket transport instead of the Tauri channel. The test also runs `axe-core` on the
drawer, the palette and the search, and checks that hostile text in an approval card stays inert.

```bash
mise run test:shell                      # tests/ui/ui.pw.ts
mise run test:shell -- -g "palette"      # one test
mise run test:shell:link                 # the same tests through link.sock (NEOHAR_UI_LINK=1)
mise run test:transport                  # the WebSocketTransport alone
```

`test:shell:link` serves the page from a client of `link/1`, so the shell is tested over the path
of a remote machine. Key files: [tests/ui](https://github.com/nikodem-wrona/neohar/tree/main/tests/ui),
[tests/support/daemon.ts](https://github.com/nikodem-wrona/neohar/blob/main/tests/support/daemon.ts).

## The phone app: `test:phone`

**Proves** that the phone app pairs, shows the desk with the form of a phone, and sends what a
person taps. The test server builds the phone app and an `nh`
with it, and starts `nh daemon --dev-ws --phone-port 0`. The listener of the phone serves the page,
and WebKit opens it as an iPhone 17 Pro Max (440 × 956, DPR 3, touch). One test of the narrow width
(tagged `@narrow`) runs as an iPhone 15 only. Small programs in panes send the hooks of Claude Code.

| Case | What the test does |
|---|---|
| Pair | The short code, and the code in the address of the app of the home screen. The paired phone shows the desk, and This phone is in the section Phone of the settings |
| Question | The Inbox of the bottom bar shows the card, and the delivery record has `via: phone` |
| Push | The address `#/run/<id>` of a push opens the Inbox with the card of the run |
| Turn ended | A reply from the line of the run in the Inbox goes to the program once, and the Timeline shows the last message in full |
| Panes | One pane in portrait with a dot for each pane of the tab, and both panes and the rails in landscape |
| Sheets | The drawer, the left column, More and the palette |
| Banner | The core stops and starts again, and the app connects again |
| Unpair | "Unpair this phone" shows the view Pair |

Each view has no side scroll and no serious `axe` violation.

```bash
mise run test:phone                      # tests/phone/phone.pw.ts
NEOHAR_PHONE_SHOTS=/tmp/shots mise run test:phone   # also pictures of the Inbox, portrait and landscape
```

`mise run test:host` checks the one thing that only the real app can show: its webview can call
`rpc`, `subscribe` and `stream` and nothing else. It opens
one window of a debug app with the feature `selftest`
([selftest.rs](https://github.com/nikodem-wrona/neohar/blob/main/apps/desktop/src-tauri/src/selftest.rs)).

## Renderers: `test:gpu`

**Proves** that the WebGL2 and Canvas2D renderers paint the same cells, that a lost WebGL context
recovers, that a frame allocates nothing after warm-up (Chromium only), and the render budgets
(`perf.pw.ts`, with `NEOHAR_GPU=1`). It runs in WebKit, Chromium on the GPU and Chromium without a
GPU, against frame files from `nh-bench gen-frames`.

```bash
mise run test:gpu
mise run test:gpu -- --project=webkit perf.pw.ts
```

Not proven: the GPU time of a draw (no GPU timer in WebKit), 120 Hz (the display runs at 60 Hz), and
Linux.

:::warning[A stale server]
Only this suite sets `reuseExistingServer: true`. A server from an earlier run on port 4173 serves
the old page and old frames. Stop it, or set `NEOHAR_RENDER_PORT`, before a run that must test new
code.
:::

## Keyboard: `test:keyboard`

**Proves** that browser key events reach a program as the right bytes. The matrix in
[tests/keyboard/cases](https://github.com/nikodem-wrona/neohar/tree/main/tests/keyboard/cases) has
969 cases, and the expected bytes come from documents (xterm `ctlseqs`, terminfo, the kitty
protocol), never from the encoder. The browser run sends synthetic DOM events through the keyboard
layer of `packages/ui`, then asks `nh-keytest` to encode them with the real encoder of `nh-term`.
The Rust half of the matrix runs in `mise run test`.

```bash
mise run test:keyboard
```

Not done yet: real captures of keyboard layouts in the app's webview.

## Real programs: `test:apps`

**Proves** that `less`, `vim` and `htop` render in panes of `nh-core` and take typing
([crates/nh/tests/core_apps.rs](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh/tests/core_apps.rs)).
A program that is not installed fails its test.

## Restart: `nh-chaos` and the corpus check

**Proves** that a killed core gets every screen back exactly and applies no input twice.

- `nh-chaos run --cycles 100` kills a headless host with `SIGKILL` 100 times under 21 live panes
  (`vim`, `less`, `htop`, the `nh-fixture` programs and a shell loop). After each restart it compares two
  screens of every pane with a reference engine that never restarted: the screen at the end of the
  replay and the screen after the resize nudge. The first one is what shows that snapshot and replay
  are exact; programs that repaint hide a bad replay after the nudge. About 6 minutes.
- `nh-snapcheck corpus --every 1` (in release) takes a snapshot and a restore at every cut point of
  the VT corpus and compares three engines. About 30 minutes. Run it after every update of
  `alacritty_terminal`.
- After a change of the corpus generator of `nh-fixture`, run its wide sweep (200 seeds of 1 MiB,
  about 30 s in a release build):
  `NEOHAR_SWEEP=1 bash scripts/with-test-env.sh cargo nextest run --release -p nh-fixture --test vt_corpus`.
  Without `NEOHAR_SWEEP=1` the test skips and says so.

`mise run build:chaos` builds `nh-chaos` and what it starts, under `target/release`.
`nh-chaos run --seed 20261008` repeats a run, and `nh-chaos clean` kills the holders of a run that
did not clean up. The restart tests of `crates/nh/tests/core_restart.rs` and a sample of the corpus
check run in `mise run test`.

## Budgets

| Command | Measures |
|---|---|
| `mise run bench` | `cargo bench -p nh-term`: frame build of a 200×60 redraw, snapshot and restore. Fails on a miss |

The other budget runs have no task. Run them by hand in release: `scripts/bench-shim.sh` for the
hook shim, and the `walk_bench` and `screen_bench` examples of `nh-agents`.

Each run writes JSON under `target/` (for example `target/bench-shim/running.json`). A commit that
changes a budgeted path includes the numbers in its message.

**macOS pitfalls.** Use the footprint (`footprint -p`), not the resident size. Read CPU time from the
kernel, not from `ps`.

## Live harnesses: `test:harness`, `record:golden`

**Proves** that the installed Claude Code and Codex still behave as the committed recordings in
[tests/harness-golden](https://github.com/nikodem-wrona/neohar/tree/main/tests/harness-golden) say,
and runs the delivery cases of an `answer_native` grant against them.
The harness runs offline: a
real binary in a `sandbox-exec` profile, a scratch home, and the mock model on the loopback. The
real `~/.claude` and `~/.codex` stay as they were. It needs macOS and both harnesses on the `PATH`.

```bash
mise run test:harness    # record each scenario and compare with the committed recording
mise run record:golden   # write new recordings into tests/harness-golden/<harness>/<version>
```

After a new harness version: record, read the new screens in the diff, bless `golden_replay`, and
add the version to `crates/nh-agents/golden-coverage.toml` only when the cases behind each grant
exist. The tests that read the recordings (`golden_coverage`, `golden_replay`, `screens_replay`)
run in `mise run test`.

## A second machine: `test:remote`

**Proves** the remote path over real ssh: the bridge starts a daemon on the remote machine, `Welcome`
comes, calls work, and the daemon stops. It runs every
`crates/nh/tests/remote_*.rs` file.

```bash
NEOHAR_REMOTE_TARGET=<full ssh host name> mise run remote:deploy   # copy nh and nh-dev to ~/.neohar-dev/bin
NEOHAR_REMOTE_TARGET=<full ssh host name> mise run test:remote
```

`remote:deploy` copies the `nh` of this checkout, so the remote machine needs the same system and
CPU type. There everything lives under `~/.neohar-dev/`. Without `NEOHAR_REMOTE=1` these tests
skip, so `mise run test` needs no network; `test:remote` sets it and fails without a target.
The runs of link chaos, a killed daemon, sleep and a network change are not done yet.

## Pitfalls

- **Do not pipe a long run into `head`.** The closed pipe ends the daemon before it stops its
  programs, and the holders stay. macOS has 511 pseudo-terminals; when panes fail to start, look
  for leftover holders with `pgrep -fl 'nh hold'`.
- **Load.** The wheel and selection test of the shell and the late-frame checks of `test:gpu` can
  fail on a busy machine. Run them again on a quiet one before you look for a fault.
