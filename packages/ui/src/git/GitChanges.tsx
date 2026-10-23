// The Changes side of the Git view: the changed files of the worktree from `HEAD`
// (`git_changes:<window>`), with a filter, and the diff of the file that the person picks
// (`git.diff`). The diff loads again when the stamp of its row changes. The view is read-only.

import { type ChangeRow, RpcError } from '@neohar/transport';
import { createEffect, createMemo, createSignal, For, on, Show } from 'solid-js';
import { useApp } from '../app/context';
import { statusLetter, statusWord } from '../changes/model';
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
  const [filter, setFilter] = createSignal('');
  const [picked, setPicked] = createSignal<string | null>(null);
  const [loaded, setLoaded] = createSignal<(Loaded & { stamp: number }) | null>(null);
  let list: HTMLUListElement | undefined;

  const shown = createMemo(() =>
    props.rows.filter((row) => matches(filter(), [row.path, row.old_path])),
  );
  const pickedRow = () => props.rows.find((row) => row.path === picked());

  createEffect(
    on(
      () => props.subject,
      () => {
        setPicked(null);
        setLoaded(null);
      },
    ),
  );

  const load = (path: string, stamp: number): void => {
    const subject = props.subject;
    setLoaded({ file: path, stamp });
    transport
      .call('git.diff', { window: windowId, file: path })
      .then((diff) => ({ diff }))
      .catch((failure: unknown) => ({ error: errorText(failure) }))
      .then((result) => {
        // A diff of another file, of an older stamp or of another worktree is dropped.
        setLoaded((before) =>
          before?.file === path && before.stamp === stamp && props.subject === subject
            ? { file: path, stamp, ...result }
            : before,
        );
      })
      .catch(report);
  };

  // The picked file that is gone closes. A picked file whose stamp changed loads again.
  createEffect(() => {
    const path = picked();
    if (path === null) return;
    const row = pickedRow();
    if (!row) {
      setPicked(null);
      setLoaded(null);
      return;
    }
    if (loaded()?.file !== path || loaded()?.stamp !== row.stamp) load(path, row.stamp);
  });

  const onListKey = (event: KeyboardEvent): void => {
    const rows = shown();
    const at = rows.findIndex((row) => row.path === picked());
    const to = moveIndex(rows.length, at, event.key);
    if (to === null) return;
    event.preventDefault();
    const path = rows[to]?.path;
    if (path === undefined) return;
    setPicked(path);
    list?.querySelector<HTMLElement>(`[data-path="${CSS.escape(path)}"]`)?.focus();
  };

  return (
    <>
      <div class="git-side-body">
        <input
          class="git-filter"
          type="text"
          placeholder="Filter"
          aria-label="Filter the changed files"
          value={filter()}
          onInput={(event) => setFilter(event.currentTarget.value)}
        />
        <Show when={props.truncated}>
          <p class="git-note">The list shows the first 1,000 files.</p>
        </Show>
        <Show
          when={shown().length > 0}
          fallback={
            <p class="git-empty">
              {props.counted ? (filter() ? 'No file matches.' : 'No changes.') : 'Counting…'}
            </p>
          }
        >
          <ul
            class="git-list"
            aria-label="Changed files"
            ref={(element) => (list = element)}
            onKeyDown={onListKey}
          >
            <For each={shown()}>
              {(row) => (
                <li>
                  <button
                    type="button"
                    class="git-file"
                    classList={{ picked: picked() === row.path }}
                    data-path={row.path}
                    aria-pressed={picked() === row.path}
                    tabIndex={
                      picked() === row.path || (picked() === null && row === shown()[0]) ? 0 : -1
                    }
                    title={row.old_path ? `${row.old_path} → ${row.path}` : row.path}
                    onClick={() => setPicked(row.path)}
                  >
                    <span
                      class="changes-status"
                      data-status={row.status}
                      title={statusWord(row.status)}
                    >
                      <span aria-hidden="true">{statusLetter(row.status)}</span>
                      <span class="visually-hidden">{statusWord(row.status)}</span>
                    </span>
                    <span class="changes-path">
                      <span class="changes-dir">{row.dir}</span>
                      <span class="changes-name">{row.name}</span>
                    </span>
                    <span class="git-lines">
                      <Show
                        when={!row.binary}
                        fallback={<span class="changes-binary">binary</span>}
                      >
                        <span class="add">+{row.add}</span> <span class="del">−{row.del}</span>
                      </Show>
                    </span>
                  </button>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </div>
      <div class="git-detail">
        <DiffPane
          loaded={loaded()}
          binary={pickedRow()?.binary ?? false}
          empty="Pick a file to see its diff."
        />
      </div>
    </>
  );
}
