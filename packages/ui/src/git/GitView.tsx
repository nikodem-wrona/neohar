// The Git view: a full view in the middle of the window, over the tab bar and the panes, that does
// for the git repositories of this machine what GitHub Desktop does, and also shows the agents
// that work in each worktree. The top bar has the Repository, the Worktree and the Branch menus;
// the left side has the changes or the history, and the right side the diff.
//
// The core finds the worktree of the folder that the page names (`git.follow`), reads git, and
// fills the projections of this window while the view follows them: `git_head:<window>`,
// `git_worktrees:<window>`, `git_branches:<window>` and `git_changes:<window>`, with `git_repos`
// for the Repository menu. The panes keep running under the view: the window tells the core that
// it draws no pane, so the core sends no frame until the view closes.

import { type ChangeRow, type GitHead } from '@neohar/transport';
import { createEffect, createMemo, createSignal, on, onCleanup, onMount, Show } from 'solid-js';
import { useApp } from '../app/context';
import { showPane } from '../app/show';
import { windowFolder } from '../files/folder';
import { createProjectionStore } from '../stores';
import { GitChanges } from './GitChanges';
import { GitHistory } from './GitHistory';
import { GitMenu, type MenuGroup } from './Menu';
import {
  agentsLabel,
  agentsText,
  branchFacts,
  branchGroups,
  branchTitle,
  isAbsolutePath,
  repoFacts,
  trackText,
  worktreeFacts,
} from './model';

type Side = 'changes' | 'history';

export function GitView() {
  const { stores, transport, run, report, windowId, view } = useApp();
  const head = createProjectionStore(transport, `git_head:${windowId}`);
  const worktrees = createProjectionStore(transport, `git_worktrees:${windowId}`);
  const branches = createProjectionStore(transport, `git_branches:${windowId}`);
  const changes = createProjectionStore(transport, `git_changes:${windowId}`);
  const repos = createProjectionStore(transport, 'git_repos');
  onCleanup(() => {
    for (const store of [head, worktrees, branches, changes, repos]) store.close();
  });
  const shown = (): GitHead | undefined => head.state.byKey['_'];
  const [side, setSide] = createSignal<Side>('changes');
  let root: HTMLElement | undefined;

  const follow = (path: string): void => {
    run('git.follow', { window: windowId, path });
  };
  // The view shows the worktree of the folder that the request names, else the folder of the
  // window. Without either, the core keeps the folder that this window named last.
  createEffect(
    on(view.gitTarget, (target) => {
      const folder = target.folder ?? windowFolder(stores, windowId);
      if (folder) follow(folder);
    }),
  );
  onMount(() => root?.focus());

  const subject = createMemo(() => shown()?.worktree ?? null);
  const rows = (): ChangeRow[] => (subject() ? changes.rows() : []);

  const repoGroups = (): MenuGroup[] => [
    {
      items: repos.rows().map((repo) => ({
        key: repo.root,
        title: repo.name,
        detail: repo.root,
        agents: repoFacts(repo) || null,
        current: repo.common_dir === shown()?.repo,
      })),
    },
  ];
  const worktreeGroups = (): MenuGroup[] => [
    {
      items: worktrees.rows().map((row) => ({
        key: row.path,
        title: row.name,
        detail: worktreeFacts(row),
        agents: agentsLabel(row.agents) ?? 'no agent',
        current: row.current === true,
        disabled: row.prunable === true,
      })),
    },
  ];
  const branchMenu = (): MenuGroup[] =>
    branchGroups(branches.rows()).map((group) => ({
      title: group.title,
      items: group.rows.map((row) => ({
        key: row.reference,
        title: row.name,
        detail: branchFacts(row),
        current: row.current === true,
        // A branch that another worktree has out shows that worktree.
        disabled: !row.current && !row.worktree,
      })),
    }));

  const pickBranch = (reference: string): void => {
    const row = branches.state.byKey[reference];
    if (row?.worktree) follow(row.worktree);
  };

  const openPane = (): void => {
    const folder = shown()?.worktree;
    const workspace = stores.windows.state.byKey[windowId]?.workspace;
    if (!folder || !workspace) return;
    transport
      .call('tab.create', { workspace, command: { argv: [], cwd: folder } })
      .then((created) => transport.call('tab.select', { tab: created.tab, window: windowId }))
      .then(() => view.closeGit())
      .catch(report);
  };

  const showFirstAgent = (): void => {
    const pane = shown()?.agents?.[0]?.pane;
    if (!pane) return;
    showPane(transport, windowId, pane)
      .then(() => view.closeGit())
      .catch(report);
  };

  const onKey = (event: KeyboardEvent): void => {
    if (event.key !== 'Escape' || event.defaultPrevented) return;
    event.preventDefault();
    view.closeGit();
  };

  return (
    <section
      class="git-view"
      aria-label="Git view"
      tabIndex={-1}
      ref={(element) => (root = element)}
      onKeyDown={onKey}
    >
      <header class="git-bar">
        <GitMenu
          label="Repository"
          value={shown()?.repo_name ?? 'No repository'}
          groups={repoGroups()}
          onPick={follow}
          actions={(filter) =>
            isAbsolutePath(filter)
              ? [
                  {
                    label: `Add ${filter.trim()}`,
                    run: () => {
                      const path = filter.trim();
                      transport
                        .call('git.repo_add', { path })
                        .then(() => follow(path))
                        .catch(report);
                    },
                  },
                ]
              : []
          }
          empty="No repository matches. Type the absolute path of a folder to add its repository."
        />
        <GitMenu
          label="Worktree"
          value={shown()?.worktree_name ?? '—'}
          note={agentsLabel(shown()?.agents)}
          groups={worktreeGroups()}
          onPick={follow}
          actions={() =>
            shown()?.worktree ? [{ label: 'Open a pane in this worktree', run: openPane }] : []
          }
          empty="No worktree."
        />
        <GitMenu
          label="Branch"
          value={shown() ? branchTitle(shown() as GitHead) : '—'}
          note={trackText(shown()?.ahead, shown()?.behind) || null}
          groups={branchMenu()}
          onPick={pickBranch}
          empty="No branch matches."
        />
        <Show when={agentsText(shown()?.agents)}>
          {(text) => (
            <button
              type="button"
              class="git-agents"
              title="Show the pane of the first agent"
              onClick={showFirstAgent}
            >
              {text()}
            </button>
          )}
        </Show>
        <span class="git-spacer" />
        <Show when={shown()?.fetched_text}>
          {(text) => <span class="git-fetched">{text()}</span>}
        </Show>
        <button
          type="button"
          class="git-close"
          title="Close the Git view (Esc)"
          onClick={() => view.closeGit()}
        >
          Close
        </button>
      </header>
      <Show when={shown()?.error}>
        {(error) => (
          <p class="git-error git-banner" role="alert">
            {error()}
          </p>
        )}
      </Show>
      <Show
        when={shown()}
        fallback={<p class="git-empty git-center">Looking for the repository…</p>}
      >
        {(current) => (
          <Show
            when={current().worktree}
            fallback={
              <p class="git-empty git-center">
                The folder {current().folder} is not in a git repository. Pick one in the Repository
                menu.
              </p>
            }
          >
            <div class="git-body">
              <div class="git-tabs" role="tablist" aria-label="Changes or history">
                <button
                  type="button"
                  role="tab"
                  class="git-tab"
                  classList={{ selected: side() === 'changes' }}
                  aria-selected={side() === 'changes'}
                  onClick={() => setSide('changes')}
                >
                  Changes
                  <Show when={current().changes}>
                    {(totals) => <span class="git-count">{totals().files}</span>}
                  </Show>
                </button>
                <button
                  type="button"
                  role="tab"
                  class="git-tab"
                  classList={{ selected: side() === 'history' }}
                  aria-selected={side() === 'history'}
                  onClick={() => setSide('history')}
                >
                  History
                </button>
              </div>
              <Show when={side() === 'changes'}>
                <GitChanges
                  rows={rows()}
                  subject={subject()}
                  truncated={current().changes?.truncated ?? false}
                  counted={current().changes !== undefined}
                />
              </Show>
              <Show when={side() === 'history'}>
                <GitHistory head={current().commit ?? null} subject={subject()} />
              </Show>
            </div>
          </Show>
        )}
      </Show>
    </section>
  );
}
