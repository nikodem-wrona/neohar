import { expect, test } from 'bun:test';
import { createFrameSender, type Frames } from './frames';

/** Frames that run when the test says so. */
function fakeFrames() {
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
    frames,
    pending: () => queue.size,
    tick() {
      const callbacks = [...queue.values()];
      queue.clear();
      for (const callback of callbacks) callback();
    },
  };
}

test('many values in one frame make one send with the last value', () => {
  const sent: number[] = [];
  const clock = fakeFrames();
  const sender = createFrameSender<number>((value) => sent.push(value), clock.frames);
  for (let i = 1; i <= 50; i++) sender.push(i);
  expect(sent).toEqual([]);
  expect(clock.pending()).toBe(1);
  clock.tick();
  expect(sent).toEqual([50]);
  clock.tick();
  expect(sent).toEqual([50]);
});

test('each frame with a new value sends once', () => {
  const sent: number[] = [];
  const clock = fakeFrames();
  const sender = createFrameSender<number>((value) => sent.push(value), clock.frames);
  sender.push(1);
  clock.tick();
  sender.push(2);
  sender.push(3);
  clock.tick();
  expect(sent).toEqual([1, 3]);
});

test('flush sends the kept value at once and the frame then sends nothing', () => {
  const sent: number[] = [];
  const clock = fakeFrames();
  const sender = createFrameSender<number>((value) => sent.push(value), clock.frames);
  sender.push(7);
  sender.flush();
  expect(sent).toEqual([7]);
  expect(clock.pending()).toBe(0);
  sender.flush();
  expect(sent).toEqual([7]);
});

test('cancel drops the kept value', () => {
  const sent: number[] = [];
  const clock = fakeFrames();
  const sender = createFrameSender<number>((value) => sent.push(value), clock.frames);
  sender.push(1);
  sender.cancel();
  clock.tick();
  expect(sent).toEqual([]);
});
