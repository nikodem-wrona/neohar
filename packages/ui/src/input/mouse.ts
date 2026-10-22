// Mouse and wheel events as the core wants them: cell coordinates, whole wheel
// notches and the mode of a selection. The page only turns pixels into cells. The core decides
// what a click means for a program that asked for the mouse, and what it means otherwise.

import type { ModName } from '@neohar/transport';

export interface Cell {
  col: number;
  row: number;
}

/** The part of a mouse event that the conversion reads. A real `MouseEvent` fits. */
export interface MouseLike {
  readonly clientX: number;
  readonly clientY: number;
  readonly button: number;
  readonly detail: number;
  readonly shiftKey: boolean;
  readonly ctrlKey: boolean;
  readonly altKey: boolean;
  readonly metaKey: boolean;
}

/**
 * The cell under a point. `box` is the box of the pane in CSS pixels, `cell` the size of a cell in
 * CSS pixels, and `grid` the size of the pane in cells. A point outside the box gives the nearest
 * cell, so that a drag that leaves the pane goes on to the edge.
 */
export function cellAt(
  point: { x: number; y: number },
  box: { left: number; top: number },
  cell: { w: number; h: number },
  grid: { cols: number; rows: number },
): Cell {
  const col = Math.floor((point.x - box.left) / cell.w);
  const row = Math.floor((point.y - box.top) / cell.h);
  return {
    col: Math.min(Math.max(col, 0), Math.max(grid.cols - 1, 0)),
    row: Math.min(Math.max(row, 0), Math.max(grid.rows - 1, 0)),
  };
}

/** The modifiers of a mouse or key event, in the order of `ModName`. */
export function modsOf(
  e: Pick<MouseLike, 'shiftKey' | 'ctrlKey' | 'altKey' | 'metaKey'>,
): ModName[] {
  const mods: ModName[] = [];
  if (e.shiftKey) mods.push('shift');
  if (e.ctrlKey) mods.push('ctrl');
  if (e.altKey) mods.push('alt');
  if (e.metaKey) mods.push('meta');
  return mods;
}

export type Button = 'left' | 'middle' | 'right' | 'none';

/** The name of `MouseEvent.button`. */
export function buttonName(button: number): Button {
  switch (button) {
    case 0:
      return 'left';
    case 1:
      return 'middle';
    case 2:
      return 'right';
    default:
      return 'none';
  }
}

export type SelectMode = 'simple' | 'word' | 'line' | 'block';

/** The mode of a selection that starts with a click: two clicks a word, three a line, Alt a block. */
export function selectMode(clicks: number, alt: boolean): SelectMode {
  if (alt) return 'block';
  if (clicks >= 3) return 'line';
  if (clicks === 2) return 'word';
  return 'simple';
}

/**
 * Whether a click makes a selection although the program asked for the mouse. The modifier is
 * Shift, as in other terminals.
 */
export function bypassesMouseMode(e: Pick<MouseLike, 'shiftKey'>): boolean {
  return e.shiftKey;
}

/** The distance of one wheel notch in CSS pixels, for a font with this cell height. */
export function pixelsPerNotch(cellHeight: number): number {
  return Math.max(cellHeight * 3, 1);
}

/**
 * Collects the small steps of a trackpad into whole notches (a wheel event has whole notches). A step that does not make a whole notch waits for the next one.
 */
export class WheelAccumulator {
  private x = 0;
  private y = 0;

  /**
   * Adds a wheel event and gives the whole notches that are ready. `mode` is
   * `WheelEvent.deltaMode`: 0 pixels, 1 lines, 2 pages.
   */
  add(deltaX: number, deltaY: number, mode: number, cellHeight: number): { x: number; y: number } {
    const unit = mode === 0 ? pixelsPerNotch(cellHeight) : mode === 1 ? 3 : 1;
    this.x += deltaX / unit;
    this.y += deltaY / unit;
    const ready = { x: Math.trunc(this.x), y: Math.trunc(this.y) };
    this.x -= ready.x;
    this.y -= ready.y;
    return ready;
  }

  reset(): void {
    this.x = 0;
    this.y = 0;
  }
}

/** The mouse protocol that a program asked for. */
export type MouseProtocol = 'none' | 'x10' | 'normal' | 'button_event' | 'any_event';

const MOUSE_X10 = 1 << 3;
const MOUSE_1000 = 1 << 4;
const MOUSE_1002 = 1 << 5;
const MOUSE_1003 = 1 << 6;

/**
 * The mouse protocol from the mode flags of a frame (`nh_proto::ModeFlags`). The frame is the
 * state of the screen that the user sees, so the page reads the protocol from it and not from the
 * `panes` projection, which can be a moment older.
 */
export function mouseProtocol(modes: number): MouseProtocol {
  if (modes & MOUSE_1003) return 'any_event';
  if (modes & MOUSE_1002) return 'button_event';
  if (modes & MOUSE_1000) return 'normal';
  if (modes & MOUSE_X10) return 'x10';
  return 'none';
}
