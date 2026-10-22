import { expect, test } from 'bun:test';
import { hitSegments, paramsOf } from './model';

test('no words, or a scope without a place, gives no query', () => {
  expect(paramsOf('   ', 'all', {})).toBeNull();
  expect(paramsOf('error', 'pane', {})).toBeNull();
  expect(paramsOf('error', 'workspace', { pane: 'p1' })).toBeNull();
});

test('the scope becomes the pane or the workspace of the window', () => {
  expect(paramsOf('error', 'all', {})).toEqual({ query: 'error', limit: 50 });
  expect(paramsOf('error', 'pane', { pane: 'p1', workspace: 'w1' })).toEqual({
    query: 'error',
    limit: 50,
    pane: 'p1',
  });
  expect(paramsOf('error', 'workspace', { pane: 'p1', workspace: 'w1' })).toEqual({
    query: 'error',
    limit: 50,
    workspace: 'w1',
  });
});

test('the text is cut at the ranges that the core marked as matches', () => {
  const parts = hitSegments({
    text: 'error: migration failed',
    spans: [
      [7, 16, 'match'],
      [0, 5, 'other'],
    ],
  });
  expect(parts).toEqual([
    { text: 'error: ', hit: false },
    { text: 'migration', hit: true },
    { text: ' failed', hit: false },
  ]);
  expect(hitSegments({ text: 'plain', spans: [] })).toEqual([{ text: 'plain', hit: false }]);
});
