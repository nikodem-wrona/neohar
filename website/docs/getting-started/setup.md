---
title: Setup
description: The toolchain, the first build, the checks and tests, the development app, the environment variables that matter, and the layout of the repository.
sidebar_position: 1
---

neohar is built and tested on macOS. The CI workflow runs `mise run check` and `mise run test` on a macOS runner, and the other suites run locally. Run every command from the root of the repository.

## Platform

| Platform | State |
|---|---|
| macOS | The only tested platform. Every check, test and budget run happens here. |
| Linux | Expected to build, not verified. The Tauri build needs `libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev librsvg2-dev`. |
| Windows | Not a target: the core uses Unix sockets and PTYs. |

## Toolchain

[mise](https://mise.jdx.dev) installs every tool at the version that [`mise.toml`](https://github.com/nikodem-wrona/neohar/blob/main/mise.toml) pins: Rust 1.99.0 (with the `wasm32-unknown-unknown` target), Bun 1.4.2, `cargo-nextest`, `cargo-deny`, `wasm-bindgen-cli`, `tauri-cli` and `cargo-about`.

```bash
mise install        # Rust, Bun and the cargo tools
bun install         # the Bun workspace: packages/*, apps/*, tests/* and three tools
```

:::warning[cargo outside mise uses another rustc]

A `cargo` from rustup or Homebrew is not the pinned version, and it builds into the same `target/`. Call cargo through mise: `mise exec -- cargo build -p nh`. `mise run <task>` always uses the pinned tools.

:::

mise does not install these:

| Program | Needed for |
|---|---|
| Xcode Command Line Tools (a C compiler) | Every Rust build (`rusqlite` and `zstd` compile C) |
| `git` | Task workspaces and their tests |
| `node` | The Playwright suites (`test:gpu`, `test:keyboard`, `test:shell`, `test:transport`, `test:phone`). Run `mise run setup:browsers` once for WebKit and Chromium |
| `less`, `vim`, `htop` | `mise run test:apps` |
| Claude Code and Codex | Daily use, `mise run test:harness` and `mise run record:golden` |

## First build, checks and tests

```bash
mise run build:wasm   # the WASM frame decoder into packages/proto-wasm/pkg (git-ignored)
mise run check        # fmt, clippy, cargo-deny, ESLint, tsc, crate rules, generated types, notices, generated pages, holder size, bundle size
mise run test         # Rust tests (nextest), UI tests (bun test), the test of app-install.sh
mise run fmt          # format the Rust and UI sources
```

`mise run test` points the three `NEOHAR_*_DIR` variables, `CLAUDE_CONFIG_DIR` and `CODEX_HOME` at a new temporary directory, so a test never touches your real config, data, sockets, `~/.claude` or `~/.codex`. The heavy suites (GPU, keyboard, chaos, budgets, live harnesses) are separate tasks; see [Local suites](/docs/testing/local-suites). [mise tasks](/docs/getting-started/mise-tasks) lists every task.

## Run the development app

```bash
mise run app:dev
```

The task builds the WASM decoder and `target/debug/nh`, then runs `cargo tauri dev` through [`scripts/dev-env.sh`](https://github.com/nikodem-wrona/neohar/blob/main/scripts/dev-env.sh). Vite serves the page on port 1420. The script sets:

| Variable | Value |
|---|---|
| `NEOHAR_CONFIG_DIR` | `target/dev-run/config` |
| `NEOHAR_DATA_DIR` | `target/dev-run/data` (the session `dev` has its `state.db` and logs in `dev/`) |
| `NEOHAR_RUNTIME_DIR` | `/tmp/nhdev-$UID`, short because a socket path on macOS has at most 103 bytes |
| `NEOHAR_NH` | `target/debug/nh`, the binary that starts the holders |

A debug build refuses to start unless the three directory variables are set, so `cargo run -p neohar-desktop` alone fails. A debug build also never touches `~/.local/bin/nh`. The core log goes to stderr; set `NEOHAR_LOG=info` (or any `tracing` filter) for more.

**Quit leaves the panes running.** That is by design: the next `app:dev` brings the panes back with their screens. To stop them:

- ⌘⌥Q (Quit and stop everything) in the app;
- `NEOHAR_STOP_ON_EXIT=1 mise run app:dev`, which stops the panes at every exit;
- `mise run app:stop` after the app ended.

:::warning[Leftover holders use up the PTYs]

Each holder keeps one pseudo-terminal, and macOS allows 511. Development runs that leave holders behind can use them all.

:::

`mise run app:dev:client` starts an `nh daemon` for the session `dev` and opens the app as its client through `link.sock`. Quit the app, then press Enter to open it again or `q` to stop the daemon.

### Point the CLI at the dev core

```bash
target/debug/nh --socket /tmp/nhdev-$(id -u)/dev/api.sock pane list
NEOHAR_RUNTIME_DIR=/tmp/nhdev-$(id -u) target/debug/nh --session dev pane list
```

In a pane, `NEOHAR_SOCKET` is set, so `nh` finds the core of that pane with no options.

## Build the app

`mise run build:app` makes the ad-hoc signed `neohar.app` in `target/release/bundle/macos`, with `nh` as its sidecar. [Install](/docs/guides/install) covers the install, the hooks and updates.

## Environment variables

[`crates/nh-proto/src/paths.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/paths.rs) is the only module that computes directories and socket paths. An empty value counts as unset.

| Variable | Meaning |
|---|---|
| `NEOHAR_CONFIG_DIR`, `NEOHAR_DATA_DIR`, `NEOHAR_RUNTIME_DIR` | Replace the config, data and runtime roots. Any one of them makes the run a test or development run. A runtime directory must be yours with mode 0700 |
| `NEOHAR_SESSION` | The session. App: `default`, or `dev` in a debug build. CLI: `default` |
| `NEOHAR_SOCKET` | The API socket. The CLI uses `--socket`, then `--session`, then this, then `NEOHAR_SESSION` |
| `NEOHAR_NH` | The `nh` binary that starts holders. When set, the app never touches `~/.local/bin/nh` |
| `NEOHAR_LOG` | A `tracing` filter. Default `warn` in a debug build, `info` in a product build |
| `NEOHAR_STOP_ON_EXIT` | Any value: every exit of the app stops the panes |

The core sets these in every pane: `NEOHAR=1`, `NEOHAR_SOCKET`, `NEOHAR_SESSION`, `NEOHAR_PANE_ID` (the handle), `NEOHAR_PANE_ULID`, `NEOHAR_WORKSPACE_ID`, `NEOHAR_TAB_ID`, `NEOHAR_BIN`, `NEOHAR_PANE_TOKEN` (binds hook calls to the pane), `TERM=xterm-256color`, `COLORTERM=truecolor` and `TERM_PROGRAM=neohar`. A task pane also gets `NEOHAR_TASK_ID`, `PORT`, `NEOHAR_PORTS` and the entries of `[tasks.env]`.

The local suites run only when a gate is set, and their `mise` task sets it: `NEOHAR_GPU=1` (`test:gpu`), `NEOHAR_HARNESS=1` (`test:harness`), `NEOHAR_APPS=1` (`test:apps`). `mise run test:remote` needs `NEOHAR_REMOTE_TARGET`, the ssh target of a second machine. `NEOHAR_BLESS=1` writes the expected files of the golden tests again; read the diff before you commit.

## Repository layout

| Path | What is there |
|---|---|
| `crates/` | The Rust crates: `nh-proto` (wire types), `nh-hold` (holder), `nh-term` (VT engine), `nh-store` (SQLite), `nh-rich`, `nh-redact`, `nh-agents`, `nh-tasks`, `nh-remote`, `nh-web`, `nh-core` and the binary `nh`. See [Code](/docs/code) |
| `apps/desktop/` | The Tauri app: `src-tauri/` is the host, `src/` mounts the UI |
| `apps/web/` | The entry of the phone app, which `nh-web` serves |
| `packages/` | `ui` (SolidJS), `transport` (Tauri and WebSocket transports, generated types), `proto-wasm` (the frame decoder) |
| `tools/` | Test, benchmark and build tools. See [Tools](/docs/code/tools) |
| `tests/` | Playwright suites, the VT corpus and the harness recordings |
| `scripts/` | The shell and Bun scripts behind the `mise` tasks |
| `website/` | This site |
| `mise.toml`, `Cargo.toml`, `package.json` | The pinned toolchain and every task, the Cargo workspace, the Bun workspace |

[`scripts/check-deps.ts`](https://github.com/nikodem-wrona/neohar/blob/main/scripts/check-deps.ts) enforces which crate may depend on which: `nh-proto` depends on no other crate and builds for `wasm32`, `nh-hold`, `nh-term` and `nh-store` never use `tokio`, and nothing below the app uses `tauri`.
