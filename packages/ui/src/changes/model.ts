// What the Changes tab needs from `changes_for:<window>` and `changes_head:<window>`. The core
// finds the worktree, and counts, orders and names everything: the files and the lines. This
// module only turns a status into its letter, says which open rows need their diff again, and
// moves between rows with the keys.

import type { ChangeRow, ChangeStatus, ChangesHead, FileDiff } from '@neohar/transport';

/** The letter of a status, as the row shows it. */
export function statusLetter(status: ChangeStatus): string {
  switch (status) {
    case 'added':
      return 'A';
    case 'deleted':
      return 'D';
    case 'renamed':
      return 'R';
    case 'untracked':
      return 'U';
    default:
      return 'M';
  }
}

/** The word of a status, for a screen reader and the title of the letter. */
export function statusWord(status: ChangeStatus): string {
  switch (status) {
    case 'added':
      return 'Added';
    case 'deleted':
      return 'Deleted';
    case 'renamed':
      return 'Renamed';
    case 'untracked':
      return 'Not tracked';
    default:
      return 'Modified';
  }
}

function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** The facts of the head of the tab: the files and the lines that are not committed. */
export function headFacts(head: ChangesHead): string[] {
  const facts: string[] = [];
  if (head.changes) facts.push(count(head.changes.files, 'file', 'files'));
  if (head.diff_stats) facts.push(`+${head.diff_stats.add} −${head.diff_stats.del}`);
  return facts;
}

/**
 * What the rows compare: the worktree. The open rows and their diffs belong to it, so a new one
 * closes them. A folder in the same worktree keeps them.
 */
export function subjectOf(head: ChangesHead | undefined): string | null {
  return head?.root ?? null;
}

/** A diff that the page fetched for an open row, with the stamp of the row at that time. */
export interface Fetched {
  stamp: number;
  diff?: FileDiff;
  error?: string;
}

/**
 * The open rows whose diff the page must fetch: an open row with no diff yet, or whose stamp is
 * not the stamp of its diff. A row that is not open never needs one.
 */
export function toFetch(
  open: ReadonlySet<string>,
  rows: readonly ChangeRow[],
  fetched: ReadonlyMap<string, Fetched>,
): string[] {
  return rows
    .filter((row) => open.has(row.path) && fetched.get(row.path)?.stamp !== row.stamp)
    .map((row) => row.path);
}

/** The open rows that are still rows. A file that has no change any more closes. */
export function stillOpen(open: ReadonlySet<string>, rows: readonly ChangeRow[]): Set<string> {
  return new Set(rows.filter((row) => open.has(row.path)).map((row) => row.path));
}

/** The row that a key moves to from `at`, or `null` for a key that does not move. */
export function moveTo(keys: readonly string[], at: string | null, key: string): string | null {
  if (keys.length === 0) return null;
  const index = at === null ? -1 : keys.indexOf(at);
  switch (key) {
    case 'ArrowDown':
      return keys[Math.min(index + 1, keys.length - 1)] ?? null;
    case 'ArrowUp':
      return keys[Math.max(index - 1, 0)] ?? null;
    case 'Home':
      return keys[0] ?? null;
    case 'End':
      return keys.at(-1) ?? null;
    default:
      return null;
  }
}
