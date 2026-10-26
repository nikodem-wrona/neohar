# CLAUDE.md

@AGENTS.md

## Skills of Claude Code

- `/install-app` (optional, macOS only) builds `neohar.app` from the working tree, installs it in `~/Applications` (the panes continue to run during the update) and installs the hooks of Claude Code and Codex again. It never stops or replaces an `nh daemon`.

## Agents of Claude Code

- `docs-updater` (`.claude/agents/docs-updater.md`) makes the documentation agree with the code. It knows each place of the documentation: `README.md`, `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, `SECURITY.md`, the GitHub templates, `website/`, the READMEs under `tests/` and `.claude/`. It runs the generators, edits the hand-written pages and runs `mise run check:docs`. It does not commit unless you ask.
