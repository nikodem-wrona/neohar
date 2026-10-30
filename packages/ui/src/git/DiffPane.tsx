// The right side of the Git view: the diff of one file, with its path and its lines, as the core
// makes it (`git.diff`, at most 20,000 lines). The view loads it; this part only paints what it
// has, and the box of the lines paints only the rows on screen.

import type { FileDiff } from '@neohar/transport';
import { VirtualDiff } from './VirtualDiff';

/** A diff that the view loaded, or the reason that it did not load. */
export interface Loaded {
  /** The file of the diff. */
  file: string;
  diff?: FileDiff;
  error?: string;
}

export function DiffPane(props: { loaded: Loaded | null; binary?: boolean; empty: string }) {
  const loaded = props.loaded;
  if (!loaded) return <p className="git-empty">{props.empty}</p>;
  const diff = loaded.diff;
  return (
    <div className="git-diff">
      <header className="git-diff-head">
        <span className="git-diff-path" title={loaded.file}>
          {loaded.file}
        </span>
        {diff ? (
          <span className="git-lines">
            <span className="add">+{diff.stats.add}</span>{' '}
            <span className="del">−{diff.stats.del}</span>
          </span>
        ) : null}
      </header>
      {diff ? (
        diff.hunks.length > 0 ? (
          <>
            {diff.truncated ? (
              <p className="git-note">The diff of this file is cut at 20,000 lines.</p>
            ) : null}
            <VirtualDiff file={diff} label={`Diff of ${loaded.file}`} />
          </>
        ) : (
          <p className="git-empty">
            {props.binary ? 'A binary file has no lines to show.' : 'No lines to show.'}
          </p>
        )
      ) : (
        <p className={loaded.error ? 'git-error' : 'git-empty'} role="status">
          {loaded.error ?? 'Loading…'}
        </p>
      )}
    </div>
  );
}
