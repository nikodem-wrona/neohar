import { expect, test } from 'bun:test';
import { DRAG_THRESHOLD, isDrag } from './drag';

test('a press is a drag when the pointer moves the threshold in any direction', () => {
  expect(isDrag(0, 0)).toBe(false);
  expect(isDrag(DRAG_THRESHOLD - 1, 0)).toBe(false);
  expect(isDrag(2, 2)).toBe(false);
  expect(isDrag(DRAG_THRESHOLD, 0)).toBe(true);
  expect(isDrag(0, -DRAG_THRESHOLD)).toBe(true);
  expect(isDrag(-3, 3)).toBe(true);
});
