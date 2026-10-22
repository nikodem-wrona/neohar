# AGENTS.md

This file gives guidance to coding agents that work in this repository. [CONTRIBUTING.md](CONTRIBUTING.md) gives the full rules for a contribution.

## What this repository is

neohar (always lowercase) is a Tauri 2 desktop workspace to supervise coding agents. A Rust core owns the terminals, the agent state, the approvals and the task workspaces. The core keeps agent processes alive when the app quits, crashes or updates. The webview is a thin surface that paints what the core sends. neohar supports Claude Code and Codex only.

The code is the reference: there is no separate spec or plan. `website/` is the documentation site (Docusaurus, with its own `bun install`). Its guide is short on purpose.

Pages with a "Generated page" note come from `bun run gen` in `website/`. Never edit them by hand. If a change makes a guide page wrong, fix the page in the same commit.

The layout:

- `crates/`: the Cargo workspace.
- `apps/desktop/`: the Tauri app. `apps/web/`: the phone app.
- `packages/`: the UI packages.
- `tools/`: the test and bench tools.
- `mise.toml`: the pinned toolchain (Rust stable, Bun) and every task.

## Commands

Run every command from the root of the repository. `mise tasks` lists every task with its description.

```bash
mise install        # Rust, Bun and the cargo tools
bun install         # UI and tooling packages
mise run check      # commit messages, versions, fmt, clippy, cargo-deny, ESLint, tsc, Prettier, crate dependency rules, generated.ts, third-party notices, generated docs, holder size, bundle size
mise run test       # Rust tests (nextest), UI tests (bun test) and the test of app-install.sh
mise run fmt        # format the Rust and UI sources
mise run app:dev    # open the app built from the working tree (config and data under target/dev-run, sockets under /tmp/nhdev-$UID)
mise run app:stop   # kill the panes of the dev app
mise run build:app  # the ad-hoc signed neohar.app; mise run app:install copies it to ~/Applications
mise run release    # the release commit and the tag vX.Y.Z (only when the maintainer asks)
```

- `mise run test` sets `NEOHAR_CONFIG_DIR`, `NEOHAR_DATA_DIR`, `NEOHAR_RUNTIME_DIR`, `CLAUDE_CONFIG_DIR` and `CODEX_HOME` to a new temporary directory (`scripts/with-test-env.sh`). A debug build of the app refuses to start without the three `NEOHAR_*_DIR` variables, so use `app:dev`.
- The CI workflow (`.github/workflows/ci.yml`) runs `mise run check` and `mise run test` on a macOS runner. The commits workflow lints the commit messages, and the release workflow publishes each tag `v*`. CI does not run the local suites: budget, GPU, keyboard, apps, shell, phone, harness and remote. Some of them need node, a screen, installed harnesses or a second machine (`NEOHAR_REMOTE_TARGET`). See `website/docs/testing/local-suites.md`.
- The panes continue to run after a quit of the app. `NEOHAR_STOP_ON_EXIT=1` stops them.
- The Playwright specs end in `.pw.ts`, so `bun test` does not run them.
- On Linux, the app build needs `libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev librsvg2-dev`.

## Change the API

A change to the API in `nh-proto` has three generated outputs. Make all three in the same commit:

1. Run `mise run gen:api`. It writes `packages/transport/src/generated.ts`. `mise run check:gen` fails without it.
2. Accept the insta snapshot with `INSTA_UPDATE=always bash scripts/with-test-env.sh cargo nextest run -p nh-proto --features api`. Use this command because `cargo insta` is not installed. The schema tests of `nh-proto` run only with `--features api`.
3. Run `bun run gen` in `website/`. `mise run check:docs` fails without it.

## Commits, versions and releases

[CONTRIBUTING.md](CONTRIBUTING.md#commits-versions-and-releases) has the full rules.

- **Conventional Commits.** Write each commit message as `<type>(<scope>): <description>`. The types and the scopes are in `scripts/commits.ts`. The description is an STE instruction that starts with a verb in lowercase and has no period. The header has at most 72 characters.
- **The checks.** The commit-msg hook in `.githooks/` refuses a message with a fault. If it refuses your message, correct the message and commit again. Do not use `--no-verify`: `mise run check` and CI lint the commits again.
- **The type sets the version.** `feat` makes a minor release. `fix`, `perf` and `revert` make a patch release. A breaking change of the public interface gets a `!` and a `BREAKING CHANGE:` footer, and makes a major release. CONTRIBUTING.md tells what the public interface is. Do not put a feature in a `fix`, a `refactor` or a `chore`.
- **Never change a version by hand.** Only `mise run release` writes the version, the sections of `CHANGELOG.md` and the tags `v*`. Run it only when the maintainer asks for a release. Then run `mise run check` and `mise run test`, and push with `git push --atomic origin main vX.Y.Z`. The tag publishes the GitHub release.
- **Linear history.** Make no merge commits. Squash each `fixup!` commit before you push to `main`.

## Scope and fixed choices

- **Harnesses:** Claude Code and Codex, and no other harness. No extension transport, no generic agent protocol, no external adapter protocol, no plugin system.
- **Name:** `neohar`, lowercase everywhere, also in `neohar.app` and at the start of a sentence.
- **UI:** SolidJS and TypeScript, Vite, Bun.
- **VT engine:** `alacritty_terminal` behind the `VtEngine` trait, with `wezterm-term` as the named fallback. Not libghostty-vt: the cut points of the holder make parser-state snapshots unnecessary, and the Tauri build must stay free of foreign toolchains.
- **Directories:** config in `~/.config/neohar/`, data in the app-support directory of the platform, and runtime sockets under `/tmp/neohar-$UID/` on macOS and `$XDG_RUNTIME_DIR` on Linux. On macOS, a socket path under `$TMPDIR` is longer than the limit.
- **Heavy tests:** the budget, GPU, chaos, live-harness and remote suites run locally, never in CI.

## Architecture in one screen

These pieces take several files to see together.

- **Two binaries.** `neohar.app` (Tauri) links the core as a library. `nh` is a small sidecar with these roles: `hold` (the PTY owner of one pane), `hook` (the shim that agent hooks call, with a budget of 5 ms), the CLI, `daemon` (the core without windows, for remote machines), and the link helpers `bridge` (the remote end of an ssh link) and `askpass` (it answers the questions of ssh). Hooks reference `~/.local/bin/nh`, a symlink that the app keeps, so updates never break them.
- **Holders are the durability core.** One `nh hold` for each pane owns the PTY and keeps its output in a journal ring with safe cut points. The app can quit, crash or update, and the holders and agents continue to run. A relaunch restores the screens from a snapshot and a replay of the journal. Input carries ids and is at-most-once.
- **The core** (`nh-core`) is a library with a single command bus into a state actor. Every mutation is one SQLite transaction that updates the state tables and appends events (transactional outbox). Reads come from in-memory projections.
- **Rendering.** VT parsing, damage tracking, snapshots, selection and keyboard encoding occur in Rust. The webview receives binary `postcard` frames of packed cells. A WASM build of `nh-proto` decodes them, and one WebGL2 context for each window draws them.
- **The thin-presentation rule.** The webview holds no business state, predicts nothing and parses no bytes. It speaks the same JSON-RPC methods as the CLI through a `Transport` abstraction. Counts, sort orders, risk, search results, dates and highlights come from the core. A pure view mapper with unit tests in a `*/model.ts` file can turn those facts into rows, labels and call parameters. ESLint (`tools/eslint-plugin-neohar`) enforces the rule. If the UI needs something new, add it to the API in `nh-proto` first.
- **Adapters.** Agent state is five independent facets, each with a `source` and a `confidence`. Structured signals (hooks, protocols) win over screen reading. Approvals, questions and plan reviews are `Interaction` objects. A recoverable delivery transaction sends each answer. Gate mode follows the desktop focus: neohar holds the approval of an unfocused pane and shows it as a card or a native notification. A focus on the pane releases it, so the dialog of the harness appears.

## Conventions

[CONTRIBUTING.md](CONTRIBUTING.md) has the full list. In short:

- **Rust:** `thiserror` in libraries, `anyhow` only in binaries and tools. No `unwrap()` outside tests. `tracing` everywhere except `nh-hold` and the hook shim. Every `unsafe` block has a `// SAFETY:` comment. `scripts/check-deps.ts` holds the crate dependency rules (for example, no `tokio` in `nh-proto`, `nh-hold` or `nh-term`).
- **Wire types** live only in `nh-proto`. The `holder` and `frame` modules change only by appends: never reorder or remove a variant or a field.
- **Temporary directories.** Every test sets the three `NEOHAR_*_DIR` variables. Harness tests also set `HOME`, `CLAUDE_CONFIG_DIR` and `CODEX_HOME`. The tests do not change `PATH`, so a core can run an installed `claude` or `codex` to read its version. No test uses the network outside `127.0.0.1`. A test that starts holders kills them on drop.
- **Gates, not skips.** No `#[ignore]` or `.skip` to make a check pass. An environment variable gates each test that needs a live harness, a GPU or an installed program. Its `mise` task sets the variable.
- **Writing:** `AGENTS.md`, `README.md`, `CONTRIBUTING.md`, the guide pages and the commit messages use Simplified Technical English.
