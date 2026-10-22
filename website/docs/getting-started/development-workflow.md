---
title: Development workflow
description: "How a change gets into neohar: a pull request from a fork, the checks before it, the conventions, the writing rules, and the commands of this site."
sidebar_position: 2
---

[CONTRIBUTING.md](https://github.com/nikodem-wrona/neohar/blob/main/CONTRIBUTING.md) at the root of
the repository has the full rules. [AGENTS.md](https://github.com/nikodem-wrona/neohar/blob/main/AGENTS.md)
has the same rules in a short form for coding agents. This page gives the steps.

## One change

1. Fork the repository, and make a branch from `main` in your fork.
2. Make the change, with its tests.
3. Fix each guide page that the change makes wrong, in the same change.
4. Run `mise run check` and `mise run test`. Both must pass.
5. Run the [local suites](/docs/testing/local-suites) that cover the change. Put their results in
   the pull request.
6. Open a pull request against `main`. The maintainer reviews it.

The CI workflow runs `mise run check` and `mise run test` on macOS for each pull request and each
push to `main`. It does not run the local suites.

## Rules

- **Build what the change needs.** No refactors outside it, no placeholder code, no new dependency
  without a reason.
- **A budget is met only when it is measured** on the target hardware, and the numbers are in the
  commit message.
- **A behavior of a harness is verified only** by a recording in `tests/harness-golden/` or a test
  against a live binary.
- **No `#[ignore]` or `.skip` to make a check pass.** An environment variable gates each test
  that needs a rare resource, for example a live harness.
- **Tests use temporary directories.** See [Rules for every test](/docs/testing/overview#rules-for-every-test).

## Code conventions

- **Rust**: edition 2024. `thiserror` in libraries, `anyhow` only in binaries and tools. No
  `unwrap()` outside tests. `tracing` everywhere except `nh-hold` and the hook shim. A `// SAFETY:`
  comment on every `unsafe` block. Wire types live only in `nh-proto`, and the `holder` and `frame`
  modules change only by appends.
- **TypeScript**: strict, with `noUncheckedIndexedAccess`. `packages/ui` imports only
  `packages/transport`, `packages/proto-wasm` and `solid-js`. ESLint enforces the thin-presentation
  rule, and pure view mappers in `model.ts` files are permitted ([Desktop app](/docs/desktop/overview)).
- **Tests**: `cargo nextest`, `proptest`, `insta`. Integration tests that need the `nh` binary are in
  `crates/nh/tests/`. A Playwright spec ends in `.pw.ts`, so `bun test` does not run it.

## Writing

`README.md`, `CONTRIBUTING.md`, `AGENTS.md`, the guide pages, the pull request text and the commit
messages use [Simplified Technical English](https://www.asd-ste100.org) (STE).
[CONTRIBUTING.md](https://github.com/nikodem-wrona/neohar/blob/main/CONTRIBUTING.md#documentation)
gives its most important rules.

## This site

The site is a Docusaurus project in `website/`, outside the Bun workspace. Run these from `website/`:

```bash
bun install           # once
bun run gen           # write the generated pages again: API, CLI, configuration defaults, mise tasks
bun run start         # a server with live reload on http://localhost:3000
bun run check         # front matter, hidden HTML tags, GitHub paths, unknown methods and tasks
bun run build:strict  # the static site; a broken link fails the build
```

Never edit a page that says "Generated page": change its source and run `bun run gen`. The CLI pages
need a built `nh`. Without one, `bun run gen` keeps the pages that are there.
[`website/README.md`](https://github.com/nikodem-wrona/neohar/blob/main/website/README.md) has the
rules for a page.

`mise run check:docs` (part of `mise run check`) fails when a generated page is not what
`bun run gen` writes. Then it runs `bun run check`. It writes nothing.
