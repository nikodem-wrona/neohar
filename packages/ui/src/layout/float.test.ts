import { expect, test } from 'bun:test';
import { movedRect, resizedRect } from './float';

const rect = { x: 0.2, y: 0.2, w: 0.5, h: 0.5 };

test('a move keeps the size and stays inside the tab', () => {
  expect(movedRect(rect, 0.1, -0.1)).toEqual({ x: 0.30000000000000004, y: 0.1, w: 0.5, h: 0.5 });
  expect(movedRect(rect, 5, 5)).toEqual({ x: 0.5, y: 0.5, w: 0.5, h: 0.5 });
  expect(movedRect(rect, -5, -5)).toEqual({ x: 0, y: 0, w: 0.5, h: 0.5 });
});

test('a resize keeps the corner at the top left and stays inside the tab', () => {
  expect(resizedRect(rect, 0.1, 0.1)).toEqual({ x: 0.2, y: 0.2, w: 0.6, h: 0.6 });
  expect(resizedRect(rect, 5, 5)).toEqual({ x: 0.2, y: 0.2, w: 0.8, h: 0.8 });
  expect(resizedRect(rect, -5, -5)).toEqual({ x: 0.2, y: 0.2, w: 0.05, h: 0.05 });
});
