import { expect, test } from 'bun:test';
import { goToPane, showPane } from './show';

test('a pane is shown by the command of the palette', () => {
  expect(goToPane('01ABC')).toBe('go.pane:01ABC');
});

test('showing a pane sends one command of the palette for the window', async () => {
  const calls: [string, unknown][] = [];
  const transport = {
    call: ((method: string, params: unknown) => {
      calls.push([method, params]);
      return Promise.resolve({ ui: null });
    }) as never,
  };
  await showPane(transport, 'win-1', '01ABC');
  expect(calls).toEqual([['palette.execute', { command: 'go.pane:01ABC', window: 'win-1' }]]);
});
