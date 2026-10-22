import { expect, test } from 'bun:test';
import { debounce, type Timers } from './timing';

/** Timers that run when the test says so. */
function manualTimers(): Timers & { advance(ms: number): void; pending(): number } {
  let now = 0;
  let next = 1;
  const queue = new Map<number, { at: number; callback: () => void }>();
  return {
    set(callback, ms) {
      const id = next++;
      queue.set(id, { at: now + ms, callback });
      return id;
    },
    clear(handle) {
      queue.delete(handle as number);
    },
    advance(ms) {
      now += ms;
      for (const [id, timer] of [...queue]) {
        if (timer.at <= now) {
          queue.delete(id);
          timer.callback();
        }
      }
    },
    pending: () => queue.size,
  };
}

test('a burst of calls makes one call, with the last arguments, at its end', () => {
  const timers = manualTimers();
  const calls: number[][] = [];
  const resize = debounce((cols: number, rows: number) => calls.push([cols, rows]), 50, timers);
  resize(80, 24);
  timers.advance(40);
  resize(90, 30);
  timers.advance(40);
  resize(100, 40);
  expect(calls).toEqual([]);
  timers.advance(49);
  expect(calls).toEqual([]);
  timers.advance(1);
  expect(calls).toEqual([[100, 40]]);
  expect(timers.pending()).toBe(0);
});

test('calls that are far enough apart each go through', () => {
  const timers = manualTimers();
  const seen: string[] = [];
  const log = debounce((text: string) => seen.push(text), 50, timers);
  log('a');
  timers.advance(50);
  log('b');
  timers.advance(50);
  expect(seen).toEqual(['a', 'b']);
});

test('cancel drops the pending call, and a call after it still works', () => {
  const timers = manualTimers();
  let count = 0;
  const bump = debounce(() => count++, 50, timers);
  bump();
  bump.cancel();
  timers.advance(100);
  expect(count).toBe(0);
  expect(timers.pending()).toBe(0);
  bump.cancel();
  bump();
  timers.advance(50);
  expect(count).toBe(1);
});
