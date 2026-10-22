// The refresh rate of the display, from the animation frames of the page. The core paces the
// frames for a window with this rate (`window.focus_report`).

import type { Frames } from '../lib/frames';

/**
 * The rate that the intervals between frames show. A display makes frames at its rate at best,
 * and a busy page makes them later, so a late frame says nothing about the display. The page takes
 * the interval at the lower quartile, in Hz and rounded.
 */
export function estimateHz(intervalsMs: readonly number[]): number | undefined {
  const usable = intervalsMs.filter((ms) => ms > 1 && ms < 250);
  if (usable.length < 3) return undefined;
  // The element with the rank at the lower quartile. The UI does not sort, and the list
  // is short.
  const rank = Math.floor(usable.length / 4);
  const quartile = usable.find(
    (value, index) =>
      usable.filter((other, at) => other < value || (other === value && at < index)).length ===
      rank,
  );
  return quartile ? Math.round(1000 / quartile) : undefined;
}

/** Waits for `count` frames and gives the rate that they show. */
export function sampleRefresh(
  frames: Frames,
  now: () => number,
  count = 16,
): Promise<number | undefined> {
  return new Promise((resolve) => {
    const intervals: number[] = [];
    let last: number | undefined;
    const tick = (): void => {
      const at = now();
      if (last !== undefined) intervals.push(at - last);
      last = at;
      if (intervals.length >= count) resolve(estimateHz(intervals));
      else frames.request(tick);
    };
    frames.request(tick);
  });
}
