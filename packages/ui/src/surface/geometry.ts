// The layout of the glyph atlas, the round shapes around the panes, and helpers that need no
// browser.

import type { Rect } from './types';

/**
 * The number of slots of the atlas. The decoder is made with this number (`createDecoder`), so
 * it is fixed for the life of a window. When the atlas is full, the decoder and the renderer
 * reset it together.
 */
export const ATLAS_SLOTS = 8192;

/** The largest width or height of a page of the atlas, in pixels. Every GPU has at least this. */
export const MAX_PAGE_PX = 2048;

/** The width of the table that tells for each slot whether its glyph is in colour. */
export const KIND_WIDTH = 128;

/** Where the slots are, for cells of one size. */
export interface AtlasGeometry {
  cellW: number;
  cellH: number;
  slots: number;
  /** Slots in a row of a page. */
  perRow: number;
  /** Slots in a page. */
  perPage: number;
  /** The number of pages (layers of the texture). */
  pages: number;
  pageW: number;
  pageH: number;
}

/**
 * The layout for cells of `cellW` × `cellH`. A page holds whole cells and is at most `maxPx` wide
 * and high. The last row of the last page may be partly empty.
 */
export function atlasGeometry(
  cellW: number,
  cellH: number,
  slots: number = ATLAS_SLOTS,
  maxPx: number = MAX_PAGE_PX,
): AtlasGeometry {
  const perRow = Math.floor(maxPx / cellW);
  const rowsPerPage = Math.floor(maxPx / cellH);
  if (perRow < 1 || rowsPerPage < 1) {
    throw new RangeError(`a cell of ${cellW}x${cellH} does not fit a page of ${maxPx}px`);
  }
  const perPage = perRow * rowsPerPage;
  const pages = Math.ceil(slots / perPage);
  const rowsInPage = Math.min(rowsPerPage, Math.ceil(slots / perRow));
  return {
    cellW,
    cellH,
    slots,
    perRow,
    perPage,
    pages,
    pageW: perRow * cellW,
    pageH: rowsInPage * cellH,
  };
}

/** The place of a slot in the atlas. */
export interface SlotPlace {
  page: number;
  x: number;
  y: number;
}

/** Writes the place of `slot` to `out`. It allocates nothing. */
export function slotPlace(g: AtlasGeometry, slot: number, out: SlotPlace): SlotPlace {
  const page = Math.floor(slot / g.perPage);
  const index = slot - page * g.perPage;
  out.page = page;
  out.x = (index % g.perRow) * g.cellW;
  out.y = Math.floor(index / g.perRow) * g.cellH;
  return out;
}

/**
 * Calls `run(first, count)` for each run of set bytes in `rows[0..length)`, in order. The caller
 * passes the same `run` function every frame, so the walk allocates nothing.
 */
export function forEachRun(
  rows: Uint8Array,
  length: number,
  run: (first: number, count: number) => void,
): void {
  let y = 0;
  while (y < length) {
    if (rows[y] === 0) {
      y++;
      continue;
    }
    const first = y;
    while (y < length && rows[y] !== 0) y++;
    run(first, y - first);
  }
}

/** The radius of each corner of a shape: top left, top right, bottom right, bottom left. */
export type Radii = readonly [number, number, number, number];

/**
 * The signed distance from the point (`px`, `py`) to the edge of a rounded rectangle:
 * negative inside, positive outside. The shader of the shapes (`gl/shaders.ts`) has the same
 * formula, so a test can check it here.
 */
export function roundedDistance(px: number, py: number, rect: Rect, radii: Radii): number {
  const hw = rect.w / 2;
  const hh = rect.h / 2;
  const x = px - (rect.x + hw);
  const y = py - (rect.y + hh);
  const r = x < 0 ? (y < 0 ? radii[0] : radii[3]) : y < 0 ? radii[1] : radii[2];
  const qx = Math.abs(x) - hw + r;
  const qy = Math.abs(y) - hh + r;
  const outside = Math.hypot(Math.max(qx, 0), Math.max(qy, 0));
  return Math.min(Math.max(qx, qy), 0) + outside - r;
}

/** The first column and the first row of the grid that a box shows. */
export interface Crop {
  left: number;
  top: number;
}

/** The crop of a pane that the renderer does not know. */
export const NO_CROP: Readonly<Crop> = Object.freeze({ left: 0, top: 0 });

/** A crop of nothing: the grid starts at the top left of its box. */
export function noCrop(): Crop {
  return { left: 0, top: 0 };
}

/**
 * The first cell of one axis of a crop. The part ends at the cursor, or at the end of the grid
 * when the cursor is in the last quarter of a box from the end. `near` is false for the columns:
 * they start at 0 until the cursor is to the right of the box.
 */
function cropStart(size: number, box: number, cursor: number, near: boolean): number {
  const most = size - box;
  if (most <= 0) return 0;
  if (near && size - 1 - cursor <= Math.floor(box / 4)) return most;
  return Math.min(Math.max(cursor + 1 - box, 0), most);
}

/**
 * The part of a grid that a box of `box` cells shows, when the grid is larger because another
 * window has the size of the PTY. Claude Code and Codex keep the cursor
 * in their input box at the bottom, so the part ends at the row of the cursor, or at the last row
 * when the cursor is near it. It starts at column 0, unless the cursor is to the right of the box.
 * A hidden cursor keeps the part of `base`. `pan` moves the part, within the grid. The result goes
 * to `out`, and `base` gets the part before the pan, so a frame allocates nothing.
 */
export function cropOf(
  grid: { cols: number; rows: number },
  box: { cols: number; rows: number },
  cursor: { x: number; y: number; visible: boolean },
  pan: Crop,
  base: Crop,
  out: Crop,
): Crop {
  if (cursor.visible) {
    base.left = cropStart(grid.cols, box.cols, cursor.x, false);
    base.top = cropStart(grid.rows, box.rows, cursor.y, true);
  }
  const mostLeft = Math.max(grid.cols - box.cols, 0);
  const mostTop = Math.max(grid.rows - box.rows, 0);
  out.left = Math.min(Math.max(Math.min(base.left, mostLeft) + pan.left, 0), mostLeft);
  out.top = Math.min(Math.max(Math.min(base.top, mostTop) + pan.top, 0), mostTop);
  return out;
}

/**
 * The crop of one pane of a renderer: the pan that the page sets and the part of the last paint.
 * The objects live as long as the pane, so a paint allocates nothing.
 */
export class PaneCrop {
  /** The part of the grid that the last paint showed. */
  readonly crop: Crop = noCrop();
  /** The move that the page asked for, in cells (two fingers on a phone). */
  readonly pan: Crop = noCrop();
  private readonly base: Crop = noCrop();
  private readonly grid = { cols: 0, rows: 0 };
  private readonly box = { cols: 0, rows: 0 };
  private readonly cursor = { x: 0, y: 0, visible: false };

  /**
   * The part that a box of `rect` shows, for cells of `cellW` × `cellH` device pixels. Returns
   * whether the part changed since the last call.
   */
  place(
    rect: Rect,
    cellW: number,
    cellH: number,
    grid: { cols: number; rows: number },
    cursor: { x: number; y: number; visible: boolean },
  ): boolean {
    const left = this.crop.left;
    const top = this.crop.top;
    this.box.cols = Math.max(Math.floor(rect.w / cellW), 1);
    this.box.rows = Math.max(Math.floor(rect.h / cellH), 1);
    this.grid.cols = grid.cols;
    this.grid.rows = grid.rows;
    this.cursor.x = cursor.x;
    this.cursor.y = cursor.y;
    this.cursor.visible = cursor.visible;
    cropOf(this.grid, this.box, this.cursor, this.pan, this.base, this.crop);
    return this.crop.left !== left || this.crop.top !== top;
  }
}
