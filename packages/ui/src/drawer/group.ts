// The cards of the inbox by run. The core orders the interactions by urgency, and the
// window does not sort them again. Cards of one run sit together, in the order of the projection,
// and the runs come in the order of their first card.
//
// A run counts in `attention` without a card when it is in `error` or `rate_limited`, or when
// its turn ended while nobody looked (`turn_ended`). Such a run gets a group of its own,
// after the runs with cards, so the inbox is never empty while the badge counts. A run with cards
// keeps its place and gets the state too.

import type { AgentValue, AttentionValue, InteractionValue } from '@neohar/transport';

/** The state of a run that counts in the inbox without a card. */
export type Trouble = 'error' | 'rate_limited' | 'turn_ended';

export interface Group {
  /** The run, or `null` for interactions that belong to none. */
  run: string | null;
  interactions: InteractionValue[];
  /** The state of the run that counts in `attention` without a card, or `null`. */
  trouble: Trouble | null;
}

export function groupByRun(
  rows: readonly InteractionValue[],
  attention: readonly AttentionValue[] = [],
): Group[] {
  const groups = new Map<string | null, Group>();
  for (const row of rows) {
    const run = row.run ?? null;
    const group = groups.get(run);
    if (group) group.interactions.push(row);
    else groups.set(run, { run, interactions: [row], trouble: null });
  }
  for (const row of attention) {
    const trouble = troubleOf(row);
    if (!trouble) continue;
    const group = groups.get(row.run);
    if (group) group.trouble = trouble;
    else groups.set(row.run, { run: row.run, interactions: [], trouble });
  }
  return [...groups.values()];
}

/** The state of a row of `attention` that has no card, or `null` when its cards say it all. */
export function troubleOf(row: AttentionValue): Trouble | null {
  if (row.reasons.includes('error')) return 'error';
  if (row.reasons.includes('rate_limited')) return 'rate_limited';
  if (row.reasons.includes('turn_ended')) return 'turn_ended';
  return null;
}

/** The state in words, as the badge of the agent says it. */
export function troubleText(trouble: Trouble): string {
  switch (trouble) {
    case 'error':
      return 'error';
    case 'rate_limited':
      return 'rate limited';
    case 'turn_ended':
      return 'turn ended';
  }
}

const TROUBLE_TITLES: Record<Trouble, string> = {
  error: 'Error',
  rate_limited: 'Rate limited',
  turn_ended: 'Turn ended',
};

/** The title of the line of a run that counts without a card: the Inbox and the phone app. */
export function troubleTitle(trouble: Trouble): string {
  return TROUBLE_TITLES[trouble];
}

/**
 * Whether the box Reply shows for a run, and why not: `agent.reply` takes a run that is
 * alive, has a pane, is `idle` and has no open interaction.
 */
export type ReplyState =
  /** The box shows. */
  | 'ready'
  /** The run works: its turn did not end. */
  | 'working'
  /** An interaction of the run waits for an answer. Its card answers it. */
  | 'waiting'
  /** The run ended, has no pane, or is not known. */
  | 'gone';

export function replyState(run: AgentValue | undefined): ReplyState {
  if (!run || !run.pane) return 'gone';
  const state = run.facets.execution.value.state;
  if (state === 'exited') return 'gone';
  if (run.facets.pending.length > 0) return 'waiting';
  return state === 'idle' ? 'ready' : 'working';
}
