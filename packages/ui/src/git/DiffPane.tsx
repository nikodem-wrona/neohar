// The right side of the Git view: the diff of one file, with its path and its lines, as the core
// makes it (`git.diff`, at most 20,000 lines). The view loads it; this part only paints what it
// has, and the box of the lines paints only the rows on screen.

import type { FileDiff } from '@neohar/transport';
import { Show } from 'solid-js';
import { VirtualDiff } from './VirtualDiff';

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
                <Show when={diff().truncated}>
                  <p class="git-note">The diff of this file is cut at 20,000 lines.</p>
                </Show>
                <VirtualDiff file={diff()} label={`Diff of ${loaded().file}`} />
              </Show>
            )}
          </Show>
        </div>
      )}
    </Show>
  );
}
