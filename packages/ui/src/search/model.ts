// What the search overlay needs from `search.query`. The core searches the
// archive, ranks the hits and marks the matches. This module only builds the params from the
// words and the scope, and cuts the text of a hit at the marked ranges.

import type { SearchQuery, Spans } from '@neohar/transport';
import { type Segment, segments } from '../palette/highlight';

export type Scope = 'all' | 'workspace' | 'pane';

/** The most hits that the overlay asks for. */
export const LIMIT = 50;

/** What the window knows of its place: the pane in focus and the workspace that it shows. */
export interface Place {
  pane?: string | null | undefined;
  workspace?: string | null | undefined;
}

/**
 * The params of `search.query`, or `null` when there is nothing to ask: no words, or a scope that
 * the window has no place for.
 */
export function paramsOf(query: string, scope: Scope, place: Place): SearchQuery | null {
  if (query.trim() === '') return null;
  if (scope === 'pane') return place.pane ? { query, limit: LIMIT, pane: place.pane } : null;
  if (scope === 'workspace') {
    return place.workspace ? { query, limit: LIMIT, workspace: place.workspace } : null;
  }
  return { query, limit: LIMIT };
}

/** The text of a hit as segments. The core marks the matches with the class `match`. */
export function hitSegments(text: Spans): Segment[] {
  const ranges = text.spans
    .filter(([, , kind]) => kind === 'match')
    .map(([start, end]) => [start, end] as const);
  return segments(text.text, ranges);
}

export const SCOPES: readonly { id: Scope; label: string }[] = [
  { id: 'all', label: 'All panes' },
  { id: 'workspace', label: 'This workspace' },
  { id: 'pane', label: 'This pane' },
];
