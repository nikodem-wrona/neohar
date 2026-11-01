---
name: pr-review
description: Reviews a GitHub pull request of this repository for correctness, security, performance, architecture and adherence to the rules of neohar in AGENTS.md and CONTRIBUTING.md, then posts inline findings and a summary to the PR through the gh CLI, written in Simplified Technical English with concrete examples. Use only when asked to review and publish comments on a pull request. For a local, read-only review of the working tree, use the built-in /code-review instead.
disable-model-invocation: true
argument-hint: "[pr-number | branch] [--dry-run]"
allowed-tools: Read, Grep, Glob, Bash, Agent
---

# Pull request review

Review a GitHub pull request across correctness, security, performance, architecture and adherence to `AGENTS.md` and `CONTRIBUTING.md`, then publish actionable findings to the PR. Ground findings in both the GitHub diff and the checked-out repository context.

Invocation of `/pr-review` authorizes posting the review. If `$ARGUMENTS` contains `--dry-run`, or the user asks for a preview, prepare the review and show it without changing the PR.

## Resolve the target

The target in `$ARGUMENTS` can be a PR number, a branch name, or omitted. When omitted, use the PR for the current branch.

```bash
gh auth status
gh pr view <target> --json number,headRefName,baseRefName,headRefOid,url
```

Omit `<target>` entirely when none was provided. Stop if authentication fails or no PR exists. Do not create a PR unless the user separately asks for one.

## Gather the review scope

Treat GitHub's diff as the source of truth for changed files and inline-comment anchors:

```bash
gh pr diff <number>
gh pr diff <number> --name-only
```

Read `AGENTS.md` and `CONTRIBUTING.md`. The layout and architecture sections of `AGENTS.md` tell which part of the system each changed path belongs to. Inspect the changed symbols, their callers, the tests that cover them, and adjacent configuration in the local checkout. If the checkout does not match the PR head, use Git/GitHub data for that head rather than assuming local files represent the PR.

Review every changed file and report only concrete issues introduced by the PR.

### General dimensions

- **Correctness**: broken contracts or callers, edge cases, error handling, async and concurrency bugs, and missing tests for material behavior.
- **Security**: unsafe file or command handling, missing validation, secret or personal-data exposure, and weakened boundaries.
- **Performance**: unbounded memory or result sets, blocking operations on hot paths, redundant work, and regressions with a plausible impact against the Budgets table of `website/docs/architecture/overview.md`.
- **Architecture**: violations of the rules and fixed choices in `AGENTS.md` and `CONTRIBUTING.md`, avoidable coupling, and changes that make `AGENTS.md`, `CONTRIBUTING.md` or a guide page in `website/docs/` wrong without fixing it in the same PR.

### neohar-specific checks

| Area | Report when the PR |
|---|---|
| Thin presentation | adds business state, a reducer, a derived store, a client-side sort or filter, an optimistic update, polling, or a markdown, diff, highlight or date library to `packages/ui`; uses `innerHTML` outside `RichHtml`; adds a Tauri command other than `rpc`, `subscribe`, `stream` |
| UI contract | gives the UI something the core does not publish as a projection, command or stream; computes "done", attention counts, risk colours or focus in the UI |
| Holder | adds `tokio`, VT parsing or a heavy dependency to `nh-hold`; inserts, reorders or removes a field in an existing holder frame; changes a frame an older peer decodes; does not update the frozen-types compatibility test |
| Durability | sends input without an `input_id`; performs a side effect (bell, clipboard, notification, query reply) while `replaying`; mutates state outside the command bus; writes a state change without its event in the same transaction |
| Untrusted agent output | lets hook payloads, transcripts, tool outputs or plans reach the DOM without `nh-rich` sanitization; lets terminal bytes reach JavaScript |
| Adapters | grants a capability for a version range without a golden case; answers an interaction outside the delivery transaction; uses a suggested binding to authorize an answer, a prompt or a policy decision; lets the shim express a decision through an exit code or print anything other than decision JSON; fails open where it must fail closed |
| Gate and focus | treats a peek, a hover or a sibling-pane focus as pane focus; does not release a held hook on focus; draws a card or dialog over a pane |
| Secrets | logs an environment value; bypasses `nh-redact` in logs, events or debug bundles |
| Scope | adds a harness adapter outside the core, an extension transport, a plugin system or a compatibility layer; writes "Neohar" with a capital letter |
| Budgets | allocates per frame in `packages/ui/surface`; grows a UI bundle past 400 KB gzipped, or the first chunk of the phone past 250 KB; adds a UI dependency beyond `react`, `react-dom/client`, the packages of shadcn/ui (`radix-ui`, `class-variance-authority`, `clsx`, `tailwind-merge`, `lucide-react`, `cmdk`, `sonner`), `packages/transport` and `packages/proto-wasm` |

Validate each finding against surrounding code before reporting it. Do not post style-only comments, speculative concerns without an impact path, or findings unrelated to changed lines. Calibrate severity as:

- `critical`: a fix is necessary before merge, because the finding creates a serious security, data-loss or production-correctness risk, or breaks a durability guarantee of the holders.
- `warning`: a fix is recommended, because the finding causes a concrete bug, a maintainability hazard, a contradiction of `AGENTS.md` or `CONTRIBUTING.md`, or a meaningful performance regression.
- `info`: non-blocking improvement with clear value.

## Write clear review comments

Read and apply the `simplified-technical-english` skill (a skill of the maintainer in `~/.claude/skills`, not in this repository) when you write finding titles, inline comments, general comments and the review summary. Apply its writing rules and checker workflow before publication or preview. Its neohar section lists the technical names and identifiers to keep exact.

For each inline or general finding:

- Start with the specific problem and the condition that causes it. Explain unfamiliar technical terms when the reader needs them to understand the problem. Keep code identifiers, configuration values and error messages exact.
- Include at least one concrete example. State the relevant input or system state, the expected behavior, the actual behavior and the consequence. Use numbered steps when the order matters. Add another example when it explains a different effect or makes a complex issue clearer.
- Distinguish evidence from illustration. Use "I reproduced" only when you performed the reproduction. For an example derived from the code, state that basis. Use "Suppose" for an illustrative scenario, and keep it consistent with the verified code path. Do not invent outcomes or broaden the finding's severity.
- Cite what the finding rests on: a rule of `AGENTS.md` or `CONTRIBUTING.md`, or the code or test that shows the behavior, in the form `path:line`.
- End with a concrete correction or the required behavior. Preserve uncertainty and the strength of the original recommendation.

Keep each comment proportional to the issue. A simple finding can use a short paragraph with one example. Use separate paragraphs and a list for a finding with several dependent steps. The summary can refer to examples in the findings without repeating them. A review with no findings does not require invented examples.

For example, a finding about a resend without reconciliation can use this form:

```markdown
This retry sends the recorded decision again when the delivery state is `delivery_unknown`. It does not reconcile first.

The delivery transaction permits a resend only when reconciliation shows that the native request is still pending.

Suppose this sequence:

1. The user answers "allow" from a notification. The core records the decision and starts delivery.
2. The core restarts before the shim acks. The record is `delivery_unknown`.
3. After the restart, this code resends "allow" as keystrokes to the pane.
4. Meanwhile the user answered "deny" in Claude's own dialog, and Claude started the next prompt.

The keystrokes reach the next prompt as text, and the record says `delivered`.

Call `reconcile` before any resend. If the hook process is gone, keep the record at `delivery_unknown` and show its card text.
```

Before publication or preview, save the final summary, finding titles and finding bodies as prose in a temporary Markdown file. Check that file rather than the JSON payload:

```bash
python3 ~/.claude/skills/simplified-technical-english/scripts/ste_check.py \
  --mode mixed /tmp/pr-review-<number>-prose.md
```

Correct structural errors. Review warnings manually and keep valid technical terms. Check that simpler wording and examples preserve the verified behavior, impact and severity. If the checker is unavailable, use the STE skill's manual-check fallback.

## Prepare findings

Write `/tmp/pr-review-<number>.json` with this shape:

```json
{
  "summary": "Markdown overview of the change, overall quality, strengths, and top actions.",
  "inline": [
    {
      "path": "crates/nh-core/src/delivery.rs",
      "line": 42,
      "side": "RIGHT",
      "severity": "warning",
      "title": "Specific problem",
      "body": "Why this fails, a concrete example with expected and actual behavior, the rule or code it rests on, and a concrete fix."
    }
  ],
  "general": [
    {
      "severity": "warning",
      "title": "Cross-cutting problem",
      "body": "Impact, a concrete example with expected and actual behavior, and a concrete fix."
    }
  ]
}
```

For each inline finding, anchor `line` to a line present in the PR diff. Use `RIGHT` for new-file lines and `LEFT` for deleted old-file lines. For a multi-line range in one hunk, also set `start_line`. Set `start_side` only when it differs from `side`. Put cross-cutting findings and issues without a valid changed-line anchor in `general`.

If there are no findings, keep both arrays empty and summarize why the change appears sound. Never invent a finding to avoid an empty review.

## Publish or preview

Post with the bundled helper:

```bash
python3 .claude/skills/pr-review/scripts/post_review.py \
  --pr <number> \
  --findings /tmp/pr-review-<number>.json
```

Use `--dry-run` when the user asks for a preview or passed `--dry-run`. The helper validates inline anchors, demotes invalid anchors to a general comment, and posts one summary review plus any general comment. It uses `REQUEST_CHANGES` when at least one finding is `critical`. Otherwise it uses `COMMENT`. It never approves. If GitHub disallows requesting changes on the user's own PR, it falls back to `COMMENT` while retaining the verdict in the summary.

Report the severity counts, the event used, the number of inline and general comments, whether this was a dry run, and the PR URL.
