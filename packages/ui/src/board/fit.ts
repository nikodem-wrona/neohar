// How many strips the board draws. The page measures the board, and this module
// decides from the width alone: the core sends nothing new. When every run has a strip, the strips
// share the room of the board up to `MAX_STRIP_CH`, so a title is not cut while the board is empty.
// The width changes only with the number of runs and the width of the board, so a strip
// keeps its place when a state changes. The runs that do not fit go behind the `+N` cell.

/** The width of a strip, in characters of the data family of the chrome. */
export const STRIP_CH = 30;
/** The widest strip, when the board has room for every run. */
export const MAX_STRIP_CH = 48;
/** The width of a strip when the board is narrow: one line with the badge, the state and the work. */
export const NARROW_STRIP_CH = 22;
/** Under this width, in characters, the board is narrow. */
export const NARROW_BOARD_CH = 100;
/** The `+N` cell, wide enough for `+999`, with the gap before it. */
export const MORE_CH = 6;
/** The gap after each strip: the strips are cards on the board. */
export const GAP_CH = 1;

export interface Fit {
  /** The board is narrower than `NARROW_BOARD_CH`: one line, and narrow strips. */
  narrow: boolean;
  /** The width of each strip, in characters. */
  strip: number;
  /** The first `shown` runs have a strip. */
  shown: number;
  /** The runs behind the `+N` cell. */
  hidden: number;
}

/** The strips that fit a board of `width` characters, for `count` runs in the order of the core. */
export function fitStrips(count: number, width: number): Fit {
  const narrow = width < NARROW_BOARD_CH;
  const strip = narrow ? NARROW_STRIP_CH : STRIP_CH;
  const room = Math.max(0, Math.floor(width));
  const pitch = strip + GAP_CH;
  if (count * pitch <= room) {
    const share = narrow ? strip : Math.min(MAX_STRIP_CH, Math.floor(room / count) - GAP_CH);
    return { narrow, strip: Math.max(strip, share), shown: count, hidden: 0 };
  }
  const shown = Math.max(0, Math.min(count, Math.floor((room - MORE_CH) / pitch)));
  return { narrow, strip, shown, hidden: count - shown };
}

/**
 * Whether a run behind the `+N` cell waits for a person. `waiting` has one entry for each run, in
 * the order of the board. It marks a part of one list that the person cannot see, so it reads one
 * store and the layout.
 */
export function waitsBehind(waiting: readonly boolean[], shown: number): boolean {
  return waiting.slice(shown).some(Boolean);
}
