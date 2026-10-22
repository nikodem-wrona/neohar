import { expect, test } from 'bun:test';
import type { Timers } from '../lib/timing';
import type { CellMetrics } from '../surface';
import { cellsFor, ResizeScheduler } from './resize';

/** Timers on a clock that the test moves. */
function clock() {
  let now = 0;
  let next = 1;
  const pending = new Map<number, { at: number; run: () => void }>();
  const timers: Timers = {
    set: (run, ms) => {
      pending.set(next, { at: now + ms, run });
      return next++;
    },
    clear: (handle) => void pending.delete(handle as number),
  };
  return {
    timers,
    /** Moves the clock by `ms`, running the timers that come due in order. */
    advance(ms: number) {
      const end = now + ms;
      for (;;) {
        let due: [number, { at: number; run: () => void }] | undefined;
        for (const entry of pending) {
          if (entry[1].at <= end && (!due || entry[1].at < due[1].at)) due = entry;
        }
        if (!due) break;
        pending.delete(due[0]);
        now = due[1].at;
        due[1].run();
      }
      now = end;
    },
  };
}

const metrics: CellMetrics = {
  cellW: 16,
  cellH: 32,
  baseline: 24,
  underlineY: 28,
  lineH: 2,
  strikeY: 16,
};

test('the cells that fit in a box', () => {
  expect(cellsFor({ w: 1600, h: 640 }, metrics)).toEqual({ cols: 100, rows: 20 });
  expect(cellsFor({ w: 1599, h: 639 }, metrics)).toEqual({ cols: 99, rows: 19 });
  expect(cellsFor({ w: 3, h: 3 }, metrics)).toEqual({ cols: 2, rows: 1 });
});

test('one change is sent 50 ms later', () => {
  const sent: number[] = [];
  const time = clock();
  const scheduler = new ResizeScheduler<number>((v) => sent.push(v), time.timers);
  scheduler.request(7);
  time.advance(49);
  expect(sent).toEqual([]);
  time.advance(1);
  expect(sent).toEqual([7]);
});

test('changes less than 50 ms apart are one send of the last value', () => {
  const sent: number[] = [];
  const time = clock();
  const scheduler = new ResizeScheduler<number>((v) => sent.push(v), time.timers);
  scheduler.request(1);
  time.advance(30);
  scheduler.request(2);
  time.advance(30);
  scheduler.request(3);
  time.advance(60);
  expect(sent).toEqual([3]);
});

test('a drag of two seconds sends at most ten values before the last one', () => {
  const sent: number[] = [];
  const time = clock();
  const scheduler = new ResizeScheduler<number>((v) => sent.push(v), time.timers);
  for (let step = 1; step <= 125; step++) {
    scheduler.request(step);
    time.advance(16);
  }
  time.advance(100);
  // Ten while the drag goes on, and the last value once it stops.
  expect(sent).toHaveLength(11);
  expect(sent.at(-1)).toBe(125);
  // A second drag starts with a new count.
  scheduler.request(500);
  time.advance(60);
  expect(sent).toHaveLength(12);
});

test('a drag of 300 ms sends a few values on the way and the last one at the end', () => {
  const sent: number[] = [];
  const time = clock();
  const scheduler = new ResizeScheduler<number>((v) => sent.push(v), time.timers);
  for (let step = 1; step <= 19; step++) {
    scheduler.request(step);
    time.advance(16);
  }
  time.advance(100);
  expect(sent.length).toBeGreaterThan(1);
  expect(sent.length).toBeLessThanOrEqual(5);
  expect(sent.at(-1)).toBe(19);
});

test('cancel drops the value that waits', () => {
  const sent: number[] = [];
  const time = clock();
  const scheduler = new ResizeScheduler<number>((v) => sent.push(v), time.timers);
  scheduler.request(1);
  scheduler.cancel();
  time.advance(500);
  expect(sent).toEqual([]);
});
