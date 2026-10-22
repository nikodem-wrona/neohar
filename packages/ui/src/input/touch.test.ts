import { describe, expect, test } from 'bun:test';
import { type GestureHandlers, LONG_PRESS_MS, TouchGestures } from './touch';

/** Timers, frames and a clock that the test moves by hand. */
function rig() {
  let now = 0;
  const timers = new Map<number, { at: number; run: () => void }>();
  const frames = new Map<number, () => void>();
  let next = 1;
  const calls: string[] = [];
  const handlers: GestureHandlers = {
    tap: (at) => calls.push(`tap ${at.x},${at.y}`),
    scroll: (dy) => calls.push(`scroll ${Math.round(dy)}`),
    press: (at) => calls.push(`press ${at.x},${at.y}`),
    extend: (at) => calls.push(`extend ${at.x},${at.y}`),
    release: () => calls.push('release'),
    pan: (dx, dy) => calls.push(`pan ${Math.round(dx)},${Math.round(dy)}`),
    pinch: (scale, done) => calls.push(`pinch ${scale.toFixed(2)}${done ? ' done' : ''}`),
  };
  const gestures = new TouchGestures(
    handlers,
    {
      set: (run, ms) => {
        const id = next++;
        timers.set(id, { at: now + ms, run });
        return id;
      },
      clear: (id) => timers.delete(id as number),
    },
    {
      request: (run) => {
        const id = next++;
        frames.set(id, run);
        return id;
      },
      cancel: (id) => frames.delete(id),
    },
    () => now,
  );
  const advance = (ms: number) => {
    now += ms;
    for (const [id, timer] of [...timers]) {
      if (timer.at <= now) {
        timers.delete(id);
        timer.run();
      }
    }
  };
  const frame = (ms = 16) => {
    now += ms;
    const ready = [...frames];
    frames.clear();
    for (const [, run] of ready) run();
  };
  return { gestures, calls, advance, frame, frames };
}

const at = (x: number, y: number, id = 1) => ({ id, x, y });

describe('the gestures of a finger on a pane', () => {
  test('a short touch that does not move is a tap', () => {
    const r = rig();
    r.gestures.start([at(50, 60)]);
    r.advance(100);
    r.gestures.move([at(53, 62)]);
    r.gestures.end([]);
    expect(r.calls).toEqual(['tap 50,60']);
  });

  test('a finger that moves up scrolls toward the end, and the scroll glides on', () => {
    const r = rig();
    r.gestures.start([at(50, 300)]);
    for (let step = 1; step <= 5; step++) {
      r.advance(10);
      r.gestures.move([at(50, 300 - step * 20)]);
    }
    r.gestures.end([]);
    expect(r.calls.slice(0, 5).every((call) => call.startsWith('scroll '))).toBe(true);
    const moved = r.calls.map((call) => Number(call.split(' ')[1])).reduce((a, b) => a + b, 0);
    expect(moved).toBe(100);
    // The glide: more scrolls on the next frames, each one smaller, until it stops.
    const before = r.calls.length;
    r.frame();
    r.frame();
    expect(r.calls.length).toBe(before + 2);
    for (let n = 0; n < 400 && r.frames.size > 0; n++) r.frame();
    expect(r.frames.size).toBe(0);
  });

  test('a finger that rests before it leaves does not glide', () => {
    const r = rig();
    r.gestures.start([at(50, 300)]);
    for (let step = 1; step <= 5; step++) {
      r.advance(10);
      r.gestures.move([at(50, 300 - step * 20)]);
    }
    r.advance(300);
    r.gestures.end([]);
    expect(r.frames.size).toBe(0);
  });

  test('a long press selects, a drag makes the selection longer, and the release ends it', () => {
    const r = rig();
    r.gestures.start([at(20, 30)]);
    r.advance(LONG_PRESS_MS);
    r.gestures.move([at(80, 30)]);
    r.gestures.end([]);
    expect(r.calls).toEqual(['press 20,30', 'extend 80,30', 'release']);
  });

  test('a move before the long press is a scroll and selects nothing', () => {
    const r = rig();
    r.gestures.start([at(20, 300)]);
    r.advance(100);
    r.gestures.move([at(20, 250)]);
    r.advance(LONG_PRESS_MS);
    expect(r.calls).toEqual(['scroll 50']);
  });

  test('two fingers that move together pan, and two that spread pinch', () => {
    const pan = rig();
    pan.gestures.start([at(100, 100, 1), at(200, 100, 2)]);
    pan.gestures.move([at(70, 140, 1), at(170, 140, 2)]);
    pan.gestures.end([]);
    expect(pan.calls).toEqual(['pan -30,40']);

    const pinch = rig();
    pinch.gestures.start([at(100, 100, 1), at(200, 100, 2)]);
    pinch.gestures.move([at(50, 100, 1), at(250, 100, 2)]);
    pinch.gestures.end([at(250, 100, 2)]);
    pinch.gestures.end([]);
    expect(pinch.calls).toEqual(['pinch 2.00', 'pinch 2.00 done']);
  });
});
