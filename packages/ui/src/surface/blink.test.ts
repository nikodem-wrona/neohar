import { expect, test } from 'bun:test';
import { BLINK_MS, blinkPhase } from './blink';

test('the phase is on for a blink period and off for the next', () => {
  expect(blinkPhase(0)).toBe(true);
  expect(blinkPhase(BLINK_MS - 1)).toBe(true);
  expect(blinkPhase(BLINK_MS)).toBe(false);
  expect(blinkPhase(2 * BLINK_MS - 1)).toBe(false);
  expect(blinkPhase(2 * BLINK_MS)).toBe(true);
});

test('a time with a fraction, as requestAnimationFrame gives it, is in the period it falls in', () => {
  expect(blinkPhase(529.9)).toBe(true);
  expect(blinkPhase(1059.5)).toBe(false);
  expect(blinkPhase(123456.789)).toBe(blinkPhase(123456));
});

test('the phase goes on across the wrap of the 32-bit time', () => {
  expect([true, false]).toContain(blinkPhase(2 ** 31 + 10));
  expect([true, false]).toContain(blinkPhase(2 ** 40));
});
