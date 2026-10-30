---
title: Tools
description: The test, measurement and code-generation tools under tools/ and the scripts under scripts/, and the mise tasks that run them.
sidebar_position: 2
---

The tools under [`tools/`](https://github.com/nikodem-wrona/neohar/tree/main/tools) exist for tests, measurements and code generation. None of them ships in `neohar.app`. Each Rust tool has a row in the dependency rules of [`scripts/check-deps.ts`](https://github.com/nikodem-wrona/neohar/blob/main/scripts/check-deps.ts), like the crates (see [Code map](/docs/code)). How to run the suites is on [Testing](/docs/testing/overview) and [Local suites](/docs/testing/local-suites).

## Tools

| Tool | What it does | Run by |
|---|---|---|
| [`mock-llm`](https://github.com/nikodem-wrona/neohar/tree/main/tools/mock-llm) | A scripted model server for the Anthropic Messages API and the OpenAI Responses API, so that real Claude Code and Codex run offline. Scenarios are TOML. | `nh-golden`; its own tests in `mise run test` |
| [`nh-golden`](https://github.com/nikodem-wrona/neohar/tree/main/tools/nh-golden) | Records real Claude Code and Codex sessions in a sandbox against `mock-llm`, and compares them with the committed recordings in `tests/harness-golden`. | `test:harness`, `record:golden` |
| [`nh-record`](https://github.com/nikodem-wrona/neohar/tree/main/tools/nh-record) | Runs a program under a real `nh hold` with a step script and saves the journal. It made the VT corpus in `tests/vt-corpus`. | by hand; `nh-golden` uses its library |
| [`nh-fixture`](https://github.com/nikodem-wrona/neohar/tree/main/tools/nh-fixture) | Generators of terminal output (`proptest` strategies, the `gen-vt` corpus) and the `tui` and `echo-seq` programs of the chaos test. | tests; `nh-chaos` |
| [`nh-snapcheck`](https://github.com/nikodem-wrona/neohar/tree/main/tools/nh-snapcheck) | Checks that an engine restored from a snapshot continues exactly like the original. | a sample in `mise run test`; all cut points by hand (`corpus --every 1`) |
| [`nh-chaos`](https://github.com/nikodem-wrona/neohar/tree/main/tools/nh-chaos) | Kills a headless host with `SIGKILL` under live panes and compares every restored screen with a reference engine. Also cleans up the panes of a dev session. | `build:chaos`, then by hand; `app:stop` |
| [`nh-bench`](https://github.com/nikodem-wrona/neohar/tree/main/tools/nh-bench) | Writes the frame files of the render tests (`gen-frames`). | `test:gpu` |
| [`nh-keytest`](https://github.com/nikodem-wrona/neohar/tree/main/tools/nh-keytest) | The keyboard fidelity matrix: the cases through the real encoder, and the server of the browser test. | a Rust test in `mise run test`; `test:keyboard` |
| [`gen-ts`](https://github.com/nikodem-wrona/neohar/tree/main/tools/gen-ts) | Writes `packages/transport/src/generated.ts` from `nh api schema`. | `gen:api`, `check:gen` |
| [`eslint-plugin-neohar`](https://github.com/nikodem-wrona/neohar/tree/main/tools/eslint-plugin-neohar) | The thin-presentation rules of the webview: an import allowlist, no HTML injection, no synthetic `beforeinput` of React, no timers, no sorting or locale formatting. | `check:eslint` |

The heavy runs (`bench`, `test:harness`, and the chaos tool by hand) are not part of `mise run test`. They run on a Mac: the chaos tool needs a release build, and `test:harness` needs both harnesses installed.

## Scripts

[`scripts/`](https://github.com/nikodem-wrona/neohar/tree/main/scripts) holds the static checks and the wrappers of the tasks. `tsc --noEmit -p scripts` type-checks the TypeScript ones in `check:tsc`.

| Script | Task | What it does |
|---|---|---|
| `commits.ts` | `check:commits`, the commit-msg hook, the commits workflow | The types and the scopes of a commit message, and its lint |
| `release.ts` | `release`, `check:version`, the release workflow | The version files, the next version from the commits, the sections of `CHANGELOG.md` |
| `check-deps.ts` | `check:deps` | The crate dependency rules and the `wasm-bindgen` pin |
| `check-bundle.ts` | `check:bundle` | The gzipped bundles of the webview and of the phone app against 400 KB each, and the first chunk of the phone app against 250 KB |
| `holder-size.ts` | `check:holder-size` | The lines of code of `crates/nh-hold/src` against 2,500 |
| `gen-notices.ts` | `gen:notices`, `check:notices` | Writes `THIRD_PARTY_LICENSES.md` from cargo-about (`about.toml`) and `node_modules` |
| `tauri-commands.ts` | its test in `bun test` | The webview calls only `rpc`, `subscribe` and `stream` |
| `with-test-env.sh` | `test:rust`, `test:ui`, `test:apps`, `test:harness` | A new temporary directory for the three `NEOHAR_*_DIR` variables, `CLAUDE_CONFIG_DIR` and `CODEX_HOME` |
| `dev-env.sh` | `app:dev`, `app:dev:client` | The directories of a development session under `target/dev-run` |
| `app-dev-client.sh` | `app:dev:client` | Starts `nh daemon` and opens the app as its client through `link.sock` |
| `test-gpu.sh`, `test-keyboard.sh`, `test-shell.sh`, `test-transport.sh` | `test:gpu`, `test:keyboard`, `test:shell`, `test:transport` | Playwright in `tests/render`, `tests/keyboard`, `tests/ui` and `tests/transport` |
| `test-phone.sh` | `test:phone` | Playwright in `tests/phone`: the phone app as an iPhone 17 Pro Max against the listener of the phone, and as an iPhone 15 for the one `@narrow` test |
| `test-host.sh` | `test:host` | The sandbox check of the webview in a debug app |
| `build-sidecar.sh` | `build:sidecar` | `nh` in release at the sidecar path of the bundle |
| `bench-shim.sh`, `shim-bench.ts` | by hand | The start time of `nh hook` through a symlink |
| `app-install.sh`, `app-install.test.sh` | `app:install`, `test:install` | Copies `neohar.app` to `~/Applications`, and its test on folders under `/tmp` |
| `remote-dev.sh`, `test-remote.sh` | `remote:deploy`, `test:remote` | A development `nh` on a second machine, and the tests over real `ssh` (needs `NEOHAR_REMOTE_TARGET`) |

Every task is listed on [mise tasks](/docs/getting-started/mise-tasks).
