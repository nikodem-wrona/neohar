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
//
// A pick in the Branch menu switches the worktree (`git.switch`), and the button after the menus
// fetches or pulls (`git.fetch`, `git.pull`). The core decides when a write must ask first: a
// working agent, changes to bring or to leave, a branch that another worktree has out. The view
// shows its question and calls again with the answer.
//
// The core runs git only in a repository that the person trusts. For another one, `git_head` has
// `trust`, and the view shows the prompt of `TrustPrompt` in place of the changes and the history.

import {
  type ChangeRow,
  type GitHead,
  type Method,
  type Params,
  RpcError,
} from '@neohar/transport';
import { type KeyboardEvent, useEffect, useRef, useState } from 'react';
import { useApp } from '../app/context';
import { showPane } from '../app/show';
import { Button } from '../components/ui/button';
import { windowFolder } from '../files/folder';
import { useRows, useSelect, useValue } from '../lib/hooks';
import { cn } from '../lib/utils';
import { useProjectionStore } from '../stores';
import { type GitAnswer, GitConfirm } from './GitConfirm';
import { GitChanges } from './GitChanges';
import { GitHistory } from './GitHistory';
import { GitMenu, type MenuGroup } from './Menu';
import { TrustPrompt } from './TrustPrompt';
import {
  agentsLabel,
  agentsText,
  branchFacts,
  branchGroups,
  branchTitle,
  fetchButton,
  type GitQuestion,
  isAbsolutePath,
  questionOf,
  repoFacts,
  trackText,
  worktreeFacts,
} from './model';

type Side = 'changes' | 'history';

export function GitView() {
  const { stores, transport, run, report, windowId, view } = useApp();
  const head = useProjectionStore(transport, `git_head:${windowId}`);
  const worktrees = useProjectionStore(transport, `git_worktrees:${windowId}`);
  const branches = useProjectionStore(transport, `git_branches:${windowId}`);
  const changes = useProjectionStore(transport, `git_changes:${windowId}`);
  const repos = useProjectionStore(transport, 'git_repos');
  const shown: GitHead | undefined = useSelect(head, (state) => state.byKey['_']);
  const [side, setSide] = useState<Side>('changes');
  const [asked, setAsked] = useState<{
    question: GitQuestion;
    answers: GitAnswer[];
    /** A new question is a new dialog. */
    seq: number;
  } | null>(null);
  const askSeq = useRef(0);
  const root = useRef<HTMLElement>(null);

  const follow = (path: string): void => {
    run('git.follow', { window: windowId, path });
  };
  // The view shows the worktree of the folder that the request names, else the folder of the
  // window. Without either, the core keeps the folder that this window named last.
  const target = useValue(view.gitTarget);
  useEffect(() => {
    const folder = target.folder ?? windowFolder(stores, windowId);
    if (folder) run('git.follow', { window: windowId, path: folder });
  }, [target, stores, windowId, run]);
  useEffect(() => root.current?.focus(), []);

  const subject = shown?.worktree ?? null;
  const changeRows = useRows(changes);
  const rows: ChangeRow[] = subject ? changeRows : [];
  const repoRows = useRows(repos);
  const worktreeRows = useRows(worktrees);
  const branchRows = useRows(branches);

  const repoGroups: MenuGroup[] = [
    {
      items: repoRows.map((repo) => ({
        key: repo.root,
        title: repo.name,
        detail: repo.root,
        agents: repoFacts(repo) || null,
        current: repo.common_dir === shown?.repo,
      })),
    },
  ];
  const worktreeGroups: MenuGroup[] = [
    {
      items: worktreeRows.map((row) => ({
        key: row.path,
        title: row.name,
        detail: worktreeFacts(row),
        agents: agentsLabel(row.agents) ?? 'no agent',
        current: row.current === true,
        disabled: row.prunable === true,
      })),
    },
  ];
  const branchMenu: MenuGroup[] = branchGroups(branchRows).map((group) => ({
    title: group.title,
    items: group.rows.map((row) => ({
      key: row.reference,
      title: row.name,
      detail: branchFacts(row),
      current: row.current === true,
    })),
  }));

  /**
   * Runs a write. A question of the core shows its dialog, and an answer calls again with the
   * params of the answer added, so a switch that asked about an agent can ask about the changes
   * next.
   */
  const write = <M extends 'git.switch' | 'git.unstash' | 'git.fetch' | 'git.pull'>(
    method: M,
    params: Params<M>,
  ): void => {
    setAsked(null);
    (transport.call as (method: Method, params: unknown) => Promise<unknown>)(method, params).catch(
      (failure: unknown) => {
        const question =
          failure instanceof RpcError
            ? questionOf(failure.code, failure.message, failure.details)
            : null;
        if (!question) {
          report(failure);
          return;
        }
        const again = (extra: Record<string, unknown>) => () =>
          write(method, { ...params, ...extra } as Params<M>);
        const answers: GitAnswer[] =
          question.kind === 'agents'
            ? [
                {
                  label: method === 'git.pull' ? 'Pull anyway' : 'Switch anyway',
                  run: again({ force: true }),
                  danger: true,
                },
              ]
            : question.kind === 'changes'
              ? [
                  { label: 'Bring my changes', run: again({ changes: 'bring' }) },
                  { label: `Leave them on ${question.from}`, run: again({ changes: 'leave' }) },
                ]
              : [
                  {
                    label: 'Show that worktree',
                    run: () => {
                      setAsked(null);
                      follow(question.worktree);
                    },
                  },
                ];
        askSeq.current += 1;
        setAsked({ question, answers, seq: askSeq.current });
      },
    );
  };

  const pickBranch = (reference: string): void => {
    if (branches.get().byKey[reference]?.current) return;
    write('git.switch', { window: windowId, branch: reference });
  };

  const remote = shown ? fetchButton(shown) : null;
  const onRemote = (): void => {
    const button = remote;
    if (button?.kind === 'fetch') write('git.fetch', { window: windowId });
    if (button?.kind === 'pull') write('git.pull', { window: windowId });
  };

  const openPane = (): void => {
    const folder = shown?.worktree;
    const workspace = stores.windows.get().byKey[windowId]?.workspace;
    if (!folder || !workspace) return;
    transport
      .call('tab.create', { workspace, command: { argv: [], cwd: folder } })
      .then((created) => transport.call('tab.select', { tab: created.tab, window: windowId }))
      .then(() => view.closeGit())
      .catch(report);
  };

  const showFirstAgent = (): void => {
    const pane = shown?.agents?.[0]?.pane;
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

  /** The content under the banners: the folder, the trust prompt, or the changes and history. */
  const body = (current: GitHead) => {
    if (!current.worktree) {
      return (
        <p className="git-empty git-center">
          The folder {current.folder} is not in a git repository. Pick one in the Repository menu.
        </p>
      );
    }
    if (current.trust) return <TrustPrompt ask={current.trust} className="git-center" />;
    return (
      <div className="git-body">
        <div className="git-tabs" role="tablist" aria-label="Changes or history">
          <button
            type="button"
            role="tab"
            className={cn('git-tab', { selected: side === 'changes' })}
            aria-selected={side === 'changes'}
            onClick={() => setSide('changes')}
          >
            Changes
            {current.changes ? <span className="git-count">{current.changes.files}</span> : null}
          </button>
          <button
            type="button"
            role="tab"
            className={cn('git-tab', { selected: side === 'history' })}
            aria-selected={side === 'history'}
            onClick={() => setSide('history')}
          >
            History
          </button>
        </div>
        {side === 'changes' ? (
          <GitChanges
            rows={rows}
            subject={subject}
            truncated={current.changes?.truncated ?? false}
            counted={current.changes !== undefined}
          />
        ) : null}
        {side === 'history' ? <GitHistory head={current.commit ?? null} subject={subject} /> : null}
      </div>
    );
  };

  const agents = agentsText(shown?.agents);
  return (
    <section className="git-view" aria-label="Git view" tabIndex={-1} ref={root} onKeyDown={onKey}>
      <header className="git-bar">
        <GitMenu
          label="Repository"
          value={shown?.repo_name ?? 'No repository'}
          groups={repoGroups}
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
          value={shown?.worktree_name ?? '—'}
          note={agentsLabel(shown?.agents)}
          groups={worktreeGroups}
          onPick={follow}
          actions={() =>
            shown?.worktree ? [{ label: 'Open a pane in this worktree', run: openPane }] : []
          }
          empty="No worktree."
        />
        <GitMenu
          label="Branch"
          value={shown ? branchTitle(shown) : '—'}
          note={trackText(shown?.ahead, shown?.behind) || null}
          groups={branchMenu}
          onPick={pickBranch}
          empty="No branch matches."
        />
        {agents ? (
          <Button
            variant="ghost"
            size="xs"
            className="text-[13px] font-normal text-live"
            title="Show the pane of the first agent"
            onClick={showFirstAgent}
          >
            {agents}
          </Button>
        ) : null}
        <span className="git-spacer" />
        {remote ? (
          // Fetch or pull, with the time of the last fetch under it. A pull is the action that goes.
          <Button
            variant="ghost"
            className="h-auto flex-col items-end gap-0 px-2 py-0.5 text-[13px]/4 font-normal disabled:opacity-100"
            disabled={remote.kind === 'busy'}
            title={shown?.fetch_note ?? shown?.fetched_text ?? undefined}
            onClick={onRemote}
          >
            <span
              className={cn(
                'font-semibold',
                remote.kind === 'pull' && 'text-live',
                remote.kind === 'busy' && 'text-muted-foreground',
              )}
            >
              {remote.label}
            </span>
            {shown?.fetched_text ? (
              <span className="text-(length:--size-small) text-muted-foreground">
                {shown.fetched_text}
              </span>
            ) : null}
          </Button>
        ) : null}
        <Button
          variant="ghost"
          size="xs"
          className="text-[13px] font-normal text-muted-foreground"
          title="Close the Git view (Esc)"
          onClick={() => view.closeGit()}
        >
          Close
        </Button>
      </header>
      {shown?.error ? (
        <p className="git-error git-banner" role="alert">
          {shown.error}
        </p>
      ) : null}
      {shown?.fetch_note ? (
        <p className="git-note git-banner" role="status">
          {shown.fetch_note}
        </p>
      ) : null}
      {shown?.stash ? (
        <p className="git-banner git-stash" role="status">
          Your changes of {shown.stash.branch} from {shown.stash.time_text} are in a stash.
          <Button
            variant="outline"
            size="xs"
            className="text-[13px] font-normal text-foreground"
            onClick={() => {
              const stash = shown.stash;
              if (stash) write('git.unstash', { window: windowId, stash: stash.commit });
            }}
          >
            Restore them
          </Button>
        </p>
      ) : null}
      {asked ? (
        <GitConfirm
          key={asked.seq}
          question={asked.question}
          answers={asked.answers}
          onCancel={() => setAsked(null)}
        />
      ) : null}
      {shown ? body(shown) : <p className="git-empty git-center">Looking for the repository…</p>}
    </section>
  );
}
