// The lines of a diff for the card. A line is spans of highlighted text, and `intra`
// marks the characters that changed inside it. The UI puts both in one list of runs, so that a
// run can have a syntax class and the mark of the change at the same time.

import type { DiffLine, FileDiff } from '@neohar/transport';
import { type Segment, segments } from './segments';

/** The class that the runs of a changed range get, next to their own class. */
export const INTRA_CLASS = 'diff-intra';

export function lineSegments(line: DiffLine): Segment[] {
  const base = segments(line.text);
  const intra = line.intra ?? [];
  if (intra.length === 0) return base;
  const out: Segment[] = [];
  let at = 0;
  for (const run of base) {
    const end = at + run.text.length;
    // The edges of the changed ranges inside this run, in the order of the ranges.
    const cuts: number[] = [];
    for (const [start, stop] of intra) {
      for (const edge of [start, stop]) {
        if (edge > at && edge < end && edge > (cuts.at(-1) ?? at)) cuts.push(edge);
      }
    }
    let from = at;
    for (const cut of [...cuts, end]) {
      const inside = intra.some(([start, stop]) => from >= start && from < stop);
      const names = [run.className, inside ? INTRA_CLASS : null].filter(Boolean);
      out.push({
        text: run.text.slice(from - at, cut - at),
        className: names.length > 0 ? names.join(' ') : null,
      });
      from = cut;
    }
    at = end;
  }
  return out.filter((s) => s.text.length > 0);
}

/** The sign in front of a line. */
export function marker(kind: DiffLine['kind']): string {
  return kind === 'add' ? '+' : kind === 'del' ? '−' : ' ';
}

/** The title of a file: its path and the counts. */
export function fileTitle(file: FileDiff): string {
  const status = file.status === 'modified' ? '' : `${file.status} `;
  return `${status}${file.path}  +${file.stats.add} −${file.stats.del}`;
}
