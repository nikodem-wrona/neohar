# Contributing to neohar

Thank you for your help. This file tells how to set up the repository, how to send a change, and which rules a change must obey. [AGENTS.md](AGENTS.md) has the same rules in a short form for coding agents.

For a large change, open an issue first. Then we can agree on the design before you write the code.

## Set up

You need macOS, the Xcode Command Line Tools (`xcode-select --install`), git and [mise](https://mise.jdx.dev). Run every command from the root of the repository.

```bash
mise install        # the pinned Rust, Bun and cargo tools of mise.toml
bun install         # the Bun workspace: packages/*, apps/*, tests/* and the tools
mise run app:dev    # open the app built from the working tree
```

`mise run app:dev` keeps its config and data under `target/dev-run` and its sockets under `/tmp/nhdev-$UID`. It never touches your installed neohar. The panes continue to run after a quit of the app. `mise run app:stop` stops them.

Call cargo through mise, for example `mise exec -- cargo build -p nh`. A `cargo` from rustup or Homebrew is not the pinned version, and it builds into the same `target/`.

[Setup](website/docs/getting-started/setup.md) has more: the environment variables, the layout of the repository, and the programs that mise does not install.

### Linux

neohar is not tested on Linux. If you try a build there, install these packages first:

```bash
sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev librsvg2-dev
```

## Send a pull request

1. Fork the repository on GitHub.
2. Make a branch from `main` in your fork.
3. Make your change, with its tests and the pages of the guide that it makes wrong.
4. Run `mise run check` and `mise run test`. Both must pass.
5. Run the local suites that cover your change (see [Local suites](#local-suites)). Put their results in the pull request.
6. Push the branch and open a pull request against `main`.

The CI workflow runs `mise run check` and `mise run test` on macOS for each pull request. The maintainer reviews each pull request. Keep a pull request to one change. Do not include refactors that the change does not need.

### Commit messages

Write commit messages in Simplified Technical English (STE). Give a short subject in the imperative, and the evidence in the body. A commit that changes a path with a budget includes the numbers of the budget run.

## Conventions

### Rust

- Use `thiserror` in libraries. Use `anyhow` only in binaries and tools.
- Do not use `unwrap()` outside tests.
- Use `tracing` everywhere except in `nh-hold` and the hook shim.
- Give every `unsafe` block a `// SAFETY:` comment.
- `scripts/check-deps.ts` holds the crate dependency rules. For example, `nh-proto`, `nh-hold` and `nh-term` have no `tokio`. A new crate fails the check until it has a row there.

### Wire types

- Wire types live only in `nh-proto`.
- The `holder` and `frame` modules change only by appends. Never reorder or remove a variant or a field. Holders outlive the app, so a new core must talk to old holders.

### The thin-presentation rule

The webview paints what the core sends. All logic lives in the core (Rust).

- The webview holds no business state. Only view state (hover, scroll, an open dialog, column widths) lives in the page.
- The webview predicts nothing. Each action is a command, and the page shows the result when the patch arrives.
- Counts, sort orders, risk, search results, dates and highlights come from the core.
- The webview parses no bytes. The WASM decoder turns frames into typed arrays.
- The webview uses the same JSON-RPC methods as the CLI.

A pure view mapper is permitted in a `model.ts` file next to its view, for example `packages/ui/src/cards/model.ts`. A view mapper turns the facts of the core into rows, labels, buttons and call parameters. It must be a pure function with no state and no calls, and it must have unit tests in `model.test.ts`. A view mapper must not decide a fact of the core: no reducer of business events, no store that combines two projections, no client-side filter, no optimistic update.

ESLint (`tools/eslint-plugin-neohar`) enforces a part of the rule, and the review checks the remainder. If the UI needs something new, add it to the API in `nh-proto` first, in the same pull request.

### API changes

A change to the API in `nh-proto` has three generated outputs. Make all three in the same commit:

1. Run `mise run gen:api`. It writes `packages/transport/src/generated.ts`.
2. Accept the insta snapshot: `INSTA_UPDATE=always bash scripts/with-test-env.sh cargo nextest run -p nh-proto --features api`. The schema tests of `nh-proto` run only with `--features api`.
3. Run `bun run gen` in `website/`.

Read the diff of the snapshot before you commit it. `mise run check` or `mise run test` fails when one of the three outputs is out of date.

### Documentation

- The documentation site is in `website/`, with its own `bun install`. [website/README.md](website/README.md) has its commands and the rules for a page.
- Pages with a "Generated page" note come from `bun run gen` in `website/`. Never edit them by hand. Change the source and run `bun run gen` again. `mise run check:docs` fails when a generated page is out of date.
- If your change makes a page of the guide wrong, fix the page in the same pull request.
- Write `README.md`, `CONTRIBUTING.md`, `AGENTS.md` and the guide pages in [Simplified Technical English](https://www.asd-ste100.org) (STE). Write `neohar` in lowercase, also at the start of a sentence.
- The most important rules of STE are these. Write one topic in each sentence. Use at most 20 words in an instruction and 25 words in a description. Use the active voice, and use one word for one meaning. Use simple words, for example "use" and not "utilize", and "start" and not "initiate". Keep code names, commands and error messages exactly as they are. Do not worry if your text is not perfect STE: a reviewer can help.

## Tests

`mise run check` runs every static check. `mise run test` runs every test that needs no live harness, no GPU, no browser and no second machine. [Testing overview](website/docs/testing/overview.md) tells what each one runs.

### Rules for every test

- **Temporary directories.** Every test sets `NEOHAR_CONFIG_DIR`, `NEOHAR_DATA_DIR` and `NEOHAR_RUNTIME_DIR` to temporary directories. `scripts/with-test-env.sh` does this, and it also sets `CLAUDE_CONFIG_DIR` and `CODEX_HOME`. Harness tests also set `HOME`. The tests do not change `PATH`. Thus the core of a test can run the installed `claude` or `codex` to read its version or login state.
- **Loopback only.** No test uses the network outside `127.0.0.1`. The tests over real ssh are the only exception, and only `mise run test:remote` turns them on.
- **Holder guards.** A test that starts holders kills them on drop.
- **Gates, not skips.** Some tests need a live harness, a GPU or an installed program. Such a test returns early and says why, unless its variable is set: `NEOHAR_APPS`, `NEOHAR_HARNESS`, `NEOHAR_GPU`, `NEOHAR_SWEEP` or `NEOHAR_REMOTE`. Its `mise` task sets the variable.
- **No `#[ignore]` or `.skip` to make a check pass.**
- **No fixed sleeps.** A test waits for events with a deadline.

Run one part of the tests with the same environment:

```bash
bash scripts/with-test-env.sh cargo nextest run -p nh --test core_restart
```

### Local suites

These suites are not part of `mise run test`, and CI does not run them. Run the suites that cover your change. [Local suites](website/docs/testing/local-suites.md) tells what each suite proves.

| Suite | Command | It needs |
|---|---|---|
| The UI shell against a headless core | `mise run test:shell`, `test:shell:link`, `test:transport` | `node` and Playwright WebKit (`mise run setup:browsers`) |
| The phone app | `mise run test:phone` | `node` and Playwright WebKit |
| The sandbox of the webview | `mise run test:host` | A display: it opens a window |
| Renderers in browsers | `mise run test:gpu` | A GPU, `node`, WebKit and Chromium |
| The keyboard matrix | `mise run test:keyboard` | `node` and WebKit |
| `less`, `vim` and `htop` in panes | `mise run test:apps` | The three programs |
| Restart under `SIGKILL` | `nh-chaos run` (after `mise run build:chaos`) | A release build, about 6 minutes |
| Budgets | `mise run bench` | A quiet machine. Put the numbers in the commit message |
| Live Claude Code and Codex | `mise run test:harness` | macOS, and both harnesses on the `PATH` |
| A second machine over ssh | `mise run test:remote` | A second machine with the same system and CPU type, reachable over ssh. Set `NEOHAR_REMOTE_TARGET` to its ssh host name, and run `mise run remote:deploy` first |

## Report a bug

Open an issue with the bug report template. Give these items:

- The version of macOS, the version of neohar (`nh --version`) and the versions of the harnesses (`claude --version`, `codex --version`).
- The output of `nh doctor`. It works also when neohar does not run.
- The steps that cause the problem, what you expected, and what occurred.

`nh debug bundle` writes a zip with the logs, the doctor, the configuration and the recent events. It removes the secrets that it can find, but it can miss one. Read the files before you attach the zip. Do not attach it to a public issue if it contains private data.

Report a vulnerability privately, as [SECURITY.md](SECURITY.md) tells. Do not open a public issue for it.

## License

neohar is licensed under the [Apache License 2.0](LICENSE). Your contribution is licensed under the same license, as section 5 of the license tells.
