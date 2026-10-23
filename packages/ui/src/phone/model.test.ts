import { describe, expect, test } from 'bun:test';
import type { AgentValue, TimelineItem } from '@neohar/transport';
import { replyState } from '../drawer/group';
import { groupByTurn, lastMessage } from '../timeline/model';
import { hashOf, pairCodeOf, type PhoneRoute, routeOf } from './model';

const RUN = '01M4F1W7EM2GN3EV6NHXZJEGGA';

/** A run as the core publishes it, with the parts a test changes. */
function run(change: {
  id?: string;
  execution?: string;
  pending?: string[];
  pane?: string | null;
}) {
  const provenance = { source: 'structured', confidence: 1, at: 'now' };
  return {
    id: change.id ?? RUN,
    harness: 'claude',
    pane: change.pane === undefined ? '01M4F1W7EM2GN3EV6NHXZJEGGB' : change.pane,
    facets: {
      execution: { value: { state: change.execution ?? 'idle' }, provenance },
      pending: change.pending ?? [],
    },
  } as unknown as AgentValue;
}

function item(id: string, turn: number, type: 'prompt' | 'assistant' | 'tool'): TimelineItem {
  const body =
    type === 'tool'
      ? { type, tool: 'Bash', summary: 'ls', status: 'ok' as const }
      : { type, preview: `${type} ${id}` };
  return { id, turn, at: 'now', body } as TimelineItem;
}

describe('the route of the hash', () => {
  test('the views of the desk that an address names', () => {
    expect(routeOf(`#/run/${RUN}`)).toEqual({ kind: 'run', run: RUN });
    expect(routeOf(`#/pane/${RUN}`)).toEqual({ kind: 'pane', pane: RUN });
    expect(routeOf(`#/interaction/${RUN}`)).toEqual({ kind: 'interaction', interaction: RUN });
    expect(routeOf('#/settings')).toEqual({ kind: 'settings', section: null });
    expect(routeOf('#/settings/phone')).toEqual({ kind: 'settings', section: 'phone' });
    expect(routeOf('#/machine/studio')).toEqual({ kind: 'machine', machine: 'studio' });
    expect(routeOf('#/machine/local')).toEqual({ kind: 'machine', machine: 'local' });
  });

  test('an address that the desk does not know shows the desk', () => {
    for (const hash of [
      '',
      '#/',
      '#/run/not-a-run',
      `#/run/${RUN}/more`,
      '#/other',
      `#/task/${RUN}`,
      '#/settings/A!',
      '#/machine/',
      '#/machine/Studio',
      `#/machine/${'a'.repeat(33)}`,
    ]) {
      expect(routeOf(hash)).toEqual({ kind: 'desk' });
    }
  });

  test('the hash of each route gives the route again', () => {
    const routes: PhoneRoute[] = [
      { kind: 'desk' },
      { kind: 'run', run: RUN },
      { kind: 'pane', pane: RUN },
      { kind: 'interaction', interaction: RUN },
      { kind: 'settings', section: null },
      { kind: 'settings', section: 'phone' },
      { kind: 'machine', machine: 'build-1' },
    ];
    for (const route of routes) expect(routeOf(hashOf(route))).toEqual(route);
  });
});

describe('the box Reply', () => {
  test('shows for a run that is idle with no open interaction', () => {
    expect(replyState(run({ execution: 'idle' }))).toBe('ready');
  });

  test('does not show while the run works', () => {
    expect(replyState(run({ execution: 'working' }))).toBe('working');
    expect(replyState(run({ execution: 'starting' }))).toBe('working');
  });

  test('does not show while an interaction waits, because its card answers it', () => {
    expect(replyState(run({ execution: 'idle', pending: ['i1'] }))).toBe('waiting');
    expect(replyState(run({ execution: 'working', pending: ['i1'] }))).toBe('waiting');
  });

  test('does not show for a run that ended, has no pane or is not known', () => {
    expect(replyState(run({ execution: 'exited' }))).toBe('gone');
    expect(replyState(run({ pane: null }))).toBe('gone');
    expect(replyState(undefined)).toBe('gone');
  });
});

/** The last turn of the items, as the timeline panel takes it. */
const lastTurn = (items: readonly TimelineItem[]) => groupByTurn(items).at(-1);

describe('the last turn', () => {
  test('is the last group of items, and its last message is the last assistant item', () => {
    const items = [
      item('p1', 1, 'prompt'),
      item('a1', 1, 'assistant'),
      item('p2', 2, 'prompt'),
      item('a2', 2, 'assistant'),
      item('t2', 2, 'tool'),
      item('a3', 2, 'assistant'),
      item('t3', 2, 'tool'),
    ];
    const turn = lastTurn(items);
    expect(turn?.turn).toBe(2);
    expect(turn?.items.map((i) => i.id)).toEqual(['p2', 'a2', 't2', 'a3', 't3']);
    expect(lastMessage(turn)?.id).toBe('a3');
  });

  test('has no message when the agent did not write yet', () => {
    expect(lastTurn([])).toBeUndefined();
    expect(lastMessage(undefined)).toBeUndefined();
    expect(lastMessage(lastTurn([item('p1', 1, 'prompt'), item('t1', 1, 'tool')]))).toBeUndefined();
  });
});

test('the code of a pairing comes from the query', () => {
  expect(pairCodeOf('?code=abc_DEF-1')).toBe('abc_DEF-1');
  expect(pairCodeOf('?x=1&code=%20K7M2%20')).toBe('K7M2');
  expect(pairCodeOf('?code=')).toBeNull();
  expect(pairCodeOf('')).toBeNull();
});
