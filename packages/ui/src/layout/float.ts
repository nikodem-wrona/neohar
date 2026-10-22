// The rectangle of a floating pane, in fractions of the tab area (`pane.set_float_rect`). The
// core refuses a rectangle outside the tab or with a side below 0.05, so the drag stops there.

import type { Rect } from '@neohar/transport';

const MIN_SIDE = 0.05;

function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), high);
}

/** The rectangle after a move by `dx` and `dy`, which stays inside the tab. */
export function movedRect(start: Rect, dx: number, dy: number): Rect {
  return {
    ...start,
    x: clamp(start.x + dx, 0, 1 - start.w),
    y: clamp(start.y + dy, 0, 1 - start.h),
  };
}

/** The rectangle after the bottom right corner moves by `dw` and `dh`. */
export function resizedRect(start: Rect, dw: number, dh: number): Rect {
  return {
    ...start,
    w: clamp(start.w + dw, MIN_SIDE, 1 - start.x),
    h: clamp(start.h + dh, MIN_SIDE, 1 - start.y),
  };
}
