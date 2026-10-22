// From the selection of the core to cells of the visible grid.

import type { SelectionCells, SelectionLines } from './types';

export const SELECT_NONE = 0;
export const SELECT_STREAM = 1;
export const SELECT_RECT = 2;

/** Modes of `SelectionLines.mode`. */
const MODE_LINE = 3;
const MODE_BLOCK = 4;

/**
 * A selection as rows of the visible grid. Rows are counted from the first visible row and are
 * kept between -1 (on or above the row before the first) and the number of rows (on or below
 * the last).
 */
export interface SelectionRows {
  mode: number;
  startCol: number;
  startRow: number;
  endCol: number;
  endRow: number;
}

export function emptyRows(): SelectionRows {
  return { mode: 0, startCol: 0, startRow: 0, endCol: 0, endRow: 0 };
}

/**
 * Writes to `out` the rows of a selection in lines of the scrollback, for a grid of `rows` rows
 * whose first row is line `topLine`.
 */
export function linesToRows(
  selection: SelectionLines | null,
  topLine: number,
  rows: number,
  out: SelectionRows,
): SelectionRows {
  if (!selection) {
    out.mode = 0;
    return out;
  }
  out.mode = selection.mode;
  out.startCol = selection.startCol;
  out.endCol = selection.endCol;
  out.startRow = clampRow(selection.startLine - topLine, rows);
  out.endRow = clampRow(selection.endLine - topLine, rows);
  return out;
}

/**
 * Writes to `out` the cells that a selection covers in a grid of `cols` × `rows`. The corners may
 * lie outside the grid (the selection may reach into the scrollback); they are kept, clamped to
 * a range that is safe in a shader.
 *
 * The core sends the points of the selection after it applied the mode (a word, a line), so the
 * renderer draws a stream of cells between them. Two modes need more: a line selection covers
 * whole rows, and a block selection is a rectangle.
 */
export function rowsToCells(
  selection: SelectionRows,
  cols: number,
  rows: number,
  out: SelectionCells,
): SelectionCells {
  if (selection.mode === 0) {
    out.mode = SELECT_NONE;
    return out;
  }
  let startCol = selection.startCol;
  let startRow = selection.startRow;
  let endCol = selection.endCol;
  let endRow = selection.endRow;
  // Order the points: first the row, then the column. A swap through an array would allocate.
  if (endRow < startRow || (endRow === startRow && endCol < startCol)) {
    const col = startCol;
    const row = startRow;
    startCol = endCol;
    startRow = endRow;
    endCol = col;
    endRow = row;
  }
  if (selection.mode === MODE_BLOCK) {
    out.mode = SELECT_RECT;
    out.startCol = Math.min(startCol, endCol);
    out.endCol = Math.max(startCol, endCol);
  } else if (selection.mode === MODE_LINE) {
    out.mode = SELECT_STREAM;
    out.startCol = 0;
    out.endCol = cols - 1;
  } else {
    out.mode = SELECT_STREAM;
    // A stream that starts above the grid starts at its first cell, and one that ends below it
    // ends at its last cell.
    out.startCol = startRow < 0 ? 0 : startCol;
    out.endCol = endRow >= rows ? cols - 1 : endCol;
  }
  out.startRow = startRow;
  out.endRow = endRow;
  return out;
}

/** A row number, kept between -1 and `rows` so that it is a small integer. */
function clampRow(row: number, rows: number): number {
  return Math.max(-1, Math.min(rows, row));
}

export function emptySelection(): SelectionCells {
  return { mode: SELECT_NONE, startCol: 0, startRow: 0, endCol: 0, endRow: 0 };
}
