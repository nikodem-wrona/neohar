import { describe, expect, test } from 'bun:test';
import {
  fitStrips,
  GAP_CH,
  MAX_STRIP_CH,
  MORE_CH,
  NARROW_BOARD_CH,
  NARROW_STRIP_CH,
  STRIP_CH,
  waitsBehind,
} from './fit';

/** A strip and the gap after it. The board has no padding and nothing at its end. */
const PITCH = STRIP_CH + GAP_CH;

describe('fitStrips', () => {
  test('a board that has room for every run draws them all, with no +N cell', () => {
    const width = 4 * PITCH;
    expect(fitStrips(4, width)).toEqual({ narrow: false, strip: STRIP_CH, shown: 4, hidden: 0 });
  });

  test('a board with room to spare shares it, up to the widest strip', () => {
    expect(fitStrips(2, 300)).toMatchObject({ strip: MAX_STRIP_CH, shown: 2, hidden: 0 });
    // Four runs on a board with room for 40 characters each, gap included.
    expect(fitStrips(4, 4 * 41)).toMatchObject({ strip: 40, shown: 4, hidden: 0 });
    // A narrow board keeps its narrow strips.
    expect(fitStrips(1, NARROW_BOARD_CH - 1).strip).toBe(NARROW_STRIP_CH);
  });

  test('one character less puts the runs that do not fit behind +N, which takes its room', () => {
    const width = 4 * PITCH - 1;
    // Three strips and the +N cell fit; the fourth run goes behind it.
    expect(fitStrips(4, width)).toEqual({ narrow: false, strip: STRIP_CH, shown: 3, hidden: 1 });
  });

  test('the +N cell takes room from the strips', () => {
    const width = 4 * PITCH + MORE_CH - 1;
    expect(fitStrips(6, width)).toMatchObject({ shown: 3, hidden: 3 });
    expect(fitStrips(6, width + 1)).toMatchObject({ shown: 4, hidden: 2 });
  });

  test('a board with room for no strip puts every run behind +N', () => {
    expect(fitStrips(3, MORE_CH)).toMatchObject({ shown: 0, hidden: 3 });
    expect(fitStrips(3, 0)).toMatchObject({ shown: 0, hidden: 3 });
  });

  test('no run, no strip and no +N cell', () => {
    expect(fitStrips(0, 200)).toMatchObject({ shown: 0, hidden: 0 });
  });

  test('a narrow board has narrow strips', () => {
    const fit = fitStrips(3, NARROW_BOARD_CH - 1);
    expect(fit.narrow).toBe(true);
    expect(fit.strip).toBe(NARROW_STRIP_CH);
    expect(fit.shown).toBe(3);
    expect(fitStrips(3, NARROW_BOARD_CH).narrow).toBe(false);
  });

  test('each strip takes its gap: a board as wide as the strips alone has room for one less', () => {
    expect(fitStrips(4, 4 * STRIP_CH)).toMatchObject({ shown: 3, hidden: 1 });
  });

  test('a fraction of a character does not make room', () => {
    expect(fitStrips(4, 4 * PITCH - 0.5)).toMatchObject({ shown: 3, hidden: 1 });
  });
});

describe('waitsBehind', () => {
  test('a waiting run behind +N marks the cell', () => {
    expect(waitsBehind([false, false, true], 2)).toBe(true);
  });

  test('a waiting run in view does not mark the cell', () => {
    expect(waitsBehind([true, false, false], 2)).toBe(false);
  });

  test('with every run in view, nothing is behind the cell', () => {
    expect(waitsBehind([true, true], 2)).toBe(false);
  });
});
