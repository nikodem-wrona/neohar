// Text with highlighted ranges. The core sends the text and `[start, end, class]`
// ranges in UTF-16 code units, which is how a JavaScript string counts, so the UI slices the text
// with the numbers as they are. The core sends the ranges in order. The result is a list
// of runs of text for text nodes; no string ever becomes HTML here.

import type { Spans } from '@neohar/transport';

export interface Segment {
  text: string;
  /** The class of the range, or `null` for text between ranges. */
  className: string | null;
}

/** Cuts the text at the ranges. A range that overlaps the one before, or runs past the text, is clamped. */
export function segments(value: Spans): Segment[] {
  const { text } = value;
  const out: Segment[] = [];
  let cursor = 0;
  for (const [start, end, className] of value.spans) {
    const from = Math.max(cursor, Math.min(start, text.length));
    const to = Math.min(Math.max(end, from), text.length);
    // A range with nothing in it changes nothing, and does not cut the text.
    if (to <= from) continue;
    if (from > cursor) out.push({ text: text.slice(cursor, from), className: null });
    out.push({ text: text.slice(from, to), className });
    cursor = to;
  }
  if (cursor < text.length) out.push({ text: text.slice(cursor), className: null });
  return out;
}
