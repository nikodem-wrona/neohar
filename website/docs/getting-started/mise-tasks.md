---
title: "mise tasks"
description: "All 44 tasks of mise.toml: what each one does, what it depends on and the commands it runs."
sidebar_position: 6
toc_max_heading_level: 3
---

:::info[Generated page]

This page is generated from [`mise.toml`](https://github.com/nikodem-wrona/neohar/blob/main/mise.toml) by `website/scripts/gen-tasks.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

Run every task from the root of the repository with `mise run <task>`. Arguments after `--` go to the last command of the task, for example `mise run app:install -- --yes`. The CI workflow runs `mise run check` and `mise run test` on macOS. A task that its description marks as local is not part of `mise run test` and needs more than a checkout, for example a browser, a GPU or an installed harness. CI does not run it.

## Toolchain

mise installs these tools at these versions (`mise install`):

| Tool | Version |
|---|---|
| `rust` | 1.99.0 |
| `bun` | 1.4.2 |
| `hyperfine` | 1.21.0 |
| `cargo-binstall` | 1.25.2 |
| `cargo:cargo-nextest` | 0.9.146 |
| `cargo:cargo-deny` | 0.20.2 |
| `cargo:wasm-bindgen-cli` | 0.2.129 |
| `cargo:tauri-cli` | 2.12.1 |
| `cargo:cargo-about` | 0.9.2 |

## Summary

| Task | What it does | Note |
|---|---|---|
| [`fmt`](#fmt) | Format Rust and UI sources | format the Rust and UI sources |
| [`check:fmt`](#check-fmt) | Check Rust and UI formatting |  |
| [`check:clippy`](#check-clippy) | Run clippy on the whole workspace with warnings as errors |  |
| [`check:deny`](#check-deny) | Run cargo-deny (licenses, advisories, bans, sources) |  |
| [`check:eslint`](#check-eslint) | Run ESLint, including the thin-presentation rules |  |
| [`check:tsc`](#check-tsc) | Type-check every package and script |  |
| [`check:gen`](#check-gen) | Fail when packages/transport/src/generated.ts is not what `nh api schema` makes |  |
| [`check:notices`](#check-notices) | Fail when THIRD_PARTY_LICENSES.md is not what gen:notices writes (offline) |  |
| [`check:deps`](#check-deps) | Check the crate dependency rules (scripts/check-deps.ts) and the wasm-bindgen pin |  |
| [`check:bundle`](#check-bundle) | Fail when the gzipped scripts, styles and WASM of the webview are over 300 KB, or those of the phone app over 150 KB. It builds both first |  |
| [`check:docs`](#check-docs) | Fail when a generated page of website/docs is not what `bun run gen` writes, or when the page checker finds a fault (offline, writes nothing). The CLI pages need an nh binary in target/ that is newer than crates/nh/src |  |
| [`check:holder-size`](#check-holder-size) | Fail when crates/nh-hold/src has more than 2,500 lines of code |  |
| [`check`](#check) | Every static check | fmt, clippy, cargo-deny, ESLint, tsc, Prettier, crate dependency rules, generated.ts, third-party notices, generated docs, holder size, bundle size |
| [`test:rust`](#test-rust) | Run the Rust tests |  |
| [`test:ui`](#test-ui) | Run the UI and tooling tests |  |
| [`test`](#test) | Every test that does not need a live harness, a GPU or a browser | Rust tests (nextest), UI tests (bun test) and the test of app-install.sh |
| [`test:gpu`](#test-gpu) | Render tests in browsers: renderers equal, context loss, no allocation, budgets (needs node) |  |
| [`test:keyboard`](#test-keyboard) | The keyboard fidelity matrix in WebKit: DOM events, the keyboard layer, the encoder (local, needs node) |  |
| [`test:transport`](#test-transport) | The WebSocketTransport in WebKit against a headless core: a snapshot, then a patch after a CLI call (local, needs node) |  |
| [`test:shell`](#test-shell) | The shell of the UI in WebKit against a headless core: the DOM follows the projections, a click sends its command (local, needs node) |  |
| [`test:phone`](#test-phone) | The phone app in WebKit as an iPhone 17 Pro Max against a headless core and its phone listener: the pairing, the desk, the cards, the reply, the sheets, the banner (local, needs node) |  |
| [`test:shell:link`](#test-shell-link) | The shell tests of tests/ui through link.sock: the core serves the page from a client of link/1 (local, needs node) |  |
| [`test:remote`](#test-remote) | The remote tests over real ssh to a second machine, crates/nh/tests/remote_*.rs (needs NEOHAR_REMOTE_TARGET; local) |  |
| [`test:host`](#test-host) | The sandbox of the webview: only the three commands, the CSP, no network (local, opens a window) |  |
| [`test:install`](#test-install) | The test of scripts/app-install.sh, on folders under /tmp (it never touches ~/Applications) |  |
| [`test:apps`](#test-apps) | Real programs in panes of nh-core: less, vim and htop (local, needs them installed) |  |
| [`test:harness`](#test-harness) | Record the scenarios of the installed Claude Code and Codex offline and compare them with the committed recordings (local, macOS, needs both installed) |  |
| [`bench`](#bench) | Run the benches of nh-term against their budgets: frame build, snapshot and restore |  |
| [`build:nh`](#build-nh) | Build the nh binary that the app starts holders with (target/debug/nh) |  |
| [`build:web`](#build-web) | Build the phone app into target/web-dist. The builds of nh and of the app put it into the binary with NEOHAR_WEB_DIST |  |
| [`build:sidecar`](#build-sidecar) | Build nh in release, with the phone app, and copy it to the sidecar path of the bundle (apps/desktop/src-tauri/binaries) |  |
| [`build:app`](#build-app) | Build the ad-hoc signed neohar.app with the nh sidecar and the phone app (local). The bundle is under target/release/bundle/macos | the ad-hoc signed neohar.app; mise run app:install copies it to ~/Applications |
| [`app:dev`](#app-dev) | Open the app with the shell, built from the working tree (local). Config and data are under target/dev-run, the sockets under /tmp/nhdev-$UID | open the app built from the working tree (config and data under target/dev-run, sockets under /tmp/nhdev-$UID) |
| [`app:dev:client`](#app-dev-client) | Start nh daemon for the dev session and open the app as its client through link.sock (local). A quit of the app leaves the daemon running; Enter opens the app again, q stops the daemon and its panes |  |
| [`app:install`](#app-install) | Copy the ad-hoc signed neohar.app of build:app to ~/Applications (local). It keeps an installed app unless you pass --yes: mise run app:install -- --yes |  |
| [`build:chaos`](#build-chaos) | Build the release binaries of the chaos runs: nh, nh-fixture and nh-chaos (local) |  |
| [`app:stop`](#app-stop) | Kill the panes of the dev app (target/dev-run). A quit of the app leaves its panes running, and the next launch brings them back | kill the panes of the dev app |
| [`build:wasm`](#build-wasm) | Build the WASM frame decoder and its JavaScript glue into packages/proto-wasm/pkg |  |
| [`gen:api`](#gen-api) | Write packages/transport/src/generated.ts from `nh api schema` |  |
| [`gen:icons`](#gen-icons) | Write the icons of the Files tab from the package material-icon-theme: the tables of names in nh-core and the art in packages/ui |  |
| [`gen:notices`](#gen-notices) | Write THIRD_PARTY_LICENSES.md: the licenses of the Rust crates, JavaScript packages and data that ship in neohar.app (cargo-about with about.toml, then node_modules) |  |
| [`record:golden`](#record-golden) | Record every scenario of the installed Claude Code and Codex into tests/harness-golden (local, macOS) |  |
| [`setup:browsers`](#setup-browsers) | Download the browsers of the render tests: Playwright WebKit and Chromium (needs the network and node) |  |
| [`remote:deploy`](#remote-deploy) | Copy a development nh and its wrapper nh-dev to ~/.neohar-dev/bin on a second machine over ssh (needs NEOHAR_REMOTE_TARGET; local). Options after --: --release, --no-build, --bin &lt;path> |  |

## Checks

Static checks. `mise run check` runs them all.

### `fmt` {#fmt}

Format Rust and UI sources

**AGENTS.md:** format the Rust and UI sources

```bash
cargo fmt --all
bun run fmt
```

### `check:fmt` {#check-fmt}

Check Rust and UI formatting

```bash
cargo fmt --all --check
bun run fmt:check
```

### `check:clippy` {#check-clippy}

Run clippy on the whole workspace with warnings as errors

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p nh --features dev-ws --all-targets --locked -- -D warnings
cargo clippy -p neohar-desktop --features selftest --all-targets --locked -- -D warnings
```

### `check:deny` {#check-deny}

Run cargo-deny (licenses, advisories, bans, sources)

```bash
cargo deny --locked check
```

### `check:eslint` {#check-eslint}

Run ESLint, including the thin-presentation rules

```bash
bun run lint
```

### `check:tsc` {#check-tsc}

Type-check every package and script

**Depends on:** [`build:wasm`](#build-wasm)

```bash
bun run typecheck
```

### `check:gen` {#check-gen}

Fail when packages/transport/src/generated.ts is not what `nh api schema` makes

```bash
bun tools/gen-ts/index.ts --check
```

### `check:notices` {#check-notices}

Fail when THIRD_PARTY_LICENSES.md is not what gen:notices writes (offline)

```bash
bun scripts/gen-notices.ts --check
```

### `check:deps` {#check-deps}

Check the crate dependency rules (scripts/check-deps.ts) and the wasm-bindgen pin

```bash
bun scripts/check-deps.ts
```

### `check:bundle` {#check-bundle}

Fail when the gzipped scripts, styles and WASM of the webview are over 300 KB, or those of the phone app over 150 KB. It builds both first

**Depends on:** [`build:wasm`](#build-wasm)

```bash
bun run --cwd apps/desktop build
bun run --cwd apps/web build
bun scripts/check-bundle.ts
```

### `check:docs` {#check-docs}

Fail when a generated page of website/docs is not what `bun run gen` writes, or when the page checker finds a fault (offline, writes nothing). The CLI pages need an nh binary in target/ that is newer than crates/nh/src

```bash
bun website/scripts/gen-all.ts --check
bun run --cwd website check
```

### `check:holder-size` {#check-holder-size}

Fail when crates/nh-hold/src has more than 2,500 lines of code

```bash
bun scripts/holder-size.ts
```

### `check` {#check}

Every static check

**AGENTS.md:** fmt, clippy, cargo-deny, ESLint, tsc, Prettier, crate dependency rules, generated.ts, third-party notices, generated docs, holder size, bundle size

**Depends on:** [`check:fmt`](#check-fmt), [`check:clippy`](#check-clippy), [`check:deny`](#check-deny), [`check:eslint`](#check-eslint), [`check:tsc`](#check-tsc), [`check:deps`](#check-deps), [`check:gen`](#check-gen), [`check:notices`](#check-notices), [`check:docs`](#check-docs), [`check:holder-size`](#check-holder-size), [`check:bundle`](#check-bundle)

It runs only its dependencies.

## Tests

Test suites. `mise run test` runs the Rust and UI tests; the others are local suites.

### `test:rust` {#test-rust}

Run the Rust tests

Tests run with the three directory overrides pointing at a fresh temp dir.

```bash
bash scripts/with-test-env.sh cargo nextest run --workspace --locked
```

### `test:ui` {#test-ui}

Run the UI and tooling tests

**Depends on:** [`build:wasm`](#build-wasm)

```bash
bash scripts/with-test-env.sh bun test
```

### `test` {#test}

Every test that does not need a live harness, a GPU or a browser

**AGENTS.md:** Rust tests (nextest), UI tests (bun test) and the test of app-install.sh

**Depends on:** [`test:rust`](#test-rust), [`test:ui`](#test-ui), [`test:install`](#test-install)

It runs only its dependencies.

### `test:gpu` {#test-gpu}

Render tests in browsers: renderers equal, context loss, no allocation, budgets (needs node)

**Depends on:** [`build:wasm`](#build-wasm)

```bash
bash scripts/test-gpu.sh
```

### `test:keyboard` {#test-keyboard}

The keyboard fidelity matrix in WebKit: DOM events, the keyboard layer, the encoder (local, needs node)

```bash
bash scripts/test-keyboard.sh
```

### `test:transport` {#test-transport}

The WebSocketTransport in WebKit against a headless core: a snapshot, then a patch after a CLI call (local, needs node)

```bash
bash scripts/test-transport.sh
```

### `test:shell` {#test-shell}

The shell of the UI in WebKit against a headless core: the DOM follows the projections, a click sends its command (local, needs node)

**Depends on:** [`build:wasm`](#build-wasm)

```bash
bash scripts/test-shell.sh
```

### `test:phone` {#test-phone}

The phone app in WebKit as an iPhone 17 Pro Max against a headless core and its phone listener: the pairing, the desk, the cards, the reply, the sheets, the banner (local, needs node)

**Depends on:** [`build:wasm`](#build-wasm)

```bash
bash scripts/test-phone.sh
```

### `test:shell:link` {#test-shell-link}

The shell tests of tests/ui through link.sock: the core serves the page from a client of link/1 (local, needs node)

**Depends on:** [`build:wasm`](#build-wasm) · **Environment:** `NEOHAR_UI_LINK=1`

```bash
bash scripts/test-shell.sh
```

### `test:remote` {#test-remote}

The remote tests over real ssh to a second machine, crates/nh/tests/remote_*.rs (needs NEOHAR_REMOTE_TARGET; local)

```bash
bash scripts/test-remote.sh
```

### `test:host` {#test-host}

The sandbox of the webview: only the three commands, the CSP, no network (local, opens a window)

**Depends on:** [`build:wasm`](#build-wasm)

```bash
bash scripts/test-host.sh
```

### `test:install` {#test-install}

The test of scripts/app-install.sh, on folders under /tmp (it never touches ~/Applications)

```bash
bash scripts/app-install.test.sh
```

### `test:apps` {#test-apps}

Real programs in panes of nh-core: less, vim and htop (local, needs them installed)

```bash
bash scripts/with-test-env.sh env NEOHAR_APPS=1 cargo nextest run -p nh --test core_apps --locked
```

### `test:harness` {#test-harness}

Record the scenarios of the installed Claude Code and Codex offline and compare them with the committed recordings (local, macOS, needs both installed)

**Depends on:** [`build:nh`](#build-nh)

```bash
bash scripts/with-test-env.sh env NEOHAR_HARNESS=1 cargo nextest run -p nh-golden --test record --test delivery --locked
```

## Benchmarks

The benches of the VT engine against their budgets. They are local: CI does not run them.

### `bench` {#bench}

Run the benches of nh-term against their budgets: frame build, snapshot and restore

Local suites. They need a browser, a GPU, installed programs or a second machine, so `mise run test` does not run them.

```bash
cargo bench -p nh-term
```

## Build and run

Building the WASM decoder, the sidecar and the app, and running the app in development.

### `build:nh` {#build-nh}

Build the nh binary that the app starts holders with (target/debug/nh)

```bash
cargo build -p nh --locked
```

### `build:web` {#build-web}

Build the phone app into target/web-dist. The builds of nh and of the app put it into the binary with NEOHAR_WEB_DIST

**Depends on:** [`build:wasm`](#build-wasm)

```bash
bun run --cwd apps/web build
```

### `build:sidecar` {#build-sidecar}

Build nh in release, with the phone app, and copy it to the sidecar path of the bundle (apps/desktop/src-tauri/binaries)

**Depends on:** [`build:web`](#build-web) · **Environment:** `NEOHAR_WEB_DIST={{config_root}}/target/web-dist`

```bash
bash scripts/build-sidecar.sh
```

### `build:app` {#build-app}

Build the ad-hoc signed neohar.app with the nh sidecar and the phone app (local). The bundle is under target/release/bundle/macos

**AGENTS.md:** the ad-hoc signed neohar.app; mise run app:install copies it to ~/Applications

**Depends on:** [`build:wasm`](#build-wasm), [`build:web`](#build-web), [`build:sidecar`](#build-sidecar) · **Directory:** `apps/desktop` · **Environment:** `NEOHAR_WEB_DIST={{config_root}}/target/web-dist`

```bash
cargo tauri build --bundles app -c src-tauri/tauri.bundle.conf.json
```

### `app:dev` {#app-dev}

Open the app with the shell, built from the working tree (local). Config and data are under target/dev-run, the sockets under /tmp/nhdev-$UID

**AGENTS.md:** open the app built from the working tree (config and data under target/dev-run, sockets under /tmp/nhdev-$UID)

**Depends on:** [`build:wasm`](#build-wasm), [`build:web`](#build-web), [`build:nh`](#build-nh) · **Directory:** `apps/desktop` · **Environment:** `NEOHAR_WEB_DIST={{config_root}}/target/web-dist`

```bash
bash ../../scripts/dev-env.sh cargo tauri dev
```

### `app:dev:client` {#app-dev-client}

Start nh daemon for the dev session and open the app as its client through link.sock (local). A quit of the app leaves the daemon running; Enter opens the app again, q stops the daemon and its panes

**Depends on:** [`build:wasm`](#build-wasm)

```bash
bash scripts/dev-env.sh bash scripts/app-dev-client.sh
```

### `app:install` {#app-install}

Copy the ad-hoc signed neohar.app of build:app to ~/Applications (local). It keeps an installed app unless you pass --yes: mise run app:install -- --yes

```bash
bash scripts/app-install.sh
```

### `build:chaos` {#build-chaos}

Build the release binaries of the chaos runs: nh, nh-fixture and nh-chaos (local)

```bash
cargo build --release --locked -p nh -p nh-fixture -p nh-chaos
```

### `app:stop` {#app-stop}

Kill the panes of the dev app (target/dev-run). A quit of the app leaves its panes running, and the next launch brings them back

**AGENTS.md:** kill the panes of the dev app

```bash
cargo run -q -p nh-chaos -- clean --data-dir target/dev-run/data --session dev
```

### `build:wasm` {#build-wasm}

Build the WASM frame decoder and its JavaScript glue into packages/proto-wasm/pkg

```bash
cargo build --profile wasm --target wasm32-unknown-unknown -p proto-wasm --locked
wasm-bindgen --target web --out-dir packages/proto-wasm/pkg --out-name proto_wasm target/wasm32-unknown-unknown/wasm/proto_wasm.wasm
```

## Generation and recording

Generated code and recorded harness sessions.

### `gen:api` {#gen-api}

Write packages/transport/src/generated.ts from `nh api schema`

```bash
bun tools/gen-ts/index.ts
```

### `gen:icons` {#gen-icons}

Write the icons of the Files tab from the package material-icon-theme: the tables of names in nh-core and the art in packages/ui

```bash
bun scripts/gen-file-icons.ts
```

### `gen:notices` {#gen-notices}

Write THIRD_PARTY_LICENSES.md: the licenses of the Rust crates, JavaScript packages and data that ship in neohar.app (cargo-about with about.toml, then node_modules)

```bash
cargo fetch --locked
bun scripts/gen-notices.ts
```

### `record:golden` {#record-golden}

Record every scenario of the installed Claude Code and Codex into tests/harness-golden (local, macOS)

**Depends on:** [`build:nh`](#build-nh)

```bash
cargo run -q -p nh-golden --locked -- record-all claude --dir tests/harness-golden/scenarios/claude --out-root tests/harness-golden
cargo run -q -p nh-golden --locked -- record-all codex --dir tests/harness-golden/scenarios/codex --out-root tests/harness-golden
```

## Setup

One-time setup.

### `setup:browsers` {#setup-browsers}

Download the browsers of the render tests: Playwright WebKit and Chromium (needs the network and node)

```bash
cd tests/render && ./node_modules/.bin/playwright install webkit chromium
```

## Other tasks

### `remote:deploy` {#remote-deploy}

Copy a development nh and its wrapper nh-dev to ~/.neohar-dev/bin on a second machine over ssh (needs NEOHAR_REMOTE_TARGET; local). Options after --: --release, --no-build, --bin &lt;path>

```bash
bash scripts/remote-dev.sh
```
