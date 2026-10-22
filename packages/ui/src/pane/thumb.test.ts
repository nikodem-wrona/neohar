import { expect, test } from 'bun:test';
import { lineAt, thumbFor } from './thumb';

test('everything fits: no thumb', () => {
  expect(thumbFor(0, 24, 24, 400)).toBeNull();
  expect(thumbFor(0, 10, 24, 400)).toBeNull();
  expect(thumbFor(0, 100, 24, 0)).toBeNull();
});

test('the thumb is the share of the history that the viewport shows', () => {
  // 24 of 240 lines on a track of 400 pixels: a tenth of the track.
  expect(thumbFor(0, 240, 24, 400)).toEqual({ offset: 0, size: 40 });
  // At the end the thumb touches the bottom of the track.
  const end = thumbFor(216, 240, 24, 400);
  expect(end?.offset).toBeCloseTo(360, 9);
  const middle = thumbFor(108, 240, 24, 400);
  expect(middle?.offset).toBeCloseTo(180, 9);
});

test('a long history does not make the thumb vanish', () => {
  expect(thumbFor(0, 100_000, 24, 400)?.size).toBe(24);
  expect(thumbFor(0, 100_000, 24, 20)?.size).toBe(20);
});

test('the line for a thumb position is the inverse of the position of the thumb', () => {
  for (const top of [0, 1, 50, 107, 216]) {
    const thumb = thumbFor(top, 240, 24, 400);
    expect(thumb).not.toBeNull();
    if (!thumb) continue;
    expect(lineAt(thumb.offset, 240, 24, 400, thumb.size)).toBe(top);
  }
});

test('a position outside the track gives the first or the last line', () => {
  expect(lineAt(-50, 240, 24, 400, 40)).toBe(0);
  expect(lineAt(5000, 240, 24, 400, 40)).toBe(216);
  expect(lineAt(10, 24, 24, 400, 40)).toBe(0);
});
