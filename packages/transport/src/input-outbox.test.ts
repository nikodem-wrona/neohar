import { expect, test } from 'bun:test';
import { InputOutbox, OUTBOX_TTL_MS, type InputParams } from './input-outbox';

const input = (id: number): InputParams => ({
  pane: 'w1:p1',
  input_id: id,
  event: { type: 'text', text: 'x' },
});

test('an input stays in the outbox until a frame confirms it', async () => {
  const sent: number[] = [];
  const outbox = new InputOutbox(async (p) => void sent.push(p.input_id));
  await outbox.submit(input(1));
  await outbox.submit(input(2));
  expect(sent).toEqual([1, 2]);
  expect(outbox.size).toBe(2);
  outbox.confirmed([1]);
  expect(outbox.size).toBe(1);
});

test('after a reconnect the kept inputs go out again with the same ids, oldest first', async () => {
  const sent: number[] = [];
  const outbox = new InputOutbox(async (p) => void sent.push(p.input_id));
  await outbox.submit(input(7));
  await outbox.submit(input(8));
  outbox.confirmed([7]);
  await outbox.resend();
  expect(sent).toEqual([7, 8, 8]);
});

test('an input that failed to send is kept and goes out at the next resend', async () => {
  let up = false;
  const sent: number[] = [];
  const outbox = new InputOutbox(async (p) => {
    if (!up) throw new Error('closed');
    sent.push(p.input_id);
  });
  await outbox.submit(input(1));
  expect(sent).toEqual([]);
  expect(outbox.size).toBe(1);
  up = true;
  await outbox.resend();
  expect(sent).toEqual([1]);
});

test('an input older than 30 seconds is dropped', async () => {
  let now = 1_000;
  const sent: number[] = [];
  const outbox = new InputOutbox(
    async (p) => void sent.push(p.input_id),
    () => now,
  );
  await outbox.submit(input(1));
  now += OUTBOX_TTL_MS - 1;
  await outbox.submit(input(2));
  now += 2;
  await outbox.resend();
  expect(sent).toEqual([1, 2, 2]);
  expect(outbox.size).toBe(1);
});

test('a resend that fails stops and leaves the rest for later', async () => {
  let fail = false;
  const sent: number[] = [];
  const outbox = new InputOutbox(async (p) => {
    if (fail) throw new Error('closed');
    sent.push(p.input_id);
  });
  await outbox.submit(input(1));
  await outbox.submit(input(2));
  fail = true;
  await outbox.resend();
  expect(sent).toEqual([1, 2]);
  expect(outbox.size).toBe(2);
});
