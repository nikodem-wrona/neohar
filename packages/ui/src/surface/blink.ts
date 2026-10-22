// The phase of a blink: cursors and blinking text.

/** A blink phase lasts this long, in milliseconds. */
export const BLINK_MS = 530;

/**
 * Whether `now` (a time in milliseconds) is in the first half of a blink period. It uses whole
 * numbers only: a division makes a number that is not an integer, and each one is boxed on the
 * heap in code that has not been optimized yet. After about 24 days the 32-bit time wraps; the
 * phase then jumps once and goes on.
 */
export function blinkPhase(now: number): boolean {
  return Math.abs(now | 0) % (2 * BLINK_MS) < BLINK_MS;
}
