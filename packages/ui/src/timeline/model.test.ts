import { expect, test } from 'bun:test';
import type { LimitValue, PaneValue, TimelineItem, Usage } from '@neohar/transport';
import {
  costText,
  focusedRun,
  groupByTurn,
  hasUsage,
  joinPages,
  limitText,
  lineOf,
  usageText,
  windowText,
} from './model';

function item(id: string, turn: number, body: TimelineItem['body']): TimelineItem {
  return { id, turn, at: '2026-10-11T00:00:00.000Z', body };
}

const prompt = (id: string, turn: number): TimelineItem =>
  item(id, turn, { type: 'prompt', preview: `prompt ${id}` });

test('the run in focus is the run of the pane that has the focus', () => {
  expect(focusedRun({ run: 'r1' } as PaneValue)).toBe('r1');
  expect(focusedRun({ run: null } as PaneValue)).toBeUndefined();
  expect(focusedRun(undefined)).toBeUndefined();
});

test('older items come first and a live item replaces an older one with the same id', () => {
  const older = [
    prompt('a', 1),
    item('t', 1, { type: 'tool', tool: 'Bash', summary: 'ls', status: 'running' }),
  ];
  const live = [
    item('t', 1, { type: 'tool', tool: 'Bash', summary: 'ls', status: 'ok', duration_ms: 5 }),
    prompt('b', 2),
  ];
  const joined = joinPages(older, live);
  expect(joined.map((i) => i.id)).toEqual(['a', 't', 'b']);
  expect(joined[1]?.body).toMatchObject({ status: 'ok' });
});

test('items group by turn in the order they come', () => {
  const turns = groupByTurn([prompt('a', 1), prompt('b', 1), prompt('c', 2), prompt('d', 1)]);
  expect(turns.map((t) => [t.turn, t.items.map((i) => i.id)])).toEqual([
    [1, ['a', 'b']],
    [2, ['c']],
    [1, ['d']],
  ]);
  expect(groupByTurn([])).toEqual([]);
});

test('each kind of item has its words', () => {
  expect(lineOf(prompt('a', 1))).toEqual({ kind: 'prompt', label: 'Prompt', text: 'prompt a' });
  expect(lineOf(item('x', 1, { type: 'assistant', preview: 'done' })).label).toBe('Agent');
  expect(
    lineOf(item('t', 1, { type: 'tool', tool: 'Edit', summary: '/a.ts', status: 'denied' })),
  ).toEqual({ kind: 'tool', label: 'Edit', text: '/a.ts', status: 'refused' });
  expect(lineOf(item('o', 1, { type: 'output', text: 'abc', truncated: true }))).toMatchObject({
    kind: 'output',
    truncated: true,
  });
  expect(
    lineOf(
      item('s', 1, { type: 'subagent', agent_id: 'a', agent_type: 'Explore', finished: true }),
    ),
  ).toMatchObject({ label: 'Subagent', text: 'Explore', status: 'done' });
  expect(lineOf(item('c', 1, { type: 'compaction' })).kind).toBe('compaction');
  expect(lineOf(item('d', 1, { type: 'diff', diff: { files: [{}, {}] } as never })).text).toBe(
    '2 files',
  );
});

test('the usage reads in words, with a cost only when the core has one', () => {
  const usage: Usage = {
    input_tokens: 1200,
    output_tokens: 30,
    cache_read_tokens: 0,
    cache_write_tokens: 0,
  };
  expect(usageText(usage)).toBe('1200 in · 30 out');
  expect(usageText({ ...usage, cache_read_tokens: 500, cost_usd: 0.0044 })).toBe(
    '1200 in · 30 out · 500 cached · $0.0044',
  );
  expect(usageText({ ...usage, cost_usd: 12.5 })).toBe('1200 in · 30 out · $12.50');
  expect(usageText({ ...usage, cost_usd: null })).toBe('1200 in · 30 out');
  expect(hasUsage(usage)).toBe(true);
  expect(hasUsage({ ...usage, input_tokens: 0, output_tokens: 0 })).toBe(false);
});

test('a limit reads as its windows and has no text without one', () => {
  const limit: LimitValue = {
    account: 'default',
    harness: 'codex',
    usage_today: { input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0 },
    windows: [
      { name: '5h', used_percent: 60.6 },
      { name: 'weekly', used_percent: 10 },
    ],
  };
  expect(windowText(limit.windows[0]!)).toBe('5h 61%');
  expect(limitText(limit, 'Codex')).toBe('Codex 5h 61% · weekly 10%');
  expect(limitText({ ...limit, windows: [] }, 'Codex')).toBeNull();
});

test('the cost of a run has two decimals, and no price is no cost', () => {
  const usage = { input_tokens: 1, output_tokens: 1, cache_read_tokens: 0, cache_write_tokens: 0 };
  expect(costText({ ...usage, cost_usd: 1.8432 })).toBe('$1.84');
  expect(costText({ ...usage, cost_usd: 0.004 })).toBe('$0.00');
  expect(costText(usage)).toBeNull();
});
