// The scrollbar of a pane: the thumb is the part of the history that the viewport
// shows. These functions turn the viewport of a frame into a thumb and a drag into a line. The
// core owns the scrolling (`pane.scroll`).

/** The thumb of a scrollbar in pixels of the track. */
export interface Thumb {
  /** From the top of the track. */
  offset: number;
  size: number;
}

/**
 * The thumb for a viewport that shows `rows` lines from line `top` of `total` lines, on a track
 * of `track` pixels. It is `null` when everything fits and there is nothing to scroll. The thumb
 * is never smaller than `minSize`.
 */
export function thumbFor(
  top: number,
  total: number,
  rows: number,
  track: number,
  minSize = 24,
): Thumb | null {
  if (total <= rows || rows <= 0 || track <= 0) return null;
  const size = Math.min(track, Math.max(minSize, (rows / total) * track));
  const range = total - rows;
  const share = Math.min(Math.max(top / range, 0), 1);
  return { offset: share * (track - size), size };
}

/**
 * The first line of the viewport when the thumb sits `offset` pixels from the top of the track.
 * It is the inverse of [`thumbFor`], and it gives a whole line inside the history.
 */
export function lineAt(
  offset: number,
  total: number,
  rows: number,
  track: number,
  size: number,
): number {
  const range = total - rows;
  const room = track - size;
  if (range <= 0 || room <= 0) return 0;
  return Math.round(Math.min(Math.max(offset / room, 0), 1) * range);
}
