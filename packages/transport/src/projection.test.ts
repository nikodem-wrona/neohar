import { expect, test } from 'bun:test';
import type { PatchOp, ProjectionData, ProjectionMessage } from './generated';
import { ProjectionFollower, ProjectionWatch, type RawSubscribe } from './projection';

const data = (n: number): ProjectionData => ({ items: { a: n }, order: ['a'] });
const snapshot = (version: number): ProjectionMessage => ({
  type: 'snapshot',
  version,
  data: data(version),
});
const patch = (version: number): ProjectionMessage => ({
  type: 'patch',
  version,
  ops: [{ op: 'set', key: 'a', value: version }],
});

/** A transport that records what the follower asks for and lets the test send messages. */
function fake() {
  const opened: {
    since: number | null;
    send: (message: ProjectionMessage) => void;
    lose: () => void;
    closed: boolean;
  }[] = [];
  const subscribe: RawSubscribe = (_name, since, onMessage, onLost) => {
    const entry = { since, send: onMessage, lose: onLost, closed: false };
    opened.push(entry);
    return {
      close: () => {
        entry.closed = true;
      },
    };
  };
  const seen: string[] = [];
  const handlers = {
    onSnapshot: (_data: ProjectionData, version: number) => seen.push(`snapshot ${version}`),
    onPatch: (_ops: PatchOp[], version: number) => seen.push(`patch ${version}`),
    onResync: () => seen.push('resync'),
  };
  return { opened, subscribe, seen, handlers };
}

test('a snapshot and the patches after it reach the handlers in order', () => {
  const t = fake();
  const follower = new ProjectionFollower(t.subscribe, 'tree', t.handlers);
  expect(t.opened).toHaveLength(1);
  expect(t.opened[0]?.since).toBeNull();
  t.opened[0]?.send(snapshot(5));
  t.opened[0]?.send(patch(6));
  t.opened[0]?.send(patch(7));
  expect(t.seen).toEqual(['snapshot 5', 'patch 6', 'patch 7']);
  expect(follower.version).toBe(7);
});

test('a patch that skips a version makes the follower ask for the patches since the last one', () => {
  const t = fake();
  const follower = new ProjectionFollower(t.subscribe, 'tree', t.handlers);
  t.opened[0]?.send(snapshot(5));
  t.opened[0]?.send(patch(6));
  // Version 7 was lost.
  t.opened[0]?.send(patch(8));
  expect(t.seen).toEqual(['snapshot 5', 'patch 6']);
  expect(t.opened).toHaveLength(2);
  expect(t.opened[0]?.closed).toBe(true);
  expect(t.opened[1]?.since).toBe(6);
  // The core sends the missed patches on the new subscription.
  t.opened[1]?.send(patch(7));
  t.opened[1]?.send(patch(8));
  expect(t.seen).toEqual(['snapshot 5', 'patch 6', 'patch 7', 'patch 8']);
  expect(follower.version).toBe(8);
  // Nothing from the old subscription counts any more.
  t.opened[0]?.send(patch(9));
  expect(follower.version).toBe(8);
});

test('a patch that repeats a version is dropped', () => {
  const t = fake();
  new ProjectionFollower(t.subscribe, 'tree', t.handlers);
  t.opened[0]?.send(snapshot(1));
  t.opened[0]?.send(patch(2));
  t.opened[0]?.send(patch(2));
  expect(t.seen).toEqual(['snapshot 1', 'patch 2']);
  expect(t.opened).toHaveLength(1);
});

test('resync tells the handlers and subscribes again for a snapshot', () => {
  const t = fake();
  const follower = new ProjectionFollower(t.subscribe, 'panes', t.handlers);
  t.opened[0]?.send(snapshot(3));
  t.opened[0]?.send({ type: 'resync' });
  expect(t.seen).toEqual(['snapshot 3', 'resync']);
  expect(t.opened).toHaveLength(2);
  expect(t.opened[1]?.since).toBeNull();
  expect(follower.version).toBeNull();
  t.opened[1]?.send(snapshot(40));
  expect(t.seen.at(-1)).toBe('snapshot 40');
});

test('a subscription that the transport lost opens again with the last version', () => {
  const t = fake();
  new ProjectionFollower(t.subscribe, 'tree', t.handlers);
  t.opened[0]?.send(snapshot(10));
  t.opened[0]?.send(patch(11));
  t.opened[0]?.lose();
  expect(t.opened).toHaveLength(2);
  expect(t.opened[1]?.since).toBe(11);
  // The core had no patches to send, and it says so.
  t.opened[1]?.send({ type: 'resync' });
  expect(t.opened[2]?.since).toBeNull();
});

test('a patch before any snapshot is a fault, and the follower asks for a snapshot', () => {
  const t = fake();
  new ProjectionFollower(t.subscribe, 'tree', t.handlers);
  t.opened[0]?.send(patch(4));
  expect(t.seen).toEqual([]);
  expect(t.opened[1]?.since).toBeNull();
});

test('close ends the subscription, and nothing reaches the handlers after it', () => {
  const t = fake();
  const follower = new ProjectionFollower(t.subscribe, 'tree', t.handlers);
  t.opened[0]?.send(snapshot(1));
  follower.close();
  expect(t.opened[0]?.closed).toBe(true);
  t.opened[0]?.send(patch(2));
  t.opened[0]?.lose();
  expect(t.seen).toEqual(['snapshot 1']);
  expect(t.opened).toHaveLength(1);
});

// --- the check of the versions and the watch ---------------------------------------

test('a core ahead of the follower makes it ask for the patches since its version', () => {
  const t = fake();
  const follower = new ProjectionFollower(t.subscribe, 'attention', t.handlers);
  t.opened[0]?.send(snapshot(1));
  t.opened[0]?.send(patch(2));
  // The channel lost version 3 and holds everything after it. The watch finds the core at 4.
  follower.check(4);
  expect(t.opened).toHaveLength(2);
  expect(t.opened[0]?.closed).toBe(true);
  expect(t.opened[1]?.since).toBe(2);
  t.opened[1]?.send(patch(3));
  t.opened[1]?.send(patch(4));
  expect(t.seen).toEqual(['snapshot 1', 'patch 2', 'patch 3', 'patch 4']);
  // The same version is nothing to do.
  follower.check(4);
  expect(t.opened).toHaveLength(2);
});

test('a core behind the follower is a core that started again, and the follower takes a snapshot', () => {
  const t = fake();
  const follower = new ProjectionFollower(t.subscribe, 'attention', t.handlers);
  t.opened[0]?.send(snapshot(7));
  follower.check(2);
  expect(t.opened).toHaveLength(2);
  expect(t.opened[1]?.since).toBeNull();
  expect(follower.version).toBeNull();
  t.opened[1]?.send(snapshot(2));
  expect(t.seen).toEqual(['snapshot 7', 'snapshot 2']);
});

test('before the first snapshot, and after close, a check does nothing', () => {
  const t = fake();
  const follower = new ProjectionFollower(t.subscribe, 'attention', t.handlers);
  follower.check(5);
  expect(t.opened).toHaveLength(1);
  t.opened[0]?.send(snapshot(1));
  follower.close();
  follower.check(9);
  expect(t.opened).toHaveLength(1);
});

test('a handler that fails does not stop the follower: it logs, takes a snapshot and goes on', () => {
  const t = fake();
  const errors: unknown[] = [];
  const log = console.error;
  console.error = (...args: unknown[]) => errors.push(args);
  try {
    let failing = true;
    const handlers = {
      ...t.handlers,
      onPatch: (ops: PatchOp[], version: number) => {
        if (failing) throw new Error('the store refused the patch');
        t.handlers.onPatch(ops, version);
      },
    };
    const follower = new ProjectionFollower(t.subscribe, 'attention', handlers);
    t.opened[0]?.send(snapshot(1));
    t.opened[0]?.send(patch(2));
    expect(errors).toHaveLength(1);
    // The state of the handlers is not known: a new subscription for a snapshot.
    expect(t.opened).toHaveLength(2);
    expect(t.opened[1]?.since).toBeNull();
    failing = false;
    t.opened[1]?.send(snapshot(2));
    t.opened[1]?.send(patch(3));
    expect(t.seen).toEqual(['snapshot 1', 'snapshot 2', 'patch 3']);
    expect(follower.version).toBe(3);
  } finally {
    console.error = log;
  }
});

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

test('the watch asks the core for the versions when the window comes back and tells each follower', async () => {
  const t = fake();
  const asked: string[][] = [];
  let versions: Record<string, number> = {};
  const wake = new EventTarget();
  const watch = new ProjectionWatch({
    probe: (names) => {
      asked.push(names);
      return Promise.resolve(versions);
    },
    wakeups: [[wake, 'focus']],
  });
  const attention = watch.subscribe(t.subscribe, 'attention', t.handlers);
  const tree = watch.subscribe(t.subscribe, 'tree', t.handlers);
  expect(watch.size).toBe(2);
  t.opened[0]?.send(snapshot(3));
  t.opened[1]?.send(snapshot(8));
  // Both are up to date: nothing opens.
  versions = { attention: 3, tree: 8 };
  wake.dispatchEvent(new Event('focus'));
  await tick();
  expect(asked).toEqual([['attention', 'tree']]);
  expect(t.opened).toHaveLength(2);
  // Attention is behind, and a name that the core does not know is ignored.
  versions = { attention: 5, panes: 1 };
  wake.dispatchEvent(new Event('focus'));
  await tick();
  expect(asked).toHaveLength(2);
  expect(t.opened).toHaveLength(3);
  expect(t.opened[2]?.since).toBe(3);
  // A probe that fails is not the business of the watch.
  const failing = new ProjectionWatch({
    probe: () => Promise.reject(new Error('down')),
    wakeups: [],
  });
  failing.subscribe(t.subscribe, 'panes', t.handlers);
  await failing.tick();
  // The last close ends the wakeups.
  attention.close();
  tree.close();
  expect(watch.size).toBe(0);
  wake.dispatchEvent(new Event('focus'));
  await tick();
  expect(asked).toHaveLength(2);
  expect(t.opened.slice(0, 3).every((entry) => entry.closed)).toBe(true);
});
