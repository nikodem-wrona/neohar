# The neohar documentation site

This directory is a [Docusaurus](https://docusaurus.io/) 3 site. Its pages are in `website/docs/`
and are served under `/docs/…`: the guide (architecture, agents, the desktop app, testing, the code)
and the generated references.

When a page of the guide and the code do not agree, the page is the one to fix.

## Commands

Run them from this directory. The site is not part of the Bun workspace of the repository, so it
has its own `node_modules` and lock file.

```bash
bun install          # the dependencies of the site
bun run gen          # write the generated pages again (see below)
bun run start        # a development server on http://localhost:3000/neohar/ with live reload
bun run build        # the static site in build/
bun run serve        # serve build/ locally
bun run build:strict # the same, and fail on a broken link
bun run check        # check the pages: front matter, hidden HTML tags, GitHub paths, names
```

`bun run check` fails on a fault: front matter that is not valid YAML (quote a value that has
`: `), a `<` before a letter outside code (CommonMark reads it as an HTML tag and hides it), a
GitHub link to a path that the repository does not have, or `Neohar` with a capital letter. It also prints warnings for API methods, `nh` commands and
`mise` tasks that the repository does not have; a page may name a planned one, so read the line.

The site is for GitHub Pages at `https://nikodem-wrona.github.io/neohar/`, so its `baseUrl` is
`/neohar/`. To serve it from a different place, set `DOCS_URL` (the `url`) and `DOCS_BASE_URL`
(the `baseUrl`).

## Generated pages

`bun run gen` runs `scripts/gen-all.ts`. It reads files of the repository and writes pages under
`docs/`. Do not edit the generated pages by hand: change the source, then run `bun run gen`.

| Generator | Source | Pages |
|---|---|---|
| `scripts/gen-api.ts` | `crates/nh-proto/tests/snapshots/api_schema__contract.snap` (the snapshot of `nh api schema`) and the doc table of `crates/nh-core/src/debug_calls.rs` for the methods outside the contract | `docs/reference/api/methods/*`, `index.md`, `projections.md`, `events.md`, `streams.md`, `errors.md`, `types.md` |
| `scripts/gen-config.ts` | `crates/nh-core/src/config/defaults.toml` | `docs/reference/configuration/defaults.md` |
| `scripts/gen-cli.ts` | `nh --help` of every command (needs `target/debug/nh` or `target/release/nh`) | `docs/reference/cli/commands/*` |
| `scripts/gen-tasks.ts` | `mise.toml` | `docs/getting-started/mise-tasks.md` |

The CLI generator skips its pages when no `nh` binary is built, and keeps the pages that are there.

`mise run check:docs` at the root of the repository runs the generators with `--check`
(`bun scripts/gen-all.ts --check`): they write nothing and fail when a file is not what they would
write. Then it runs `bun run check`. Neither needs `node_modules`. With `--check`, the CLI generator
also skips its pages when the `nh` binary is older than `crates/nh/src`.

## Writing a page

- Keep the guide short: one page per subject, and the facts a person needs.
- Write `.md` files. With `markdown.format: 'detect'`, they are CommonMark, so `<`, `{` and `}` in
  text need no escape. Use `.mdx` only for a page that needs a React component.
- Every page has front matter with `title`, `description` and `sidebar_position`.
- Write `neohar` in lowercase, also at the start of a sentence.
- Use `they` for a person whose pronouns you do not know.
- Link to other pages of the guide by route, for example `/docs/architecture/holders`.
- Link to code with a full GitHub URL:
  `https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-hold/src/journal.rs`.
- Use admonitions (`:::note`, `:::tip`, `:::warning`, `:::danger`, `:::info`) and Mermaid code
  blocks (`` ```mermaid ``) where they help. Do not use raw HTML.
- Say whether a feature is built, partly built or not built yet.
