import { expect, test } from 'bun:test';
import { dragRatios, MIN_SHARE, ratiosDiffer } from './divider';

test('a divider moves the share of its two neighbours and keeps their sum', () => {
  const next = dragRatios([0.5, 0.5], 0, 0.1);
  expect(next[0]).toBeCloseTo(0.6, 9);
  expect(next[1]).toBeCloseTo(0.4, 9);
  const three = dragRatios([0.2, 0.3, 0.5], 1, -0.1);
  expect(three[0]).toBe(0.2);
  expect(three[1]).toBeCloseTo(0.2, 9);
  expect(three[2]).toBeCloseTo(0.6, 9);
  expect(three.reduce((a, b) => a + b, 0)).toBeCloseTo(1, 9);
});

test('a neighbour keeps its smallest share', () => {
  const far = dragRatios([0.5, 0.5], 0, 5);
  expect(far[0]).toBeCloseTo(1 - MIN_SHARE, 9);
  expect(far[1]).toBeCloseTo(MIN_SHARE, 9);
  const back = dragRatios([0.5, 0.5], 0, -5);
  expect(back[0]).toBeCloseTo(MIN_SHARE, 9);
});

test('a divider that does not exist changes nothing', () => {
  expect(dragRatios([0.5, 0.5], 1, 0.1)).toEqual([0.5, 0.5]);
  expect(dragRatios([1], 0, 0.1)).toEqual([1]);
});

test('shares that differ by less than the core would notice are the same', () => {
  expect(ratiosDiffer([0.5, 0.5], [0.50001, 0.49999])).toBe(false);
  expect(ratiosDiffer([0.5, 0.5], [0.6, 0.4])).toBe(true);
  expect(ratiosDiffer([0.5, 0.5], [1])).toBe(true);
});
