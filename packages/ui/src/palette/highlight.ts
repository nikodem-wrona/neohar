// The title of a command with the matched ranges marked. The core ranks and gives the
// ranges in UTF-16 code units, and a JavaScript string is sliced in those units, so the
// page cuts the title at the ranges and does not search it.

export interface Segment {
  text: string;
  /** The part of the title that the query matched. */
  hit: boolean;
}

/** The title as segments, in order. The ranges are `[start, end)` and come sorted from the core. */
export function segments(title: string, ranges: readonly (readonly [number, number])[]): Segment[] {
  const out: Segment[] = [];
  let at = 0;
  for (const [start, end] of ranges) {
    const from = Math.min(Math.max(start, at), title.length);
    const to = Math.min(Math.max(end, from), title.length);
    if (from > at) out.push({ text: title.slice(at, from), hit: false });
    if (to > from) out.push({ text: title.slice(from, to), hit: true });
    at = Math.max(at, to);
  }
  if (at < title.length) out.push({ text: title.slice(at), hit: false });
  return out;
}
