// The Changes tab of the drawer: what changed in the git worktree that
// holds the folder of the window, the folder of the Files tab. The core finds the worktree. The
// changes go from HEAD: they are what is not committed. The page names the folder
// (`changes.follow`), and the core counts and orders the rows (`changes_for:<window>`), fills
// the head (`changes_head:<window>`) and gives the diff of one file (`changes.diff`). The window
// follows the projections only while this tab shows, and the core watches the worktree only while
// a window follows them. A diff loads when the person opens its row, and again when the stamp of
// the row changes. The page has no timer, and the view is read-only. "Open in Git view" shows the
// same worktree in the Git view, with its branches and its history. A repository that the person
// does not trust has `trust` in the head, and the tab shows the prompt of `TrustPrompt` instead.

import { type ChangeRow, type ChangesHead, RpcError } from '@neohar/transport';
import { createEffect, createMemo, createSignal, For, on, onCleanup, Show } from 'solid-js';
import { useApp } from '../app/context';
import { windowFolder } from '../files/folder';
import { TrustPrompt } from '../git/TrustPrompt';
import { DiffHunks } from '../rich/Diff';
import { createProjectionStore, type ProjectionStore } from '../stores';
import {
  type Fetched,
  headFacts,
  moveTo,
  statusLetter,
  statusWord,
  stillOpen,
  subjectOf,
  toFetch,
} from './model';

function errorText(failure: unknown): string {
  return failure instanceof RpcError ? failure.message : 'The diff did not load.';
}

export function ChangesPanel() {
  const { stores, transport, run, report, windowId, view, form } = useApp();
  const folder = createMemo(() => windowFolder(stores, windowId));

  // The subscriptions live while this tab shows and the window has a folder. Without a folder,
  // the core must not count the folder that the window named before.
  const [live, setLive] = createSignal<ProjectionStore<ChangeRow> | null>(null);
  const [heads, setHeads] = createSignal<ProjectionStore<ChangesHead> | null>(null);
  createEffect(
    on(
      () => folder() !== undefined,
      (has) => {
        if (!has) return;
        const rows = createProjectionStore(transport, `changes_for:${windowId}`);
        const head = createProjectionStore(transport, `changes_head:${windowId}`);
        setLive(rows);
        setHeads(head);
        onCleanup(() => {
          rows.close();
          head.close();
          setLive(null);
          setHeads(null);
        });
      },
    ),
  );
  const head = () => heads()?.state.byKey['_'];
  const subject = createMemo(() => subjectOf(head()));

  // The core follows the folder of the window. A `cd` in the pane with the focus names a new one.
  createEffect(
    on(folder, (path) => {
      if (path) run('changes.follow', { window: windowId, path });
    }),
  );

  const [open, setOpen] = createSignal<ReadonlySet<string>>(new Set());
  const [fetched, setFetched] = createSignal<ReadonlyMap<string, Fetched>>(new Map());
  const [focusPath, setFocusPath] = createSignal<string | null>(null);
  let list: HTMLUListElement | undefined;

  // The open rows belong to the worktree. Another one closes them.
  createEffect(
    on(subject, () => {
      setOpen(new Set<string>());
      setFetched(new Map());
      setFocusPath(null);
    }),
  );

  const rows = (): ChangeRow[] => (subject() ? (live()?.rows() ?? []) : []);
  const keys = createMemo(() => rows().map((row) => row.path));
  const byPath = createMemo(() => new Map(rows().map((row) => [row.path, row])));
  const tabStop = () => {
    const at = focusPath();
    return at !== null && byPath().has(at) ? at : (keys()[0] ?? null);
  };

  const load = (path: string, stamp: number): void => {
    const at = subject();
    // The stamp is noted at once, so that the same stamp never loads twice.
    setFetched((before) => new Map(before).set(path, { ...before.get(path), stamp }));
    transport
      .call('changes.diff', { window: windowId, path })
      .then((diff) => ({ stamp, diff }))
      .catch((failure: unknown) => ({ stamp, error: errorText(failure) }))
      .then((entry: Fetched) => {
        // A diff of an older stamp, of a row that closed, or of another worktree is dropped.
        setFetched((before) =>
          before.get(path)?.stamp === stamp && subject() === at
            ? new Map(before).set(path, entry)
            : before,
        );
      })
      .catch(report);
  };

  // An open row that is gone closes. An open row whose stamp changed loads its diff again.
  createEffect(() => {
    const now = rows();
    const kept = stillOpen(open(), now);
    if (kept.size !== open().size) setOpen(kept);
    for (const path of toFetch(kept, now, fetched())) {
      const row = byPath().get(path);
      if (row) load(path, row.stamp);
    }
  });

  const toggle = (path: string): void => {
    const next = new Set(open());
    if (next.has(path)) {
      next.delete(path);
      setFetched((before) => {
        const after = new Map(before);
        after.delete(path);
        return after;
      });
    } else {
      next.add(path);
    }
    setOpen(next);
  };

  const onListKey = (event: KeyboardEvent): void => {
    const to = moveTo(keys(), focusPath(), event.key);
    if (to === null) return;
    event.preventDefault();
    setFocusPath(to);
    list?.querySelector<HTMLElement>(`[data-path="${CSS.escape(to)}"]`)?.focus();
  };

  return (
    <div class="changes-panel">
      <Show when={folder()} fallback={<p class="drawer-empty">This tab has no folder.</p>}>
        <Show when={head()} fallback={<p class="drawer-empty">Counting…</p>}>
          {(shown) => (
            <Show
              when={!shown().trust && shown().root}
              fallback={
                <Show
                  when={shown().trust}
                  fallback={<p class="drawer-empty">This folder is not in a git repository.</p>}
                >
                  {(ask) => <TrustPrompt ask={ask()} />}
                </Show>
              }
            >
              {(root) => (
                <>
                  <header class="changes-head">
                    <div class="changes-branch">
                      <span class="changes-branch-name">
                        {shown().branch ?? (shown().changes ? 'Detached HEAD' : '')}
                      </span>
                      <Show when={form !== 'phone'}>
                        <button
                          type="button"
                          class="changes-open-git"
                          onClick={() => view.openGit(folder() ?? null)}
                        >
                          Open in Git view
                        </button>
                      </Show>
                    </div>
                    <div class="changes-root" title={root()}>
                      {root()}
                    </div>
                    <Show when={headFacts(shown()).length > 0}>
                      <div class="changes-facts" data-testid="changes-facts">
                        {headFacts(shown()).join(' · ')}
                      </div>
                    </Show>
                    <Show when={shown().changes?.truncated}>
                      <p class="changes-note">The list shows the first 1,000 files.</p>
                    </Show>
                    <Show when={shown().changes?.error}>
                      {(error) => (
                        <p class="changes-error" role="alert">
                          {error()}
                        </p>
                      )}
                    </Show>
                  </header>
                  <Show
                    when={keys().length > 0}
                    fallback={
                      <p class="drawer-empty">
                        {shown().changes ? 'No changes yet.' : 'Counting…'}
                      </p>
                    }
                  >
                    <ul
                      class="changes-list"
                      aria-label="Changed files"
                      ref={(element) => (list = element)}
                      onKeyDown={onListKey}
                    >
                      <For each={keys()}>
                        {(path) => {
                          const row = () => byPath().get(path);
                          const isOpen = () => open().has(path);
                          return (
                            <Show when={row()}>
                              {(shownRow) => (
                                <li class="changes-item">
                                  <button
                                    type="button"
                                    class="changes-row"
                                    classList={{ open: isOpen() }}
                                    data-path={path}
                                    aria-expanded={isOpen()}
                                    tabIndex={tabStop() === path ? 0 : -1}
                                    title={
                                      shownRow().old_path
                                        ? `${shownRow().old_path} → ${path}`
                                        : path
                                    }
                                    onFocus={() => setFocusPath(path)}
                                    onClick={() => toggle(path)}
                                  >
                                    <span
                                      class="changes-status"
                                      data-status={shownRow().status}
                                      title={statusWord(shownRow().status)}
                                    >
                                      <span aria-hidden="true">
                                        {statusLetter(shownRow().status)}
                                      </span>
                                      <span class="visually-hidden">
                                        {statusWord(shownRow().status)}
                                      </span>
                                    </span>
                                    <span class="changes-path">
                                      <span class="changes-dir">{shownRow().dir}</span>
                                      <span class="changes-name">{shownRow().name}</span>
                                    </span>
                                    <span class="changes-lines">
                                      <Show
                                        when={!shownRow().binary}
                                        fallback={<span class="changes-binary">binary</span>}
                                      >
                                        <span class="add">+{shownRow().add}</span>{' '}
                                        <span class="del">−{shownRow().del}</span>
                                      </Show>
                                    </span>
                                  </button>
                                  <Show when={isOpen()}>
                                    <div
                                      class="changes-diff diff"
                                      role="group"
                                      aria-label={`Diff of ${path}`}
                                    >
                                      <Show
                                        when={fetched().get(path)}
                                        fallback={<p class="drawer-empty">Loading…</p>}
                                      >
                                        {(entry) => (
                                          <Show
                                            when={entry().diff}
                                            fallback={
                                              <p
                                                class={
                                                  entry().error ? 'changes-error' : 'drawer-empty'
                                                }
                                              >
                                                {entry().error ?? 'Loading…'}
                                              </p>
                                            }
                                          >
                                            {(diff) => (
                                              <Show
                                                when={diff().hunks.length > 0}
                                                fallback={
                                                  <p class="drawer-empty">
                                                    {shownRow().binary
                                                      ? 'A binary file has no lines to show.'
                                                      : 'No lines to show.'}
                                                  </p>
                                                }
                                              >
                                                <DiffHunks file={diff()} />
                                              </Show>
                                            )}
                                          </Show>
                                        )}
                                      </Show>
                                    </div>
                                  </Show>
                                </li>
                              )}
                            </Show>
                          );
                        }}
                      </For>
                    </ul>
                  </Show>
                </>
              )}
            </Show>
          )}
        </Show>
      </Show>
    </div>
  );
}
