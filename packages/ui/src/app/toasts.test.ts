import { expect, test } from 'bun:test';
import type { Timers } from '../lib/timing';
import { createToasts, TOAST_LIMIT, TOAST_SHOW_MS } from './toasts';

/** Timers that run when the test says so. */
function manualTimers(): Timers & { pauses: number[]; pending(): number; run(): void } {
  const queue: { callback: () => void }[] = [];
  const pauses: number[] = [];
  return {
    pauses,
    set(callback, ms) {
      pauses.push(ms);
      const entry = { callback };
      queue.push(entry);
      return entry;
    },
    clear(handle) {
      const at = queue.indexOf(handle as (typeof queue)[number]);
      if (at >= 0) queue.splice(at, 1);
    },
    pending: () => queue.length,
    run() {
      for (const entry of queue.splice(0)) entry.callback();
    },
  };
}

test('a toast shows for a time and then goes', () => {
  const timers = manualTimers();
  const toasts = createToasts(timers);
  toasts.show({ title: 'Answer delivered', tone: 'ok' });
  expect(toasts.shown()).toEqual([{ id: 1, title: 'Answer delivered', tone: 'ok' }]);
  expect(timers.pauses).toEqual([TOAST_SHOW_MS]);
  timers.run();
  expect(toasts.shown()).toEqual([]);
});

test('a toast that the person closes goes at once, with its timer', () => {
  const timers = manualTimers();
  const toasts = createToasts(timers);
  toasts.show({ title: 'One', tone: 'ok' });
  toasts.show({ title: 'Two', tone: 'error' });
  toasts.dismiss(1);
  expect(toasts.shown().map((toast) => toast.title)).toEqual(['Two']);
  expect(timers.pending()).toBe(1);
});

test('a new toast pushes out the oldest one over the limit', () => {
  const timers = manualTimers();
  const toasts = createToasts(timers);
  for (let n = 1; n <= TOAST_LIMIT + 1; n += 1) toasts.show({ title: `T${n}`, tone: 'ok' });
  expect(toasts.shown().map((toast) => toast.id)).toEqual([2, 3, 4]);
  expect(timers.pending()).toBe(TOAST_LIMIT);
});

test('no toast goes while the pointer is on them, and each gets its whole time again after', () => {
  const timers = manualTimers();
  const toasts = createToasts(timers);
  toasts.show({ title: 'One', tone: 'ok' });
  toasts.pause();
  expect(timers.pending()).toBe(0);
  toasts.show({ title: 'Two', tone: 'ok' });
  expect(timers.pending()).toBe(0);
  toasts.resume();
  expect(timers.pending()).toBe(2);
  timers.run();
  expect(toasts.shown()).toEqual([]);
});

test('clear removes every toast and every timer', () => {
  const timers = manualTimers();
  const toasts = createToasts(timers);
  toasts.show({ title: 'One', tone: 'ok' });
  toasts.show({ title: 'Two', tone: 'warn', description: 'More' });
  toasts.clear();
  expect(toasts.shown()).toEqual([]);
  expect(timers.pending()).toBe(0);
});
