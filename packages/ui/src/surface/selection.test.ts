import { expect, test } from 'bun:test';
import {
  emptyRows,
  emptySelection,
  linesToRows,
  rowsToCells,
  SELECT_NONE,
  SELECT_RECT,
  SELECT_STREAM,
} from './selection';
import type { SelectionLines } from './types';

const sel = (
  mode: number,
  startCol: number,
  startLine: number,
  endCol: number,
  endLine: number,
): SelectionLines => ({ mode, startCol, startLine, endCol, endLine });

/** The cells of a selection in lines of the scrollback: `linesToRows`, then `rowsToCells`. */
const cells = (s: SelectionLines | null, top = 100, cols = 80, rows = 24) =>
  structuredClone(
    rowsToCells(linesToRows(s, top, rows, emptyRows()), cols, rows, emptySelection()),
  );

test('no selection gives mode none', () => {
  expect(cells(null).mode).toBe(SELECT_NONE);
  expect(cells(sel(0, 1, 1, 1, 1)).mode).toBe(SELECT_NONE);
});

test('a stream between two points in view keeps the points', () => {
  expect(cells(sel(1, 5, 102, 9, 104))).toEqual({
    mode: SELECT_STREAM,
    startCol: 5,
    startRow: 2,
    endCol: 9,
    endRow: 4,
  });
});

test('points in the wrong order are put in order', () => {
  expect(cells(sel(1, 9, 104, 5, 102))).toEqual(cells(sel(1, 5, 102, 9, 104)));
  expect(cells(sel(1, 9, 103, 5, 103))).toEqual({
    mode: SELECT_STREAM,
    startCol: 5,
    startRow: 3,
    endCol: 9,
    endRow: 3,
  });
});

test('a stream that starts above the grid starts at its first cell', () => {
  const c = cells(sel(1, 30, 90, 4, 101));
  expect([c.startRow, c.startCol, c.endRow, c.endCol]).toEqual([-1, 0, 1, 4]);
});

test('a stream that ends below the grid ends at its last cell', () => {
  const c = cells(sel(1, 30, 120, 4, 900));
  expect([c.startRow, c.startCol, c.endRow, c.endCol]).toEqual([20, 30, 24, 79]);
});

test('a selection that is wholly outside the grid covers no row of it', () => {
  const above = cells(sel(1, 3, 10, 8, 20));
  expect([above.startRow, above.endRow]).toEqual([-1, -1]);
  const below = cells(sel(1, 3, 500, 8, 520));
  expect([below.startRow, below.endRow]).toEqual([24, 24]);
});

test('a line selection covers whole rows', () => {
  expect(cells(sel(3, 12, 102, 3, 104))).toEqual({
    mode: SELECT_STREAM,
    startCol: 0,
    startRow: 2,
    endCol: 79,
    endRow: 4,
  });
});

test('a block selection is a rectangle, whatever the order of the corners', () => {
  expect(cells(sel(4, 30, 102, 10, 106))).toEqual({
    mode: SELECT_RECT,
    startCol: 10,
    startRow: 2,
    endCol: 30,
    endRow: 6,
  });
});

test('lines become rows of the grid, kept between -1 and the number of rows', () => {
  const rows = (s: SelectionLines | null) => structuredClone(linesToRows(s, 100, 24, emptyRows()));
  expect(rows(sel(1, 5, 102, 9, 104))).toEqual({
    mode: 1,
    startCol: 5,
    startRow: 2,
    endCol: 9,
    endRow: 4,
  });
  expect(rows(sel(1, 5, 3, 9, 1e9))).toMatchObject({ startRow: -1, endRow: 24 });
  expect(rows(null).mode).toBe(0);
});
