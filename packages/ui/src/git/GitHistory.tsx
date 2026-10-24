// The History side of the Git view: the commits of `HEAD`, the newest first, in pages of 200
// (`git.log`), with a mark on each commit that a push would send. A pick shows the commit with its
// message and its files (`git.show`), and a pick of a file shows its diff from the first parent
// (`git.diff` with the commit). A new `HEAD` reads the history again from the start.

import { type GitCommit, type GitLogResult, type GitShown, RpcError } from '@neohar/transport';
import { createEffect, createSignal, For, on, Show } from 'solid-js';
import { useApp } from '../app/context';
import { statusLetter, statusWord } from '../changes/model';
import { DiffPane, type Loaded } from './DiffPane';
import { commitLabels, matches, moveIndex } from './model';

function errorText(failure: unknown, what: string): string {
  return failure instanceof RpcError ? failure.message : `${what} did not load.`;
}

export function GitHistory(props: {
  /** The commit of `HEAD`. Another one reads the history again. */
  head: string | null;
  /** The worktree. Another one reads the history again. */
  subject: string | null;
}) {
  const { transport, report, windowId } = useApp();
  const [commits, setCommits] = createSignal<GitCommit[]>([]);
  const [more, setMore] = createSignal(false);
  const [loading, setLoading] = createSignal(false);
  const [failed, setFailed] = createSignal<string | null>(null);
  const [filter, setFilter] = createSignal('');
  const [picked, setPicked] = createSignal<string | null>(null);
  const [shown, setShown] = createSignal<GitShown | { error: string } | null>(null);
  const [loaded, setLoaded] = createSignal<Loaded | null>(null);
  let list: HTMLUListElement | undefined;
  /** Grows with each new start, so that a page of an older start is dropped. */
  let generation = 0;

  const page = (skip: number): void => {
    const at = generation;
    setLoading(true);
    transport
      .call('git.log', { window: windowId, skip })
      .then((result: GitLogResult) => {
        if (at !== generation) return;
        setCommits((before) => (skip === 0 ? result.commits : [...before, ...result.commits]));
        setMore(result.more);
        setFailed(null);
      })
      .catch((failure: unknown) => {
        if (at === generation) setFailed(errorText(failure, 'The history'));
      })
      .finally(() => {
        if (at === generation) setLoading(false);
      })
      .catch(report);
  };

  createEffect(
    on(
      () => [props.subject, props.head] as const,
      ([subject]) => {
        generation += 1;
        setCommits([]);
        setMore(false);
        setPicked(null);
        setShown(null);
        setLoaded(null);
        if (subject) page(0);
      },
    ),
  );

  const pick = (hash: string): void => {
    setPicked(hash);
    setShown(null);
    setLoaded(null);
    transport
      .call('git.show', { window: windowId, commit: hash })
      .then((result) => {
        if (picked() === hash) setShown(result);
      })
      .catch((failure: unknown) => {
        if (picked() === hash) setShown({ error: errorText(failure, 'The commit') });
      })
      .catch(report);
  };

  const pickFile = (commit: string, file: string): void => {
    setLoaded({ file });
    transport
      .call('git.diff', { window: windowId, file, commit })
      .then((diff) => ({ diff }))
      .catch((failure: unknown) => ({ error: errorText(failure, 'The diff') }))
      .then((result) => {
        setLoaded((before) =>
          before?.file === file && picked() === commit ? { file, ...result } : before,
        );
      })
      .catch(report);
  };

  const visible = () =>
    commits().filter((commit) =>
      matches(filter(), [commit.subject, commit.author, commit.short, ...commitLabels(commit)]),
    );

  const onListKey = (event: KeyboardEvent): void => {
    const rows = visible();
    const at = rows.findIndex((commit) => commit.hash === picked());
    const to = moveIndex(rows.length, at, event.key);
    if (to === null) return;
    event.preventDefault();
    const hash = rows[to]?.hash;
    if (hash === undefined) return;
    pick(hash);
    list?.querySelector<HTMLElement>(`[data-hash="${hash}"]`)?.focus();
  };

  const detail = () => {
    const value = shown();
    return value && 'commit' in value ? value : null;
  };

  return (
    <>
      <div class="git-side-body">
        <input
          class="git-filter"
          type="text"
          placeholder="Filter"
          aria-label="Filter the commits"
          value={filter()}
          onInput={(event) => setFilter(event.currentTarget.value)}
        />
        <Show when={failed()}>
          {(text) => (
            <p class="git-error" role="alert">
              {text()}
            </p>
          )}
        </Show>
        <Show
          when={visible().length > 0}
          fallback={
            <p class="git-empty">
              {loading() ? 'Loading…' : filter() ? 'No commit matches.' : 'No commit yet.'}
            </p>
          }
        >
          <ul
            class="git-list"
            aria-label="Commits"
            ref={(element) => (list = element)}
            onKeyDown={onListKey}
          >
            <For each={visible()}>
              {(commit) => (
                <li>
                  <button
                    type="button"
                    class="git-commit"
                    classList={{ picked: picked() === commit.hash }}
                    data-hash={commit.hash}
                    aria-pressed={picked() === commit.hash}
                    tabIndex={
                      picked() === commit.hash || (picked() === null && commit === visible()[0])
                        ? 0
                        : -1
                    }
                    onClick={() => pick(commit.hash)}
                  >
                    <span class="git-commit-subject">
                      <Show when={commit.unpushed}>
                        <span class="git-unpushed" title="Not pushed">
                          ↑
                        </span>
                      </Show>
                      {commit.subject}
                    </span>
                    <span class="git-commit-meta">
                      {commit.author} · {commit.time_text} ·{' '}
                      <span class="git-hash">{commit.short}</span>
                    </span>
                    <Show when={commitLabels(commit).length > 0}>
                      <span class="git-labels">
                        <For each={commitLabels(commit)}>
                          {(label) => <span class="git-label">{label}</span>}
                        </For>
                      </span>
                    </Show>
                  </button>
                </li>
              )}
            </For>
          </ul>
          <Show when={more()}>
            <button
              type="button"
              class="git-more"
              disabled={loading()}
              onClick={() => page(commits().length)}
            >
              {loading() ? 'Loading…' : 'Show more commits'}
            </button>
          </Show>
        </Show>
      </div>
      <div class="git-detail">
        <Show
          when={picked()}
          fallback={<p class="git-empty">Pick a commit to see its message and its files.</p>}
        >
          {(hash) => (
            <Show
              when={detail()}
              fallback={
                <p class={shown() ? 'git-error' : 'git-empty'} role="status">
                  {(() => {
                    const value = shown();
                    return value && 'error' in value ? value.error : 'Loading…';
                  })()}
                </p>
              }
            >
              {(commit) => (
                <div class="git-commit-detail">
                  <header class="git-commit-head">
                    <h2 class="git-commit-title">{commit().commit.subject}</h2>
                    <p class="git-commit-meta">
                      {commit().commit.author} · {commit().commit.time_text} ·{' '}
                      <span class="git-hash">{commit().commit.short}</span>
                      <Show when={commit().parents.length > 1}>
                        {' '}
                        · merge of {commit().parents.join(', ')}
                      </Show>
                    </p>
                    <Show when={commit().body}>
                      <pre class="git-commit-body">{commit().body}</pre>
                    </Show>
                  </header>
                  <ul class="git-list git-commit-files" aria-label="Files of the commit">
                    <For each={commit().files}>
                      {(row) => (
                        <li>
                          <button
                            type="button"
                            class="git-file"
                            classList={{ picked: loaded()?.file === row.path }}
                            aria-pressed={loaded()?.file === row.path}
                            title={row.old_path ? `${row.old_path} → ${row.path}` : row.path}
                            onClick={() => pickFile(hash(), row.path)}
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
                                <span class="add">+{row.add}</span>{' '}
                                <span class="del">−{row.del}</span>
                              </Show>
                            </span>
                          </button>
                        </li>
                      )}
                    </For>
                  </ul>
                  <Show when={commit().truncated}>
                    <p class="git-note">The list shows the first 1,000 files.</p>
                  </Show>
                  <DiffPane loaded={loaded()} empty="Pick a file to see its diff." />
                </div>
              )}
            </Show>
          )}
        </Show>
      </div>
    </>
  );
}
