import { expect, test } from 'bun:test';
import { revealed } from './reveal';

test('a row without a request asks for nothing', () => {
  expect(revealed(0, undefined)).toEqual({ seen: 0, show: null });
});

test('a new number shows the card once', () => {
  const first = revealed(0, { interaction: 'i1', seq: 1 });
  expect(first).toEqual({ seen: 1, show: 'i1' });
  expect(revealed(first.seen, { interaction: 'i1', seq: 1 })).toEqual({ seen: 1, show: null });
});

test('the same card asked for again shows again', () => {
  const again = revealed(1, { interaction: 'i1', seq: 2 });
  expect(again).toEqual({ seen: 2, show: 'i1' });
});
