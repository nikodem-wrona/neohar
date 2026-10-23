// The right side of the Git view: the diff of one file, with its path and its lines, as the core
// makes it (`git.diff`). The view loads it; this part only paints what it has.

import type { FileDiff } from '@neohar/transport';
import { Show } from 'solid-js';
import { DiffHunks } from '../rich/Diff';

/** A diff that the view loaded, or the reason that it did not load. */
export interface Loaded {
  /** The file of the diff. */
  file: string;
  diff?: FileDiff;
  error?: string;
}

export function DiffPane(props: { loaded: Loaded | null; binary?: boolean; empty: string }) {
  return (
    <Show when={props.loaded} fallback={<p class="git-empty">{props.empty}</p>}>
      {(loaded) => (
        <div class="git-diff">
          <header class="git-diff-head">
            <span class="git-diff-path" title={loaded().file}>
              {loaded().file}
            </span>
            <Show when={loaded().diff}>
              {(diff) => (
                <span class="git-lines">
                  <span class="add">+{diff().stats.add}</span>{' '}
                  <span class="del">−{diff().stats.del}</span>
                </span>
              )}
            </Show>
          </header>
          <Show
            when={loaded().diff}
            fallback={
              <p class={loaded().error ? 'git-error' : 'git-empty'} role="status">
                {loaded().error ?? 'Loading…'}
              </p>
            }
          >
            {(diff) => (
              <Show
                when={diff().hunks.length > 0}
                fallback={
                  <p class="git-empty">
                    {props.binary ? 'A binary file has no lines to show.' : 'No lines to show.'}
                  </p>
                }
              >
                <div
                  class="git-diff-body diff"
                  role="group"
                  aria-label={`Diff of ${loaded().file}`}
                >
                  <DiffHunks file={diff()} />
                </div>
              </Show>
            )}
          </Show>
        </div>
      )}
    </Show>
  );
}
