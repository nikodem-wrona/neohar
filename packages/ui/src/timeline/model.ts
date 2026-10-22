// What the Timeline tab and the status bar need from `timeline:<run>`, `limits` and the usage of a
// run. The core builds the items, orders them and counts the usage. This
// module only picks the run in focus, joins pages, groups the items by turn, and chooses the words
// for an item. It sorts nothing: the order is the one that the core gives.

import type {
  LimitValue,
  PaneValue,
  RateLimitWindow,
  TimelineItem,
  ToolStatus,
  Usage,
} from '@neohar/transport';

/** How many items `timeline:<run>` holds at most. With fewer, no older item exists. */
export const LIVE_ITEMS = 500;

/** The run of the agent in the pane that has the focus, if there is one. */
export function focusedRun(pane: PaneValue | undefined): string | undefined {
  return pane?.run ?? undefined;
}

/**
 * The older items first and then the live ones. An item that both lists have is shown once, from
 * the live list, because the live list has the latest state of a tool call.
 */
export function joinPages(
  older: readonly TimelineItem[],
  live: readonly TimelineItem[],
): TimelineItem[] {
  const liveIds = new Set(live.map((item) => item.id));
  return [...older.filter((item) => !liveIds.has(item.id)), ...live];
}

export interface Turn {
  turn: number;
  items: TimelineItem[];
}

/** The items in groups of one turn each, in the order they come. */
export function groupByTurn(items: readonly TimelineItem[]): Turn[] {
  const turns: Turn[] = [];
  for (const item of items) {
    const last = turns[turns.length - 1];
    if (last && last.turn === item.turn) last.items.push(item);
    else turns.push({ turn: item.turn, items: [item] });
  }
  return turns;
}

export interface Line {
  /** The kind of the item, for a class and for a screen reader. */
  kind: string;
  /** The short name before the text. */
  label: string;
  text: string;
  /** The outcome of a tool call, in words. */
  status?: string;
  /** The text was cut by the core. */
  truncated?: boolean;
}

const STATUS: Record<ToolStatus, string> = {
  running: 'running',
  ok: 'done',
  failed: 'failed',
  denied: 'refused',
};

export function statusWord(status: ToolStatus): string {
  return STATUS[status];
}

/** The words of one item. */
export function lineOf(item: TimelineItem): Line {
  const body = item.body;
  switch (body.type) {
    case 'prompt':
      return { kind: 'prompt', label: 'Prompt', text: body.preview };
    case 'assistant':
      return { kind: 'assistant', label: 'Agent', text: body.preview };
    case 'tool':
      return {
        kind: 'tool',
        label: body.tool,
        text: body.summary,
        status: statusWord(body.status),
      };
    case 'diff':
      return {
        kind: 'diff',
        label: 'Changes',
        text: `${body.diff.files.length} ${body.diff.files.length === 1 ? 'file' : 'files'}`,
      };
    case 'output':
      return {
        kind: 'output',
        label: 'Output',
        text: body.text,
        truncated: body.truncated === true,
      };
    case 'subagent':
      return {
        kind: 'subagent',
        label: 'Subagent',
        text: body.agent_type,
        status: body.finished ? 'done' : 'running',
      };
    case 'compaction':
      return { kind: 'compaction', label: 'Compacted', text: 'The conversation was shortened.' };
  }
}

/** The tokens of a run in words, and its cost when the core has one. */
export function usageText(usage: Usage): string {
  const parts = [`${usage.input_tokens} in`, `${usage.output_tokens} out`];
  const cached = usage.cache_read_tokens;
  if (cached > 0) parts.push(`${cached} cached`);
  if (usage.cost_usd !== null && usage.cost_usd !== undefined) {
    parts.push(`$${usage.cost_usd.toFixed(usage.cost_usd < 1 ? 4 : 2)}`);
  }
  return parts.join(' · ');
}

/** The cost of a run for the strip of its pane, `$1.84`, or `null` when the core has no price. */
export function costText(usage: Usage): string | null {
  if (usage.cost_usd === null || usage.cost_usd === undefined) return null;
  return `$${usage.cost_usd.toFixed(2)}`;
}

/** Whether the usage has anything to show. */
export function hasUsage(usage: Usage): boolean {
  return (
    usage.input_tokens + usage.output_tokens + usage.cache_read_tokens + usage.cache_write_tokens >
    0
  );
}

/** The window of a limit in words: `5h 61%`. */
export function windowText(window: RateLimitWindow): string {
  return `${window.name} ${Math.round(window.used_percent)}%`;
}

/** The text of the status bar for one harness, or `null` when it has no window. */
export function limitText(limit: LimitValue, harness: string): string | null {
  if (limit.windows.length === 0) return null;
  return `${harness} ${limit.windows.map(windowText).join(' · ')}`;
}

/**
 * The last message of the agent in a turn: the item whose full text the run view asks for with
 * `timeline.item`. The view shows it in full and leaves out its preview.
 */
export function lastMessage(turn: Turn | undefined): TimelineItem | undefined {
  if (!turn) return undefined;
  for (let at = turn.items.length - 1; at >= 0; at -= 1) {
    const item = turn.items[at];
    if (item?.body.type === 'assistant') return item;
  }
  return undefined;
}
