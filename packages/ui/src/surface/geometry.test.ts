import { describe, expect, test } from 'bun:test';
import {
  ATLAS_SLOTS,
  atlasGeometry,
  cropOf,
  forEachRun,
  noCrop,
  type Radii,
  roundedDistance,
  slotPlace,
} from './geometry';

test('a page holds whole cells and the pages hold every slot', () => {
  const g = atlasGeometry(18, 36);
  expect(g.perRow).toBe(113);
  expect(g.pageW).toBe(113 * 18);
  expect(g.pageW).toBeLessThanOrEqual(2048);
  expect(g.pageH).toBeLessThanOrEqual(2048);
  expect(g.pages * g.perPage).toBeGreaterThanOrEqual(ATLAS_SLOTS);
  expect((g.pages - 1) * g.perPage).toBeLessThan(ATLAS_SLOTS);
});

test('a page is only as high as its slots need', () => {
  const g = atlasGeometry(10, 20, 100);
  expect(g.pages).toBe(1);
  // 204 slots fit a row of the page, so 100 slots need one row.
  expect(g.pageH).toBe(20);
});

test('a slot has one place, and no two slots share a place', () => {
  const g = atlasGeometry(9, 18, 5000);
  const seen = new Set<string>();
  const place = { page: 0, x: 0, y: 0 };
  for (let slot = 0; slot < 5000; slot++) {
    slotPlace(g, slot, place);
    expect(place.x % 9).toBe(0);
    expect(place.y % 18).toBe(0);
    expect(place.x + 9).toBeLessThanOrEqual(g.pageW);
    expect(place.y + 18).toBeLessThanOrEqual(g.pageH);
    seen.add(`${place.page}/${place.x}/${place.y}`);
  }
  expect(seen.size).toBe(5000);
});

test('slot 0 is the first cell of the first page, and the next page starts after perPage', () => {
  const g = atlasGeometry(40, 80, 4000);
  const place = { page: 0, x: 0, y: 0 };
  expect(slotPlace(g, 0, place)).toEqual({ page: 0, x: 0, y: 0 });
  expect(slotPlace(g, g.perPage, place)).toEqual({ page: 1, x: 0, y: 0 });
  expect(slotPlace(g, g.perPage + g.perRow + 1, place)).toEqual({ page: 1, x: 40, y: 80 });
});

test('a cell that does not fit a page is an error', () => {
  expect(() => atlasGeometry(3000, 10)).toThrow(RangeError);
});

test('forEachRun walks the runs of set rows in order', () => {
  const rows = Uint8Array.from([0, 1, 1, 0, 1, 0, 0, 1, 1, 1]);
  const runs: Array<[number, number]> = [];
  forEachRun(rows, rows.length, (first, count) => runs.push([first, count]));
  expect(runs).toEqual([
    [1, 2],
    [4, 1],
    [7, 3],
  ]);
  runs.length = 0;
  forEachRun(rows, 3, (first, count) => runs.push([first, count]));
  expect(runs).toEqual([[1, 2]]);
  forEachRun(new Uint8Array(5), 5, () => {
    throw new Error('no run expected');
  });
});

describe('the round shapes around the panes', () => {
  const box = { x: 10, y: 20, w: 200, h: 100 };
  const round: Radii = [0, 0, 16, 16];

  test('the distance is negative inside, zero on the edge and positive outside', () => {
    expect(roundedDistance(110, 70, box, round)).toBe(-50);
    expect(roundedDistance(10, 70, box, round)).toBe(0);
    expect(roundedDistance(5, 70, box, round)).toBe(5);
  });

  test('a square corner keeps its point, and a round one cuts it', () => {
    expect(roundedDistance(10, 20, box, round)).toBe(0);
    const corner = roundedDistance(210, 120, box, round);
    expect(corner).toBeCloseTo(16 * Math.SQRT2 - 16, 6);
  });
});

describe('the crop of a grid that is larger than its box', () => {
  const crop = (
    grid: [number, number],
    box: [number, number],
    cursor: [number, number, boolean],
    pan: [number, number] = [0, 0],
    base = noCrop(),
  ) =>
    cropOf(
      { cols: grid[0], rows: grid[1] },
      { cols: box[0], rows: box[1] },
      { x: cursor[0], y: cursor[1], visible: cursor[2] },
      { left: pan[0], top: pan[1] },
      base,
      noCrop(),
    );

  test('a grid that fits its box starts at the top left', () => {
    expect(crop([56, 20], [56, 20], [10, 19, true])).toEqual({ left: 0, top: 0 });
    expect(crop([40, 10], [56, 20], [39, 9, true], [5, 5])).toEqual({ left: 0, top: 0 });
  });

  test('the part ends at the row of the cursor, or at the last row when the cursor is near it', () => {
    // The grid of the Mac: 120 × 50. The box of the phone: 56 × 20.
    expect(crop([120, 50], [56, 20], [2, 30, true])).toEqual({ left: 0, top: 11 });
    expect(crop([120, 50], [56, 20], [2, 5, true])).toEqual({ left: 0, top: 0 });
    // The input box of an agent, with a status line under it.
    expect(crop([120, 50], [56, 20], [2, 46, true])).toEqual({ left: 0, top: 30 });
  });

  test('the part starts at column 0 until the cursor is to the right of the box', () => {
    expect(crop([120, 50], [56, 20], [55, 49, true])).toEqual({ left: 0, top: 30 });
    expect(crop([120, 50], [56, 20], [80, 49, true])).toEqual({ left: 25, top: 30 });
    expect(crop([120, 50], [56, 20], [119, 49, true])).toEqual({ left: 64, top: 30 });
  });

  test('a pan moves the part within the grid, and a hidden cursor keeps the part', () => {
    expect(crop([120, 50], [56, 20], [2, 30, true], [10, -5])).toEqual({ left: 10, top: 6 });
    expect(crop([120, 50], [56, 20], [2, 30, true], [-10, 100])).toEqual({ left: 0, top: 30 });
    const base = noCrop();
    crop([120, 50], [56, 20], [80, 30, true], [0, 0], base);
    expect(crop([120, 50], [56, 20], [0, 0, false], [0, 0], base)).toEqual({ left: 25, top: 11 });
  });
});
