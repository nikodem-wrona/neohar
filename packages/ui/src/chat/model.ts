// What the chat view of a pane needs from `timeline:<run>`. The core builds the items and gives
// the full texts as sanitized HTML (`timeline.texts`). This module only groups the items into
// your prompts and the replies of the agent, says which texts to ask for, chooses the words of the
// line while the agent works, and reads the start of the composer for the list of skills. It sorts
// nothing: the order is the one that the core gives.

import type { TimelineItem } from '@neohar/transport';

/** The most items of one `timeline.texts`. */
export const TEXTS_PER_CALL = 100;

/** A prompt of yours, or what the agent did and said between two prompts. */
export interface Group {
  kind: 'prompt' | 'agent';
  items: TimelineItem[];
}

/** The items in groups, in the order they come: each prompt alone, the other items together. */
export function groupMessages(items: readonly TimelineItem[]): Group[] {
  const groups: Group[] = [];
  for (const item of items) {
    if (item.body.type === 'prompt') {
      groups.push({ kind: 'prompt', items: [item] });
      continue;
    }
    const last = groups[groups.length - 1];
    if (last?.kind === 'agent') last.items.push(item);
    else groups.push({ kind: 'agent', items: [item] });
  }
  return groups;
}

/**
 * The key of the full text of a prompt or a message of the agent, or `null` for another item.
 * A new preview of the same item makes a new key, so the view asks again.
 */
export function textKey(item: TimelineItem): string | null {
  const body = item.body;
  if (body.type !== 'prompt' && body.type !== 'assistant') return null;
  return `${item.id}\n${body.preview}`;
}

/**
 * The items whose full text the view does not have and did not ask for, for one call. The latest
 * come first, because the chat shows its end.
 */
export function missingTexts(
  items: readonly TimelineItem[],
  known: (key: string) => boolean,
): TimelineItem[] {
  const missing: TimelineItem[] = [];
  for (let at = items.length - 1; at >= 0 && missing.length < TEXTS_PER_CALL; at -= 1) {
    const item = items[at];
    if (!item) continue;
    const key = textKey(item);
    if (key !== null && !known(key)) missing.push(item);
  }
  return missing;
}

/** The words under the messages while the agent works: the tool of this turn that runs, if any. */
export function workingText(items: readonly TimelineItem[]): string {
  for (let at = items.length - 1; at >= 0; at -= 1) {
    const body = items[at]?.body;
    if (!body || body.type === 'prompt') break;
    if (body.type === 'tool' && body.status === 'running') {
      return `Working: ${body.tool} ${body.summary}`.trim();
    }
  }
  return 'Working';
}

/**
 * The name after `/` or `$` at the start of the composer while it has no white space: the list of
 * skills shows the names that have it. `null` when the composer has other text.
 */
export function skillQuery(text: string): string | null {
  const match = /^[/$](\S*)$/.exec(text);
  return match ? (match[1] ?? '') : null;
}

/** The next place in a list of `count` rows after a move by `step`, round at the ends. */
export function moveChoice(at: number, step: 1 | -1, count: number): number {
  if (count === 0) return 0;
  return (at + step + count) % count;
}
