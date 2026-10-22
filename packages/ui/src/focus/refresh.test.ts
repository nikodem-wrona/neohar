import { expect, test } from 'bun:test';
import type { Frames } from '../lib/frames';
import { estimateHz, sampleRefresh } from './refresh';

test('the median interval gives the rate', () => {
  expect(estimateHz(Array(10).fill(16.67))).toBe(60);
  expect(estimateHz(Array(10).fill(8.33))).toBe(120);
  // A slow frame or two does not change the rate: a busy page makes late frames, not early ones.
  expect(estimateHz([16.7, 16.6, 50, 16.7, 33, 16.7, 16.6])).toBe(60);
  expect(estimateHz([40, 41, 120, 16.7, 55, 16.7, 90, 16.6])).toBe(60);
});

test('too few or odd intervals give no rate', () => {
  expect(estimateHz([16.7, 16.7])).toBeUndefined();
  expect(estimateHz([0.2, 0.3, 0.4, 400, 500])).toBeUndefined();
});

test('a sample of frames at 16.7 ms gives 60 Hz', async () => {
  let time = 0;
  const queue: (() => void)[] = [];
  const frames: Frames = {
    request: (callback) => queue.push(callback),
    cancel: () => undefined,
  };
  const result = sampleRefresh(frames, () => time, 8);
  while (queue.length > 0) {
    time += 1000 / 60;
    queue.shift()?.();
  }
  expect(await result).toBe(60);
});
