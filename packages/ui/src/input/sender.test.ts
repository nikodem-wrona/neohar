import { expect, test } from 'bun:test';
import type { Transport } from '@neohar/transport';
import type { Frames } from '../lib/frames';
import { InputIds } from './ids';
import { InputSender } from './sender';

function setup(seed = 1000) {
  const calls: { method: string; params: Record<string, unknown> }[] = [];
  const transport = {
    call: (method: string, params: Record<string, unknown>) => {
      calls.push({ method, params });
      return Promise.resolve({});
    },
  } as unknown as Transport;
  const queue = new Map<number, () => void>();
  let next = 1;
  const frames: Frames = {
    request: (callback) => {
      queue.set(next, callback);
      return next++;
    },
    cancel: (id) => void queue.delete(id),
  };
  return {
    calls,
    sender: new InputSender(transport, frames, new InputIds(seed)),
    frame: () => {
      const callbacks = [...queue.values()];
      queue.clear();
      for (const callback of callbacks) callback();
    },
  };
}

test('each key goes at once with a new input id', () => {
  const { sender, calls } = setup(500);
  sender.send('P', { type: 'text', text: 'a' });
  sender.send('P', { type: 'text', text: 'b' });
  expect(calls.map((c) => c.params.input_id)).toEqual([500, 501]);
  expect(calls[0]).toMatchObject({ method: 'pane.input', params: { pane: 'P' } });
});

test('the wheel notches of a frame go out as one event with their sum', () => {
  const { sender, calls, frame } = setup();
  sender.wheel('P', { col: 3, row: 4, x: 0, y: 1, mods: [] });
  sender.wheel('P', { col: 5, row: 6, x: 0, y: 2, mods: ['shift'] });
  expect(calls).toHaveLength(0);
  frame();
  expect(calls).toHaveLength(1);
  expect(calls[0]?.params.event).toEqual({
    type: 'mouse',
    kind: 'wheel',
    col: 5,
    row: 6,
    wheel_x: 0,
    wheel_y: 3,
    mods: ['shift'],
  });
  frame();
  expect(calls).toHaveLength(1);
});

test('notches that cancel each other send nothing', () => {
  const { sender, calls, frame } = setup();
  sender.wheel('P', { col: 0, row: 0, x: 0, y: 1, mods: [] });
  sender.wheel('P', { col: 0, row: 0, x: 0, y: -1, mods: [] });
  frame();
  expect(calls).toHaveLength(0);
});

test('a drag of a selection sends the last point of each frame', () => {
  const { sender, calls, frame } = setup();
  const anchor = { col: 1, line: 10 };
  for (let col = 2; col < 20; col++) {
    sender.select('P', { anchor, head: { col, line: 10 }, mode: 'simple' });
  }
  frame();
  expect(calls).toHaveLength(1);
  expect(calls[0]).toEqual({
    method: 'pane.select',
    params: { pane: 'P', anchor, head: { col: 19, line: 10 }, mode: 'simple' },
  });
});

test('the move of the pointer is one event for each frame', () => {
  const { sender, calls, frame } = setup();
  sender.move('P', { col: 1, row: 1, mods: [], button: 'none' });
  sender.move('P', { col: 2, row: 2, mods: [], button: 'none' });
  frame();
  expect(calls).toHaveLength(1);
  expect(calls[0]?.params.event).toMatchObject({ kind: 'move', col: 2, row: 2 });
});

test('a press sends what waits before it, and flush sends the last point of a drag', () => {
  const { sender, calls } = setup();
  sender.wheel('P', { col: 0, row: 0, x: 0, y: 1, mods: [] });
  sender.mouse('P', { type: 'mouse', kind: 'down', button: 'left', col: 1, row: 1, mods: [] });
  expect(calls.map((c) => (c.params.event as { kind: string }).kind)).toEqual(['wheel', 'down']);
  sender.select('P', { anchor: { col: 0, line: 0 }, head: { col: 4, line: 0 }, mode: 'simple' });
  sender.flush();
  expect(calls.at(-1)?.method).toBe('pane.select');
});
