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
6. Push the branch and open a pull request against `main`. Give it a title that is a [commit header](#commit-messages).

The CI workflow runs `mise run check` and `mise run test` on macOS for each pull request. The maintainer reviews each pull request. Keep a pull request to one change. Do not include refactors that the change does not need.

GitHub squashes each pull request into one commit on `main`. The title of the pull request becomes the header of that commit. Thus the commits workflow lints the title as a commit header. The title must also keep the release level of the commits: if one commit is a `feat`, the title is a `feat`, and if one commit is a breaking change, the title has a `!`.

## Commits, versions and releases

### Commit messages

Each commit on `main` is a [Conventional Commit](https://www.conventionalcommits.org/en/v1.0.0/):

```text
<type>(<scope>): <description>

<body>

BREAKING CHANGE: <what does not work now, and what a user must do>
```

- **Header.** Use at most 72 characters. Write the description as an instruction in STE. Start it with a verb in lowercase, for example "add", "fix" or "remove". Do not put a period at the end.
- **Type.** Use a type from the table below. The type sets the next version.
- **Scope.** The scope is optional. Use a scope from `SCOPES` in [scripts/commits.ts](scripts/commits.ts), for example `core`, `ui`, `agents` or `website`. If the change is in many parts, do not give a scope. A new part of the repository gets a new row in `SCOPES`.
- **Body.** Tell why you made the change, and give the evidence. A commit that changes a path with a budget includes the numbers of the budget run.
- **Breaking change.** Put a `!` before the colon, and add a `BREAKING CHANGE:` footer. The footer tells what does not work now and what a user must do. [Versions](#versions) gives the rules for a breaking change.
- **Revert.** Write `revert: <the header of the reverted commit>`, and put `This reverts commit <sha>.` in the body.
- **No merge commits.** `main` has a linear history. Rebase your branch. Squash each `fixup!` commit before you push to `main`.

| Type | Use it for | Release |
|---|---|---|
| `feat` | A new feature for users | minor |
| `fix` | A bug fix for users | patch |
| `perf` | Less time or memory, with the numbers in the body | patch |
| `revert` | The removal of an earlier commit | patch |
| `refactor` | A change of the code that keeps the behavior | none |
| `docs` | Documentation only | none |
| `test` | Tests only | none |
| `build` | The build, the toolchain and the dependencies | none |
| `ci` | The GitHub workflows | none |
| `style` | Formatting only | none |
| `chore` | Other work that changes no file that neohar ships | none |

A `!` makes a major release with each type. Examples:

```text
feat(ui): show the errors of commands in a banner
fix(core): keep the holder of a pane that moves to a new tab
docs(website): serve the site under /neohar/ on GitHub Pages
feat(api)!: remove the method pane.paste_confirm

BREAKING CHANGE: pane.paste_confirm gives the error method_not_found. Send pane.paste.
```

Three checks lint the commits:

- The commit-msg hook in `.githooks/` refuses a message with a fault. `bun install` sets `core.hooksPath` to `.githooks`, so the hook starts to work after your first `bun install`.
- `mise run check:commits` (part of `mise run check`) lints the commits that you did not push.
- The commits workflow lints each push to `main`, and the title and the commits of each pull request.

### Versions

neohar uses [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html): `MAJOR.MINOR.PATCH`. `[workspace.package] version` in `Cargo.toml` is the version. Every `package.json`, `tauri.conf.json`, `Cargo.lock` and `bun.lock` holds the same version. Only `mise run release` changes it. `mise run check:version` fails when a file holds a different version, or when the version is not the version of the last tag.

The public interface of neohar is what a user, a script or an earlier build of neohar uses:

- The `nh` commands, their options and their `--json` output.
- The JSON-RPC API in `nh-proto`: the methods, the fields, the events and the error codes.
- The keys of the configuration file.
- The data of a session. A new build opens the database of an earlier build and talks to its holders.
- The hooks that neohar installs in Claude Code and Codex.
- The link to `nh daemon` on another machine. An app and a remote core with the same major version work together.

If an item of the public interface does not work as before after a change, the change is a breaking change. Examples are a removed method or option, a renamed configuration key, and a schema that an earlier database cannot get. A new item is a feature. A change of the window only is not a breaking change.

### Releases

The maintainer makes the releases. A coding agent makes a release only when the maintainer asks for it.

1. Run `mise run release -- --dry-run`. It shows the next version and its section of `CHANGELOG.md`.
2. Run `mise run release`. It needs a clean `main` that is equal to `origin/main`. It writes the version into each file and the section into `CHANGELOG.md`. Then it makes the commit `chore(release): X.Y.Z` and the annotated tag `vX.Y.Z`.
3. Run `mise run check` and `mise run test`.
4. Push the commit and the tag together: `git push --atomic origin main vX.Y.Z`.

The tag starts `.github/workflows/release.yml`. The workflow makes sure that the tag is on the release commit on `main`. Then it runs the CI workflow on the tag. At the same time, `mise run build:dmg` makes the DMG of the app on macOS. When both pass, the workflow publishes the GitHub release with the section of `CHANGELOG.md` as its text and the DMG as its file. GitHub refuses a change or a deletion of a tag `v*`. If a release has a bug, make a new patch release.

The release script writes the section of `CHANGELOG.md` from the commits since the last tag. The breaking changes come first. Then `feat` goes to "Added", `perf` and `revert` go to "Changed", and `fix` goes to "Fixed". The other types do not show. Text that you write under `## [Unreleased]`, for example a note about an update, goes to the top of the next release.

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

### UI

- The UI uses React 19 and TypeScript, shadcn/ui on Radix and Tailwind CSS 4, Vite and Bun. Do not add `StrictMode`: it runs each effect two times, and a second attach of a pane loses its screen.
- `packages/ui` can import only `react`, `react-dom/client`, the packages of shadcn/ui (see the last item), `@neohar/transport`, `@neohar/proto-wasm`, `@/…` (`packages/ui/src`, as the shadcn/ui CLI writes it) and relative modules. The ESLint rule `neohar/ui-imports` enforces this.
- The rules `react-hooks/rules-of-hooks` and `react-hooks/exhaustive-deps` are errors in `packages/ui`.
- Do not use `onBeforeInput`. React makes it from other events, and it is not the native `beforeinput`. Use `addEventListener('beforeinput', …)`. The ESLint rule `neohar/no-synthetic-beforeinput` enforces this.
- Use [shadcn/ui](https://ui.shadcn.com/) for each component that it has. To add one, run `bunx shadcn@latest add <name>` in `packages/ui`. The command writes it to `packages/ui/src/components/ui/`. Before you add a component, look in `components/ui/` for one that you can use or extend. Do not make a button, a field, a dialog, a menu or another control by hand when shadcn/ui has it. When shadcn/ui has no equivalent, make the component from shadcn components and Radix primitives. Use Tailwind classes and the theme variables of `styles/tokens.css`. You can change a file in `components/ui/` for neohar, but keep its props. A component shows the results and the order that the core gives: do not filter or sort in it (for example, set `shouldFilter={false}` on `Command`). `neohar/ui-imports` allows only the packages of shadcn/ui: `radix-ui`, `class-variance-authority`, `clsx`, `tailwind-merge`, `lucide-react`, `cmdk` and `sonner`. A new package needs a change of that list.
- After `bunx shadcn add`, examine the new file and `packages/ui/package.json`. The CLI can write `import { cn } from "cn"` and add the npm package `cn`: change the import to `@/lib/utils`, and remove the package. The window has a dark theme only, so a component does not use `next-themes`. A shadcn/ui layer (a popover, a menu, a dialog) calls `useLayer(open)` (`app/layer.ts`), so that the pane does not take the focus from it.

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
