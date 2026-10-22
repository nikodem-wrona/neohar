import { expect, test } from 'bun:test';
import type { Transport } from '@neohar/transport';
import type { Frames } from '../lib/frames';
import { InputIds, pageInputIds } from './ids';
import { InputSender } from './sender';

/** A transport that records the `input_id` of each call, in the order of the calls. */
function recorder(ids: number[]): Transport {
  return {
    call: (_method: string, params: Record<string, unknown>) => {
      if (typeof params.input_id === 'number') ids.push(params.input_id);
      return Promise.resolve({});
    },
  } as unknown as Transport;
}

const noFrames: Frames = { request: () => 0, cancel: () => undefined };

function expectGrowing(ids: number[]): void {
  expect(new Set(ids).size).toBe(ids.length);
  for (let i = 1; i < ids.length; i++) expect(ids[i]).toBeGreaterThan(ids[i - 1] ?? 0);
}

test('a counter gives ids that grow from its seed', () => {
  const ids = new InputIds(7);
  expect([ids.take(), ids.take(), ids.take()]).toEqual([7, 8, 9]);
});

test('the ids of all the senders of a page never repeat and always grow', async () => {
  const taken: number[] = [];
  const transport = recorder(taken);
  // The desk and the phone each make a sender. A phone that shows another machine makes a new one.
  const desk = new InputSender(transport, noFrames);
  const phone = new InputSender(transport, noFrames, undefined, { outbox: true });
  const later = new InputSender(transport, noFrames);
  for (let round = 0; round < 50; round++) {
    desk.send('P', { type: 'text', text: 'a' });
    await phone.send('P', { type: 'text', text: 'b' });
    later.paste('P', 'c');
    await desk.insertPaths('P', ['/tmp/x']);
    // A reply, a key-bar reply and a file attach take an id for a call that they make.
    taken.push(desk.nextId(), phone.nextId(), later.nextId());
    later.mouse('P', {
      type: 'mouse',
      kind: 'down',
      button: 'left',
      col: 0,
      row: 0,
      mods: [],
    });
  }
  expect(taken).toHaveLength(50 * 8);
  expectGrowing(taken);
  expect(taken.at(-1)).toBeLessThan(pageInputIds.take());
});

test('the counter of the page starts above the ids of an earlier page', () => {
  const before = Date.now() * 1000;
  expect(new InputIds().take()).toBeGreaterThanOrEqual(before);
});
