import { expect, test } from 'bun:test';
import { afterPatch, emptyState, fromSnapshot, values } from './reducer';

const snapshot = fromSnapshot<{ n: number }>(
  { items: { a: { n: 1 }, b: { n: 2 } }, order: ['a', 'b'] },
  4,
);

test('a snapshot gives the rows, their order and the version', () => {
  expect(snapshot.version).toBe(4);
  expect(snapshot.status).toBe('live');
  expect(values(snapshot)).toEqual([{ n: 1 }, { n: 2 }]);
  expect(emptyState().status).toBe('loading');
  expect(emptyState().version).toBeNull();
});

test('set changes a row and adds a new row at the end', () => {
  const next = afterPatch(
    snapshot,
    [
      { op: 'set', key: 'a', value: { n: 10 } },
      { op: 'set', key: 'c', value: { n: 3 } },
    ],
    5,
  );
  expect(next.version).toBe(5);
  expect(next.order).toEqual(['a', 'b', 'c']);
  expect(values(next)).toEqual([{ n: 10 }, { n: 2 }, { n: 3 }]);
  // The old state stays as it was.
  expect(values(snapshot)).toEqual([{ n: 1 }, { n: 2 }]);
});

test('remove drops a row and its place in the order', () => {
  const next = afterPatch(snapshot, [{ op: 'remove', key: 'a' }], 5);
  expect(next.order).toEqual(['b']);
  expect('a' in next.byKey).toBe(false);
});

test('order puts the named keys first and keeps the others after them', () => {
  const three = afterPatch(snapshot, [{ op: 'set', key: 'c', value: { n: 3 } }], 5);
  const next = afterPatch(three, [{ op: 'order', keys: ['c', 'a', 'gone'] }], 6);
  expect(next.order).toEqual(['c', 'a', 'b']);
});

test('a patch with a set, a remove and an order in one message', () => {
  const next = afterPatch(
    snapshot,
    [
      { op: 'remove', key: 'a' },
      { op: 'set', key: 'z', value: { n: 26 } },
      { op: 'order', keys: ['z', 'b'] },
    ],
    9,
  );
  expect(next.order).toEqual(['z', 'b']);
  expect(next.version).toBe(9);
});
