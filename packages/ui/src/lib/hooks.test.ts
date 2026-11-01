import { expect, test } from 'bun:test';
import { shallowEqual } from './hooks';

test('shallow equality compares the items of arrays and the values of objects', () => {
  const row = { id: 'a' };
  expect(shallowEqual([row, 1], [row, 1])).toBe(true);
  expect(shallowEqual([row], [{ id: 'a' }])).toBe(false);
  expect(shallowEqual({ a: 1, b: row }, { a: 1, b: row })).toBe(true);
  expect(shallowEqual({ a: 1 }, { a: 1, b: undefined })).toBe(false);
  expect(shallowEqual<unknown>(null, {})).toBe(false);
});
