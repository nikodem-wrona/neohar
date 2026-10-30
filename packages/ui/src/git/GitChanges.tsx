// The Changes side of the Git view: the changed files of the worktree from `HEAD`
// (`git_changes:<window>`), with a filter, and the diff of the file that the person picks
// (`git.diff`). The diff loads again when the stamp of its row changes. The view is read-only.

import { type ChangeRow, RpcError } from '@neohar/transport';
import { type KeyboardEvent, useCallback, useEffect, useRef, useState } from 'react';
import { useApp } from '../app/context';
import { Input } from '../components/ui/input';
import { statusLetter, statusWord } from '../changes/model';
import { useLatest } from '../lib/hooks';
import { cn } from '../lib/utils';
import { DiffPane, type Loaded } from './DiffPane';
import { matches, moveIndex } from './model';

function errorText(failure: unknown): string {
  return failure instanceof RpcError ? failure.message : 'The diff did not load.';
}

export function GitChanges(props: {
  rows: ChangeRow[];
  /** What the rows compare: the worktree. Another one starts the list again. */
  subject: string | null;
  truncated: boolean;
  counted: boolean;
}) {
  const { transport, report, windowId } = useApp();
  const [filter, setFilter] = useState('');
  const [picked, setPicked] = useState<string | null>(null);
  const [loaded, setLoaded] = useState<(Loaded & { stamp: number }) | null>(null);
  const list = useRef<HTMLUListElement>(null);
  const subject = useLatest(props.subject);

  const shown = props.rows.filter((row) => matches(filter, [row.path, row.old_path]));
  const pickedRow = props.rows.find((row) => row.path === picked);

  // A new subject starts the list again, in the render where it comes, before any effect loads
  // the picked file of the old one.
  const [seen, setSeen] = useState(props.subject);
  if (seen !== props.subject) {
    setSeen(props.subject);
    setPicked(null);
    setLoaded(null);
  }

  const load = useCallback(
    (path: string, stamp: number): void => {
      const asked = subject.current;
      setLoaded({ file: path, stamp });
      transport
        .call('git.diff', { window: windowId, file: path })
        .then((diff) => ({ diff }))
        .catch((failure: unknown) => ({ error: errorText(failure) }))
        .then((result) => {
          // A diff of another file, of an older stamp or of another worktree is dropped.
          setLoaded((before) =>
            before?.file === path && before.stamp === stamp && subject.current === asked
              ? { file: path, stamp, ...result }
              : before,
          );
        })
        .catch(report);
    },
    [transport, windowId, report, subject],
  );

  // The picked file that is gone closes. A picked file whose stamp changed loads again.
  useEffect(() => {
    if (picked === null) return;
    if (!pickedRow) {
      setPicked(null);
      setLoaded(null);
      return;
    }
    if (loaded?.file !== picked || loaded.stamp !== pickedRow.stamp) load(picked, pickedRow.stamp);
  }, [picked, pickedRow, loaded, load]);

  const onListKey = (event: KeyboardEvent): void => {
    const rows = shown;
    const at = rows.findIndex((row) => row.path === picked);
    const to = moveIndex(rows.length, at, event.key);
    if (to === null) return;
    event.preventDefault();
    const path = rows[to]?.path;
    if (path === undefined) return;
    setPicked(path);
    list.current?.querySelector<HTMLElement>(`[data-path="${CSS.escape(path)}"]`)?.focus();
  };

  return (
    <>
      <div className="git-side-body">
        <Input
          type="text"
          placeholder="Filter"
          aria-label="Filter the changed files"
          className="h-auto flex-none rounded-sm px-2 py-0.5 text-[13px]/5 shadow-none md:text-[13px]/5"
          value={filter}
          onChange={(event) => setFilter(event.currentTarget.value)}
        />
        {props.truncated ? <p className="git-note">The list shows the first 1,000 files.</p> : null}
        {shown.length > 0 ? (
          <ul className="git-list" aria-label="Changed files" ref={list} onKeyDown={onListKey}>
            {shown.map((row) => (
              <li key={row.path}>
                <button
                  type="button"
                  className={cn('git-file', { picked: picked === row.path })}
                  data-path={row.path}
                  aria-pressed={picked === row.path}
                  tabIndex={picked === row.path || (picked === null && row === shown[0]) ? 0 : -1}
                  title={row.old_path ? `${row.old_path} → ${row.path}` : row.path}
                  onClick={() => setPicked(row.path)}
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
                  <span className="git-lines">
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
              </li>
            ))}
          </ul>
        ) : (
          <p className="git-empty">
            {props.counted ? (filter ? 'No file matches.' : 'No changes.') : 'Counting…'}
          </p>
        )}
      </div>
      <div className="git-detail">
        <DiffPane
          loaded={loaded}
          binary={pickedRow?.binary ?? false}
          empty="Pick a file to see its diff."
        />
      </div>
    </>
  );
}
