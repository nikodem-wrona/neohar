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
import { type KeyboardEvent, useEffect, useRef, useState } from 'react';
import { useApp } from '../app/context';
import { useWindowFolder } from '../files/folder';
import { TrustPrompt } from '../git/TrustPrompt';
import { useLatest, useRows, useSelect } from '../lib/hooks';
import { cn } from '../lib/utils';
import { DiffHunks } from '../rich/Diff';
import { useProjectionStore } from '../stores';
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

/** The rows of a worktree that is not known yet: the same array at each read. */
const NO_ROWS: ChangeRow[] = [];

export function ChangesPanel() {
  const { stores, windowId } = useApp();
  const folder = useWindowFolder(stores, windowId);
  // The subscriptions live while this tab shows and the window has a folder. Without a folder,
  // the core must not count the folder that the window named before. `FolderChanges` owns them.
  return (
    <div className="changes-panel">
      {folder !== undefined ? (
        <FolderChanges folder={folder} />
      ) : (
        <p className="drawer-empty">This tab has no folder.</p>
      )}
    </div>
  );
}

/** The tab for a window that has a folder. */
function FolderChanges(props: { folder: string }) {
  const { transport, run, report, windowId, view, form } = useApp();
  const { folder } = props;
  const live = useProjectionStore(transport, `changes_for:${windowId}`);
  const heads = useProjectionStore(transport, `changes_head:${windowId}`);
  const head = useSelect(heads, (state) => state.byKey['_']);
  const subject = subjectOf(head);

  // The core follows the folder of the window. A `cd` in the pane with the focus names a new one.
  useEffect(() => {
    if (folder) run('changes.follow', { window: windowId, path: folder });
  }, [folder, run, windowId]);

  const [open, setOpen] = useState<ReadonlySet<string>>(() => new Set());
  const [fetched, setFetched] = useState<ReadonlyMap<string, Fetched>>(() => new Map());
  const [focusPath, setFocusPath] = useState<string | null>(null);
  const list = useRef<HTMLUListElement>(null);

  // The open rows belong to the worktree. Another one closes them. The reset is in the render, so
  // that no effect sees the open rows of the worktree before.
  const [openOf, setOpenOf] = useState(subject);
  if (openOf !== subject) {
    setOpenOf(subject);
    setOpen(new Set<string>());
    setFetched(new Map());
    setFocusPath(null);
  }

  const liveRows = useRows(live);
  const rows: ChangeRow[] = subject ? liveRows : NO_ROWS;
  const keys = rows.map((row) => row.path);
  const byPath = new Map(rows.map((row) => [row.path, row]));
  const tabStop = focusPath !== null && byPath.has(focusPath) ? focusPath : (keys[0] ?? null);

  const subjectNow = useLatest(subject);
  const load = (path: string, stamp: number): void => {
    const at = subject;
    // The stamp is noted at once, so that the same stamp never loads twice.
    setFetched((before) => new Map(before).set(path, { ...before.get(path), stamp }));
    transport
      .call('changes.diff', { window: windowId, path })
      .then((diff) => ({ stamp, diff }))
      .catch((failure: unknown) => ({ stamp, error: errorText(failure) }))
      .then((entry: Fetched) => {
        // A diff of an older stamp, of a row that closed, or of another worktree is dropped.
        setFetched((before) =>
          before.get(path)?.stamp === stamp && subjectNow.current === at
            ? new Map(before).set(path, entry)
            : before,
        );
      })
      .catch(report);
  };

  // An open row that is gone closes. An open row whose stamp changed loads its diff again.
  const loadNow = useLatest(load);
  useEffect(() => {
    const kept = stillOpen(open, rows);
    if (kept.size !== open.size) setOpen(kept);
    const rowOf = new Map(rows.map((row) => [row.path, row]));
    for (const path of toFetch(kept, rows, fetched)) {
      const row = rowOf.get(path);
      if (row) loadNow.current(path, row.stamp);
    }
  }, [rows, open, fetched, loadNow]);

  const toggle = (path: string): void => {
    const next = new Set(open);
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

  const onListKey = (event: KeyboardEvent<HTMLUListElement>): void => {
    const to = moveTo(keys, focusPath, event.key);
    if (to === null) return;
    event.preventDefault();
    setFocusPath(to);
    list.current?.querySelector<HTMLElement>(`[data-path="${CSS.escape(to)}"]`)?.focus();
  };

  /** The diff of an open row: loading, an error, no lines, or its hunks. */
  const diffOf = (row: ChangeRow) => {
    const entry = fetched.get(row.path);
    if (!entry) return <p className="drawer-empty">Loading…</p>;
    if (!entry.diff) {
      return (
        <p className={entry.error ? 'changes-error' : 'drawer-empty'}>
          {entry.error ?? 'Loading…'}
        </p>
      );
    }
    if (entry.diff.hunks.length === 0) {
      return (
        <p className="drawer-empty">
          {row.binary ? 'A binary file has no lines to show.' : 'No lines to show.'}
        </p>
      );
    }
    return <DiffHunks file={entry.diff} />;
  };

  /** The head and the rows of a worktree that the person trusts. */
  const worktree = (shown: ChangesHead, root: string) => (
    <>
      <header className="changes-head">
        <div className="changes-branch">
          <span className="changes-branch-name">
            {shown.branch ?? (shown.changes ? 'Detached HEAD' : '')}
          </span>
          {form !== 'phone' ? (
            <button type="button" className="changes-open-git" onClick={() => view.openGit(folder)}>
              Open in Git view
            </button>
          ) : null}
        </div>
        <div className="changes-root" title={root}>
          {root}
        </div>
        {headFacts(shown).length > 0 ? (
          <div className="changes-facts" data-testid="changes-facts">
            {headFacts(shown).join(' · ')}
          </div>
        ) : null}
        {shown.changes?.truncated ? (
          <p className="changes-note">The list shows the first 1,000 files.</p>
        ) : null}
        {shown.changes?.error ? (
          <p className="changes-error" role="alert">
            {shown.changes.error}
          </p>
        ) : null}
      </header>
      {keys.length > 0 ? (
        <ul className="changes-list" aria-label="Changed files" ref={list} onKeyDown={onListKey}>
          {rows.map((row) => {
            const path = row.path;
            const isOpen = open.has(path);
            return (
              <li key={path} className="changes-item">
                <button
                  type="button"
                  className={cn('changes-row', { open: isOpen })}
                  data-path={path}
                  aria-expanded={isOpen}
                  tabIndex={tabStop === path ? 0 : -1}
                  title={row.old_path ? `${row.old_path} → ${path}` : path}
                  onFocus={() => setFocusPath(path)}
                  onClick={() => toggle(path)}
                >
                  <span
                    className="changes-status"
                    data-status={row.status}
                    title={statusWord(row.status)}
                  >
                    <span aria-hidden="true">{statusLetter(row.status)}</span>
                    <span className="visually-hidden">{statusWord(row.status)}</span>
                  </span>
                  <span className="changes-path">
                    <span className="changes-dir">{row.dir}</span>
                    <span className="changes-name">{row.name}</span>
                  </span>
                  <span className="changes-lines">
                    {!row.binary ? (
                      <>
                        <span className="add">+{row.add}</span>{' '}
                        <span className="del">−{row.del}</span>
                      </>
                    ) : (
                      <span className="changes-binary">binary</span>
                    )}
                  </span>
                </button>
                {isOpen ? (
                  <div className="changes-diff diff" role="group" aria-label={`Diff of ${path}`}>
                    {diffOf(row)}
                  </div>
                ) : null}
              </li>
            );
          })}
        </ul>
      ) : (
        <p className="drawer-empty">{shown.changes ? 'No changes yet.' : 'Counting…'}</p>
      )}
    </>
  );

  /** The tab for the head that the core gave. */
  const content = (shown: ChangesHead) => {
    if (!shown.trust && shown.root) return worktree(shown, shown.root);
    if (shown.trust) return <TrustPrompt ask={shown.trust} />;
    return <p className="drawer-empty">This folder is not in a git repository.</p>;
  };

  if (!folder) return <p className="drawer-empty">This tab has no folder.</p>;
  return head ? content(head) : <p className="drawer-empty">Counting…</p>;
}
