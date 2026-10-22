import { expect, test } from 'bun:test';
import { hasColor } from './atlas';

const pixels = (...rgba: number[]) => Uint8ClampedArray.from(rgba);

test('white text, with any coverage, is not colour', () => {
  expect(hasColor(pixels(255, 255, 255, 255, 255, 255, 255, 90, 0, 0, 0, 0))).toBe(false);
});

test('a pixel with a colour of its own is colour', () => {
  expect(hasColor(pixels(255, 255, 255, 255, 220, 40, 30, 255))).toBe(true);
});

test('nearly transparent pixels do not count', () => {
  expect(hasColor(pixels(10, 20, 30, 12))).toBe(false);
  expect(hasColor(new Uint8ClampedArray(0))).toBe(false);
});
