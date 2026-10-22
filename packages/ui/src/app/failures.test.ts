import { expect, test } from 'bun:test';
import { RpcError, type Transport } from '@neohar/transport';
import type { Timers } from '../lib/timing';
import { createFailures, FAILURE_SHOW_MS, reportRejections } from './failures';

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

/** A transport whose calls fail with `failure`, or succeed when it is `null`. */
function transport(failure: unknown) {
  const calls: { method: string; params: unknown }[] = [];
  const fake = {
    call: (method: string, params: unknown) => {
      calls.push({ method, params });
      return failure === null ? Promise.resolve({}) : Promise.reject(failure);
    },
  } as unknown as Pick<Transport, 'call'>;
  return { calls, fake };
}

/** Lets the promises of a call settle. */
const settle = () => new Promise((resolve) => setImmediate(resolve));

test('a command that fails shows its message and its code', async () => {
  const { calls, fake } = transport(new RpcError('pane_gone', 'the pane p3 has no process'));
  const failures = createFailures(fake, manualTimers());
  failures.run('pane.restart', { pane: 'p3' });
  expect(calls).toEqual([{ method: 'pane.restart', params: { pane: 'p3' } }]);
  await settle();
  expect(failures.shown()).toEqual({
    id: 1,
    code: 'pane_gone',
    message: 'the pane p3 has no process',
  });
});

test('a command that works shows nothing', async () => {
  const { fake } = transport(null);
  const timers = manualTimers();
  const failures = createFailures(fake, timers);
  failures.run('tab.create', { workspace: 'w1' });
  await settle();
  expect(failures.shown()).toBeNull();
  expect(timers.pending()).toBe(0);
});

test('the banner goes after a time, and a new failure takes the place of the last', async () => {
  const { fake } = transport(new RpcError('not_found', 'no tab t9'));
  const timers = manualTimers();
  const failures = createFailures(fake, timers);
  failures.run('tab.select', { tab: 't9', window: 'W' });
  await settle();
  failures.report(new RpcError('busy', 'try again'));
  expect(failures.shown()).toEqual({ id: 2, code: 'busy', message: 'try again' });
  // The second failure starts the time again.
  expect(timers.pauses).toEqual([FAILURE_SHOW_MS, FAILURE_SHOW_MS]);
  expect(timers.pending()).toBe(1);
  timers.run();
  expect(failures.shown()).toBeNull();
});

test('the person closes the banner', () => {
  const timers = manualTimers();
  const failures = createFailures(transport(null).fake, timers);
  failures.report(new RpcError('internal', 'a fault'));
  failures.dismiss();
  expect(failures.shown()).toBeNull();
  expect(timers.pending()).toBe(0);
});

test('a fault of the page and a rejected value that is not an error show too', () => {
  const failures = createFailures(transport(null).fake, manualTimers());
  failures.report(new TypeError('x is undefined'));
  expect(failures.shown()).toMatchObject({ code: 'error', message: 'x is undefined' });
  failures.report('gone');
  expect(failures.shown()).toMatchObject({ code: 'error', message: 'gone' });
});

test('a promise of the page that nobody handled shows until the report stops', () => {
  const page = new EventTarget();
  const reported: unknown[] = [];
  const stop = reportRejections(page, (failure) => reported.push(failure));
  const rejection = (reason: unknown): Event =>
    Object.assign(new Event('unhandledrejection'), { reason });
  page.dispatchEvent(rejection(new RpcError('link_down', 'the link is down')));
  expect(reported).toHaveLength(1);
  expect((reported[0] as RpcError).code).toBe('link_down');
  stop();
  page.dispatchEvent(rejection('later'));
  expect(reported).toHaveLength(1);
});
