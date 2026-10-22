import { expect, test } from 'bun:test';
import { RpcError } from './errors';
import type { ProjectionMessage } from './generated';
import { type TauriHost, TauriTransport } from './tauri';
import type { Timers } from './websocket';

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

interface FakeChannel {
  onMessage: (message: unknown) => void;
}

/**
 * A host whose `subscribe` refuses as long as `refuse` is set, with the error object that the
 * host serializes. Each call of a command is kept.
 */
function fakeHost() {
  const state = {
    refuse: true as boolean,
    subscribes: [] as { name: string; since: number | null; channel: FakeChannel }[],
    calls: [] as { method: string; params: unknown }[],
    streamRefusal: null as unknown,
  };
  const host: TauriHost = {
    rpc: (args) => {
      state.calls.push(args);
      return Promise.resolve({});
    },
    subscribe: (args) => {
      state.subscribes.push(args as (typeof state.subscribes)[number]);
      return state.refuse
        ? Promise.reject({ code: 'link_down', message: 'the link to devbox is down' })
        : Promise.resolve(`s${state.subscribes.length}`);
    },
    stream: () =>
      state.streamRefusal === null ? Promise.resolve() : Promise.reject(state.streamRefusal),
    channel: (onMessage) => ({ onMessage }) as FakeChannel,
  };
  return { host, state };
}

const RENDER = { window: 'W', proto: 1, viewport_hz: 60 };

/** Lets the promises of the transport settle. */
const settle = () => new Promise((resolve) => setImmediate(resolve));

test('a refused subscription shows once, and the transport subscribes again after a pause', async () => {
  const timers = manualTimers();
  const { host, state } = fakeHost();
  const transport = new TauriTransport({ timers, host });
  const failures: RpcError[] = [];
  transport.onFailure((failure) => failures.push(failure));
  const snapshots: number[] = [];
  transport.subscribe('panes', {
    onSnapshot: (_data, version) => snapshots.push(version),
    onPatch: () => undefined,
  });

  await settle();
  expect(state.subscribes).toHaveLength(1);
  expect(failures).toHaveLength(1);
  expect(failures[0]).toBeInstanceOf(RpcError);
  expect(failures[0]?.code).toBe('link_down');
  expect(failures[0]?.message).toBe('the link to devbox is down');

  // The second refusal in a row is the same failure: it does not show again, and the pause doubles.
  timers.run();
  await settle();
  expect(state.subscribes).toHaveLength(2);
  expect(failures).toHaveLength(1);
  expect(timers.pauses).toEqual([250, 500]);

  // The host takes the next one, and the snapshot comes on its channel.
  state.refuse = false;
  timers.run();
  await settle();
  expect(state.subscribes).toHaveLength(3);
  expect(state.subscribes[2]?.name).toBe('panes');
  expect(timers.pending()).toBe(0);
  const message: ProjectionMessage = {
    type: 'snapshot',
    version: 4,
    data: { items: {}, order: [] },
  };
  state.subscribes[2]?.channel.onMessage(message);
  expect(snapshots).toEqual([4]);
});

test('a refusal after a subscription that the host took shows again', async () => {
  const timers = manualTimers();
  const { host, state } = fakeHost();
  const transport = new TauriTransport({ timers, host });
  const failures: RpcError[] = [];
  transport.onFailure((failure) => failures.push(failure));
  state.refuse = false;
  transport.subscribe('panes', { onSnapshot: () => undefined, onPatch: () => undefined });
  await settle();
  state.refuse = true;
  transport.subscribe('panes', { onSnapshot: () => undefined, onPatch: () => undefined });
  await settle();
  expect(failures).toHaveLength(1);
  expect(timers.pauses).toEqual([250]);
});

test('a subscription that closes while it waits does not subscribe again', async () => {
  const timers = manualTimers();
  const { host, state } = fakeHost();
  const transport = new TauriTransport({ timers, host });
  const subscription = transport.subscribe('tree', {
    onSnapshot: () => undefined,
    onPatch: () => undefined,
  });
  await settle();
  expect(timers.pending()).toBe(1);
  subscription.close();
  expect(timers.pending()).toBe(0);
  timers.run();
  await settle();
  expect(state.subscribes).toHaveLength(1);
});

test('a listener that ended gets nothing', async () => {
  const timers = manualTimers();
  const { host } = fakeHost();
  const transport = new TauriTransport({ timers, host });
  const failures: RpcError[] = [];
  const stop = transport.onFailure((failure) => failures.push(failure));
  stop();
  transport.subscribe('tree', { onSnapshot: () => undefined, onPatch: () => undefined });
  await settle();
  expect(failures).toEqual([]);
});

test('a refused stream shows, and a stream that closed first does not', async () => {
  const timers = manualTimers();
  const { host, state } = fakeHost();
  const transport = new TauriTransport({ timers, host });
  const failures: RpcError[] = [];
  transport.onFailure((failure) => failures.push(failure));
  state.streamRefusal = { code: 'not_found', message: 'no window W' };
  transport.stream('render', RENDER, () => undefined);
  await settle();
  expect(failures.map((failure) => failure.code)).toEqual(['not_found']);

  const closed = transport.stream('render', RENDER, () => undefined);
  closed.close();
  await settle();
  expect(failures).toHaveLength(1);
});
