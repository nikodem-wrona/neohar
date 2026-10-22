---
title: "Testing overview"
description: "What mise run check and mise run test run, where each kind of test lives, the rules that every test follows, and the local suites."
sidebar_position: 1
---

Two `mise` tasks gate every change. Run them before each pull request. The CI workflow
(`.github/workflows/ci.yml`) runs them on a macOS runner for each pull request and each push to
`main`:

- `mise run check`: every static check.
- `mise run test`: every test that needs no live harness, no GPU, no browser and no second machine.

The other suites are local suites. They need a browser, a window, a release build, an installed
harness or a second machine reachable over ssh. Run the ones that cover the changed code and put
their numbers in the commit message. They are on [Local suites](/docs/testing/local-suites).

## Every suite

| Suite | Command | Where it runs |
|---|---|---|
| Static checks | `mise run check` | Any machine, every session |
| Rust, UI and install tests | `mise run test` | Any machine, every session |
| The UI shell against a headless core | `mise run test:shell`, `test:shell:link`, `test:transport` | A Mac with WebKit and `node` |
| The phone app against the listener of the phone | `mise run test:phone` | A Mac with WebKit and `node` |
| The sandbox of the real webview | `mise run test:host` | A Mac with a display (opens a window) |
| Renderers in browsers | `mise run test:gpu` | A Mac with a GPU, WebKit and Chromium |
| Keyboard matrix in browsers | `mise run test:keyboard` | A Mac with WebKit |
| `less`, `vim`, `htop` in panes | `mise run test:apps` | A Mac with the programs installed |
| Restart under `SIGKILL` | `nh-chaos run` (by hand) | A Mac, release build |
| Snapshot at every cut point | `nh-snapcheck corpus --every 1` (by hand) | A Mac, release build, about 30 min |
| Budgets | `mise run bench`; `scripts/bench-shim.sh` and the bench examples by hand | A Mac, release build |
| Live Claude Code and Codex | `mise run test:harness`, `record:golden` | A Mac with both harnesses installed |
| A second machine over ssh | `mise run test:remote` | A Mac and a second machine reachable over ssh (`NEOHAR_REMOTE_TARGET`) |

The generated list of every task is on [mise tasks](/docs/getting-started/mise-tasks).

## `mise run check`

| Check | What it guards |
|---|---|
| `check:commits` | Each commit that `@{upstream}` does not have is a Conventional Commit with a type and a scope of `scripts/commits.ts` |
| `check:version` | Each version file holds the version of `Cargo.toml`, `CHANGELOG.md` has its section, and it is the version of the last tag `v*` |
| `check:fmt` | `cargo fmt` and Prettier. `mise run fmt` fixes every failure |
| `check:clippy` | Clippy with `-D warnings` on the workspace, and again with the features that the workspace build leaves off (`dev-ws`, `selftest`). No `unwrap()` outside tests; `unsafe` is denied unless a module allows it with a `// SAFETY:` comment |
| `check:deny` | `cargo deny`: licenses, advisories, bans, crates.io only. It needs the network |
| `check:eslint` | The thin-presentation rule of the webview: no `innerHTML`, no polling, no timers outside `timing.ts`, no sorting or locale formatting outside `surface/`. See [Desktop app](/docs/desktop/overview) |
| `check:tsc` | Strict TypeScript in every package and script. It builds the WASM decoder first |
| `check:deps` | The crate dependency rules and the `wasm-bindgen` pin |
| `check:gen` | `packages/transport/src/generated.ts` equals what `nh api schema` makes. Fix with `mise run gen:api` |
| `check:notices` | `THIRD_PARTY_LICENSES.md` equals what `mise run gen:notices` writes. It runs cargo-about offline, in about 25 s |
| `check:docs` | Each generated page of `website/docs` equals what `bun run gen` in `website/` writes, and the page checker (`bun run check`) finds no fault. It writes nothing and needs no network. It checks the CLI pages only with an `nh` binary that is newer than `crates/nh/src` |
| `check:holder-size` | `crates/nh-hold/src` stays under 2,500 lines of code |
| `check:bundle` | The gzipped scripts, styles and WASM of the webview and of the phone app stay under 300 KB each, and the first chunk of the phone app (what loads before the first paint) under 150 KB |

**Crate dependency rules.** [scripts/check-deps.ts](https://github.com/nikodem-wrona/neohar/blob/main/scripts/check-deps.ts)
gives each crate a list of workspace crates that it may use and crates that must not be in its tree.
The rules keep the layers apart: `nh-hold` has no `tokio`, `tracing` or `serde_json`, so it stays
small and starts fast; `nh-proto` has no `tokio`, so it compiles to WASM; no crate below `nh-core`
reaches up into it; only the app links Tauri. A new crate fails the check until it has a row.
Dev-dependencies do not count.

**Holder size.** Holders outlive the app and its updates, so a new core must talk to old holders,
and every change of the holder is expensive.
[scripts/holder-size.ts](https://github.com/nikodem-wrona/neohar/blob/main/scripts/holder-size.ts)
counts lines of code without comments and inline test modules. Keep the tests of `nh-hold` inline:
the script refuses a test module in another file.

## `mise run test`

`mise run test` runs three tasks:

| Task | Command | Scope |
|---|---|---|
| `test:rust` | `cargo nextest run --workspace --locked` | Every Rust test of every crate, tool and the app |
| `test:ui` | `bun test` (builds the WASM decoder first) | Every `*.test.ts`, `*.test.tsx` and `*.test.js` file. Playwright specs end in `.pw.ts`, so `bun test` skips them |
| `test:install` | `scripts/app-install.test.sh` | The install script of the local app, on folders under `/tmp` |

Each runs through [scripts/with-test-env.sh](https://github.com/nikodem-wrona/neohar/blob/main/scripts/with-test-env.sh).

| Kind of test | Where it lives |
|---|---|
| Unit tests | `#[cfg(test)]` modules and `tests.rs` files next to the code |
| Property tests (`proptest`) | Cut points and the journal ring of `nh-hold`, cell packing and frames of `nh-proto`, frame exactness and the encoder of `nh-term`, Markdown safety of `nh-rich`, the WASM decoder, the layout tree and the delivery model of `nh-core` |
| Snapshot tests (`insta`) | The API contract in `crates/nh-proto/tests/snapshots`, the cards in `crates/nh-rich/tests/snapshots` |
| Frozen wire fixtures | `crates/nh-proto/tests/fixtures`: one encoded value per file. A test never changes one; a change of the wire needs a new protocol version |
| Integration with real holders | `crates/nh/tests`: holders, the core, the API socket, the CLI, the hook shim, restart, tasks, files, the link and machines, against real `nh hold` processes |
| Corpus checks | A sample of the snapshot check (`tools/nh-snapcheck/tests`), the keyboard matrix in Rust (`tools/nh-keytest/tests`), the replay of the harness recordings (`crates/nh-agents/tests`) |
| Store crash test | `crates/nh-store/tests/crash.rs`: a writer killed 50 times leaves a sound database |
| UI and tooling tests | `packages/*/src/**/*.test.ts`, `scripts/*.test.ts`, `tools/*/src/*.test.ts` |

Run one part with the same environment:

```bash
bash scripts/with-test-env.sh cargo nextest run -p nh --test core_restart
mise exec -- cargo nextest run -p nh-term   # the pinned toolchain of mise.toml
```

A changed `insta` snapshot is written next to the old one as `.snap.new`. Read the diff, then accept
it: rename the file, or run the test again with `INSTA_UPDATE=always` (`cargo insta` is not
installed). For the contract snapshot of the API:

```bash
INSTA_UPDATE=always bash scripts/with-test-env.sh cargo nextest run -p nh-proto --features api
```

The schema tests of `nh-proto` run only with `--features api`. In the same commit, run
`mise run gen:api` and `bun run gen` in `website/`.

### Bless variables

Some tests compare with files that a person reviewed. A variable writes the files again. Read the
diff before the commit.

| Variable | Writes |
|---|---|
| `NEOHAR_BLESS=1 cargo test -p nh-agents --test golden_replay` | `expected.jsonl` of each recording |
| `NEOHAR_BLESS_FIXTURES=1` | Missing frozen wire fixtures, never an existing one |

## Rules for every test

- **Temporary directories**. Every test and every dev run
  sets `NEOHAR_CONFIG_DIR`, `NEOHAR_DATA_DIR` and `NEOHAR_RUNTIME_DIR` to temporary directories.
  `with-test-env.sh` also sets `CLAUDE_CONFIG_DIR` and `CODEX_HOME`, and the headless core of the
  Playwright suites gets a temporary `HOME` too.
  Nothing writes to the real configuration, data, sockets, `~/.claude`, `~/.codex` or
  `~/.local/bin/nh`. A debug build of the app refuses to start without the three variables.
  The tests do not change `PATH`. Thus the core of a test can run the installed `claude` or `codex`
  to read its version or login state.
- **Holder guards.** Holders outlive their parent by design. Every test that starts holders uses a
  guard that kills them on drop: `TestHolder` in
  [crates/nh/tests/support/mod.rs](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh/tests/support/mod.rs)
  and `Harness` in
  [crates/nh/tests/core_support/mod.rs](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh/tests/core_support/mod.rs).
- **Loopback only.** No test talks to a network service except on 127.0.0.1. The mock model server
  stands in for the model APIs. The tests over real ssh are the one exception, and only
  `mise run test:remote` turns them on.
- **No fixed sleeps.** A test waits on events with a deadline. Timer logic runs on paused time. A
  flaky test has a race; find it.
- **Gates, not skips.** A test that needs something rare returns early and says why, unless its
  variable is set: `NEOHAR_APPS`, `NEOHAR_HARNESS`, `NEOHAR_GPU`, `NEOHAR_SWEEP`, `NEOHAR_REMOTE`. No
  test uses `#[ignore]` or `.skip` to make a check pass.
- **Truthful results.** A budget is met only when it is measured on the target hardware and the
  numbers are in the commit message. A behavior of a harness is verified only by a recording or a
  test against a live binary. A check must be able to fail.

## Slow and load-sensitive tests

- The snapshot-check sample (`tools/nh-snapcheck/tests/corpus.rs`) takes about 40 s. It is the
  largest cost of `test:rust`.
- The pacer tests of `crates/nh/tests/core_render.rs` and the config watcher tests of `nh-core`
  measure wall time. A busy machine can fail them. Run them alone before you look for a fault.
- The first launch of a new test binary on macOS waits for the system's code checks, 10 to 25 s. It
  is not a hang.
- On macOS, `target/debug/deps` collects old object files, and nothing removes them. With hundreds
  of thousands of files there, each file watch and each first launch of a test binary is slow, and
  the watch tests can time out. `scripts/with-test-env.sh` shows a warning above 100 000 files. Then
  run `cargo clean`.
