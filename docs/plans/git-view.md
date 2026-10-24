# neohar Git view spec

Oct 23, 2026 · @Nikodem Wrona

Copied from the Claude Doc [neohar Git view spec](https://claude.ai/artifact/WhxgyRCaRQhrqhn48dDTt2) (revision 19).

neohar gets a Git view: a full-size view of its own, not a drawer tab and not a pane, that does what GitHub Desktop does for the repositories on this machine (switch repository, worktree and branch, fetch, preview changes, read history) and also knows which agents work in each worktree.

## Scope

v1 reads git and switches it: pick a repository, a worktree and a branch, fetch and pull, preview changes, read history. Commit and push come in phase 2.

| Area | v1 | Phase 2 and later | Out of scope |
| --- | --- | --- | --- |
| Repositories | Repos that neohar finds (workspace roots, pane folders) and repos you add | Remove, reorder, pin | Clone, GitHub account sign-in |
| Worktrees | List them, show one, open a pane in one | Add, remove, prune by hand | A worktree made for each agent |
| Branches | List local and remote, switch, show ahead and behind | New, rename, delete, compare two branches |  |
| Remote | Fetch (button and background), pull with fast-forward only | Push, publish a branch, pull with merge or rebase | Force push |
| Changes | Changed files and the diff of each, from `HEAD` | Stage files, hunks and lines; commit with summary and description; discard; stash | Text editing in the view |
| History | Commits of the branch, the files and diff of one commit | Graph lanes, search, revert, cherry-pick | Interactive rebase, conflict editor |
| GitHub |  | PR and CI state of a branch through `gh` | Issues, review threads |

This is not the removed task workspaces. Those made a worktree for each agent and bound the agent to it. The Git view binds nothing: it shows git as it is, and each agent keeps its own folder.

The Changes tab of the drawer stays as the quick look at the focused pane's worktree. It gets one link: "Open in Git view".

## Where it lives

The Git view takes the middle of the window, where the tab bar and the panes are. The command line, the board and the status bar stay, so an agent that waits stays in view.

![neohar window · before and after the Git view opens](git-view/window.svg)

- **Open and close.** ⌘⇧G, a Git key in the status bar, "Git" in the palette, or "Show in Git view" on a board strip. Esc or ⌘⇧G goes back to the same tab.
- **Full width.** The left column and the drawer fold to their rails while the view shows. They open again when it closes.
- **Panes keep running.** The window reports no visible panes (`window.set_visible_panes`), so the core sends no frames until the view closes. The holders and agents do not notice.
- **A mode of the window, not a tab.** The layout code expects a PTY in each tab, so the view is window state in `app/view.ts`, next to the drawer state. It holds no business state.
- **Own window in phase 2.** "Open in new window" shows the same view in a native window.

## Look

The view keeps the GitHub Desktop layout from your screenshot and draws it in neohar's own look: the glass panels, the sans font for names, the terminal font for paths and code.

![Git view mock-up · sample data](git-view/mockup.svg)

- **Top bar.** Repository, Worktree, Branch, then Fetch. Each menu has a filter field and keys (type to filter, ↑ ↓, Enter). Fetch becomes "Pull origin 3↓" when the branch is behind.
- **Left list.** Changes or History. A History row has the subject, author, time text and short hash, labels for branches and tags, and a mark on commits that are not pushed.
- **Diff.** Unified, with old and new line numbers, intra-line marks and the highlight from `nh-rich`, as the Changes tab draws it today. In History, a click on a commit shows its message and files above the diff.
- **Commit box.** Phase 2 puts it under the list, as in your screenshot.

## Flows

Each action is one git command that a click starts. The core never retries a write, and every write goes to the timeline with its command and result.

| Flow | You do | Core runs | Guard |
| --- | --- | --- | --- |
| Open | ⌘⇧G, the Git key in the status bar, or "Git" in the palette | `rev-parse --show-toplevel` on the focused pane's folder | Opens on that worktree, else the workspace root, else the last one in this window |
| Switch repository | Pick in the Repository menu; type to filter | `rev-parse --git-common-dir` groups worktrees into one repo | None: it changes only what the view shows |
| Switch worktree | Pick in the Worktree menu; "Open a pane here" on a row | `worktree list --porcelain` | None for the view; the new pane starts in that folder |
| Switch branch | Pick in the Branch menu: recent, local, remote | `switch <b>`, or `switch --track origin/<b>` | Agent guard (next section); dirty tree asks "Bring my changes" or "Leave them on \<a>" (stash); a branch out in another worktree offers that worktree instead |
| Fetch | Click "Fetch origin"; it also runs in the background | `fetch --prune <upstream remote>` | Never touches the worktree or the index, so no agent guard |
| Pull | The button becomes "Pull origin 3↓" when behind | `merge --ff-only @{u}` | Agent guard; diverged shows "2↑ 3↓" and offers no merge in v1 |
| Preview changes | Click a file in Changes | `diff-index` + one file diff, as the Changes tab | Read-only; never writes the index |
| Read history | Click a commit in History; scroll for more | `log` in pages of 200, `show` for one commit | Read-only |

"Last fetched 18 minutes ago" comes from the time of `FETCH_HEAD`. Ahead and behind come from `rev-list --left-right --count @{u}...HEAD`.

With "Leave my changes on \<a>", the stash carries the branch name. When you switch back to that branch, the view offers to restore it, as GitHub Desktop does.

## Agents and git

The view knows which agents work in each worktree, and it never changes the files under a working agent without a question. GitHub Desktop cannot do this; neohar already has the facts.

The core maps each pane to a worktree: the pane's live folder (`panes.cwd`, which the pane watcher reads once a second) goes to the worktree root that holds it. The agent of that pane gives the state (`facets.execution`, `facets.pending`). Worktrees that Claude Code makes under `.claude/worktrees/` show up like any other, with their agent.

Where the view shows it:

- The Worktree menu: each row has its agents, for example "Claude · working" and "Codex · idle".
- The top bar: "2 agents in this worktree" next to the branch. A click shows the first pane.
- The board: a strip gets "Show in Git view", which opens the view on that agent's worktree.

The guard applies to each write that changes files in a worktree: switch, pull, and in phase 2 discard and stash.

| Agents in that worktree | What happens |
| --- | --- |
| None | The command runs |
| Idle or exited | The command runs; the timeline of the agent notes the switch |
| Starting, working, or with an open approval or question | A dialog names the agents: "Claude is working in neohar-git. A switch changes its files." Buttons: Switch anyway, Cancel |
| State unknown | Same as working |

The core decides the guard, not the page. It uses the pattern of `workspace.close`: a write without `force: true` answers `confirm_required` with the list of agents. The page shows the dialog and calls again with `force: true`. The CLI passes `--force`.

## Core design

The core gets a `git` service that grows out of `changes.rs` and `changes/git.rs`. It runs the `git` program exactly as the Changes tab does today. The page only paints what the service sends.

- **The git program, no library.** The user's `PATH`, config, credential helpers and hooks apply. Reads keep `GIT_OPTIONAL_LOCKS=0` and use plumbing (`diff-index`, `for-each-ref`, `log`) that never writes the index, so an agent's commit never fails on `index.lock`. No libgit2 or gix: the Tauri build stays free of new native code.
- **The repository list.** A repository is one git common dir. The core finds them from workspace roots and live pane folders, and adds the ones you add by hand (one new store table).
- **Follow, as the Changes tab does.** The page calls `git.follow {window, worktree}`. The core fills the Git projections of that window only while it follows. It watches the worktree and its git dir, so a commit or a switch that an agent makes in its pane shows at once.
- **One write at a time.** The core holds a lock for each worktree while a write runs. A second write answers `busy`. Fetch and pull run as jobs with progress, and the head row shows it.
- **Credentials.** A fetch from the button sets `GIT_ASKPASS` and `SSH_ASKPASS` to `nh askpass`, so a passphrase or token question shows the sheet that ssh links already use. A background fetch never asks: it stops, and the button says "Fetch needs your passphrase".
- **Background fetch.** Every `git.fetch_minutes` (default 10, 0 turns it off), only for repositories that a window follows.
- **Events.** Each write appends one event (`git.switched`, `git.pulled`, `git.fetched`) in the same transaction, so the timeline and the audit show it.
- **Remote machines.** The core of the workspace's machine runs git, as for Files and Changes. A Mac window shows a repository on a remote machine with no extra code.
- **The diff.** `nh-rich` makes a `rich::FileDiff` with highlights and intra-line marks, as today. The view asks for up to 20,000 lines (the drawer stops at 2,000) and paints only the lines on screen.

Budgets to hold, measured on the neohar repository:

| Step | Target |
| --- | --- |
| Open the view: head, worktrees, branches | 150 ms |
| Changes count | 100 ms (the Changes tab measures 60–100 ms) |
| First 200 commits of History | 100 ms |
| One file diff | 50 ms |
| View updates after a switch | 300 ms after git ends |
| Work while no window follows | No git commands, no watches |

## API and CLI

v1 adds 9 methods and 5 projections to `nh-proto`, all keyed by window like the Changes tab. The UI and the CLI call the same methods; a pane token calls none of them.

| Method | Params | Answer | Errors |
| --- | --- | --- | --- |
| `git.follow` | `window`, `worktree` | Empty | `not_found` |
| `git.repo_add` | `path` | Empty | `not_found` (not in a git repo) |
| `git.switch` | `window`, `branch`, `track?`, `changes: bring \| leave`, `force` | Empty | `confirm_required`, `conflict`, `busy`, `git_failed` |
| `git.unstash` | `window`, `stash` | Empty | `conflict`, `busy` |
| `git.fetch` | `window` | `JobStarted` | `busy`, `git_failed` |
| `git.pull` | `window`, `force` | `JobStarted` | `confirm_required`, `conflict` (diverged), `busy` |
| `git.log` | `window`, `after?`, `limit` (≤ 200) | Commits + `more` |  |
| `git.show` | `window`, `commit` | Message, author, time text, parents, file rows | `not_found` |
| `git.diff` | `window`, `path`, `commit?` | `rich::FileDiff` | `too_large` |

| Projection | Kind | What it holds |
| --- | --- | --- |
| `git_repos` | Rows | Name, common dir, machine, where it came from, agent count |
| `git_head:<window>` | Singleton | Repository, worktree, branch, upstream, ahead, behind, last fetch time and text, job, stash of this branch, agents here |
| `git_worktrees:<window>` | Rows | Path, branch, dirty count, locked, prunable, agents with harness and state |
| `git_branches:<window>` | Rows | Name, local or remote, ahead, behind, last commit time text, worktree that has it out; recent first |
| `git_changes:<window>` | Rows | `ChangeRow`, the row type of the Changes tab |

"Open a pane here" uses `tab.create`, whose `ProcessSpec` already has `cwd`.

The CLI gets `nh git repos | worktrees | branches | status | switch <branch> [--force] | fetch | pull | log [-n N]`. Each prints the same rows as the projections.

As for every API change: `mise run gen:api`, the insta snapshot of `nh-proto`, and `bun run gen` in `website/`, in the same commit.

## Phases

v1 ships in two steps: v1a reads, v1b switches and fetches. v1a adds no write method, so it cannot touch an agent's worktree.

![Git view roadmap · 4 phases, 2 gates](git-view/roadmap.svg)

Each step is a `feat` commit that passes `mise run check` and `mise run test`. The guard gate needs a local test with a live Claude pane, gated by an environment variable, as the other harness suites are.

## Prior art

The Git view takes its layout from GitHub Desktop and its worktree habits from Fork and lazygit. None of them knows which agent works in a worktree; that is the part neohar adds.

| Tool | What it does | Take | Skip |
| --- | --- | --- | --- |
| [GitHub Desktop 3.6](https://github.blog/changelog/2026-06-26-github-desktop-3-6-worktrees-and-deeper-copilot-integration/) (June 2026) | Current Repository, Current Worktree and Current Branch menus in the toolbar; New Worktree; pitched for parallel agent sessions | The three menus and the Fetch button as the top bar; Changes and History tabs on the left, diff on the right | Copilot commit messages and AI conflict help: the agents in neohar's panes do that |
| [GitHub Desktop stash](https://docs.github.com/en/enterprise-server@3.22/desktop/making-changes-in-a-branch/stashing-changes-in-github-desktop) | A switch with changes asks: stash, or bring them; one stash, tied to its branch; Restore from "Stashed Changes" | The bring or leave choice and the stash tied to a branch | The limit of one stash: neohar keeps one for each branch |
| [Fork](https://fork.dev/releasenotes) | Worktrees in the sidebar (2.31, July 2023); create and delete (2.41, March 2024); an icon on branches out in another worktree (2.59, November 2025); remote branch as a worktree (2.68, June 2026) | The worktree mark in the Branch menu | Tabs for each worktree: neohar has one view that switches |
| [lazygit](https://github.com/jesseduffield/lazygit/discussions/2803) | Space on a worktree switches into it; git refuses a branch that another worktree has out | Keys for every menu; offer the other worktree when git refuses | Changing the shell's folder on quit |
| Sublime Merge (from memory, not checked) | Every action in a command palette; unified or split diff | Every Git action in the neohar palette; split diff in phase 2 |  |

## Open questions

Each question has the default this spec takes. Change any of them and the spec follows.

1. **Same window or own window?** Default: the view takes the middle of the window, and "Open in new window" comes in phase 2.
2. **Commit in v1?** Your screenshot has the commit box. Default: phase 2, because v1 has no staging.
3. **The drawer's Changes tab.** Default: keep it as the quick look, with "Open in Git view". Or remove it when the view ships.
4. **Background fetch.** Default: every 10 minutes for followed repositories. Or off until you turn it on.
5. **Which repositories show.** Default: found from workspaces and panes, plus the ones you add. Or also scan a folder such as `~/Development`.
6. **"Switch anyway" under a working agent.** Default: warn only. Or interrupt the agent first (`agent.interrupt`), then switch.
7. **Phone.** Default: no Git view on the phone in v1; the methods work, the layout waits.
8. **Pull when diverged (phase 2).** Default: follow the repository's `pull.rebase` setting.

## Sources

- [GitHub Desktop 3.6: worktrees and deeper Copilot integration](https://github.blog/changelog/2026-06-26-github-desktop-3-6-worktrees-and-deeper-copilot-integration/), GitHub changelog, 2026-06-26
- [Stashing changes in GitHub Desktop](https://docs.github.com/en/enterprise-server@3.22/desktop/making-changes-in-a-branch/stashing-changes-in-github-desktop), GitHub Docs
- [Fork release notes](https://fork.dev/releasenotes), fork.dev
- [Worktree UX, discussion #2803](https://github.com/jesseduffield/lazygit/discussions/2803), lazygit on GitHub
- neohar code on `main` at 0d22ecc: `nh-core/src/changes.rs`, `changes/git.rs`, `askpass.rs`, `watch.rs`, `nh-proto/src/api/{methods,model,rich,envelope}.rs`, `packages/ui/src/shell/Shell.tsx`
