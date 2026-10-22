import { expect, test } from 'bun:test';
import { RpcError } from './errors';
import type { ProjectionData, ProjectionMessage } from './generated';
import { type SocketLike, type Timers, WebSocketTransport } from './websocket';

/** A socket that the test drives. */
class FakeSocket implements SocketLike {
  binaryType = 'blob';
  onopen: ((event: unknown) => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: unknown) => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  readonly sent: string[] = [];
  closed = false;

  constructor(readonly url: string) {}

  send(data: string): void {
    this.sent.push(data);
  }

  close(): void {
    this.closed = true;
    this.onclose?.({});
  }

  open(): void {
    this.onopen?.({});
  }

  receive(data: unknown): void {
    this.onmessage?.({ data });
  }

  /** The requests that the client sent, as objects. */
  requests(): { id: number; method: string; params: unknown }[] {
    return this.sent.map(
      (text) => JSON.parse(text) as { id: number; method: string; params: unknown },
    );
  }

  reply(id: number, result: unknown): void {
    this.receive(JSON.stringify({ jsonrpc: '2.0', id, result }));
  }

  notify(subscription: string, message: ProjectionMessage): void {
    this.receive(
      JSON.stringify({
        jsonrpc: '2.0',
        method: 'projection.message',
        params: { subscription, message },
      }),
    );
  }
}

/** Timers that run when the test says so. */
function manualTimers(): Timers & { pauses: number[]; run(): void } {
  const queue: { callback: () => void; ms: number }[] = [];
  const pauses: number[] = [];
  return {
    pauses,
    set(callback, ms) {
      pauses.push(ms);
      const entry = { callback, ms };
      queue.push(entry);
      return entry;
    },
    clear(handle) {
      const at = queue.indexOf(handle as (typeof queue)[number]);
      if (at >= 0) queue.splice(at, 1);
    },
    run() {
      const due = queue.splice(0);
      for (const entry of due) entry.callback();
    },
  };
}

function transport(extra: { onReconnect?: () => void } = {}) {
  const sockets: FakeSocket[] = [];
  const timers = manualTimers();
  const t = new WebSocketTransport({
    port: 4242,
    token: 'secret',
    socket: (url) => {
      const socket = new FakeSocket(url);
      sockets.push(socket);
      return socket;
    },
    timers,
    backoff: { initialMs: 100, maxMs: 800 },
    // The socket alone: the watch of the subscriptions has its own tests.
    watch: false,
    ...extra,
  });
  return { t, sockets, timers };
}

const data = (n: number): ProjectionData => ({ items: { a: n }, order: ['a'] });
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

test('a call is a request of JSON-RPC on the control socket with the token in the address', async () => {
  const { t, sockets } = transport();
  const answer = t.call('session.info', {});
  await flush();
  const socket = sockets[0];
  expect(socket?.url).toBe('ws://127.0.0.1:4242/rpc?token=secret');
  // The request waits for the socket to open.
  expect(socket?.sent).toEqual([]);
  socket?.open();
  await flush();
  const [request] = socket?.requests() ?? [];
  expect(request?.method).toBe('session.info');
  socket?.reply(request?.id ?? 0, { api_version: 'neohar/1' });
  expect(await answer).toEqual({ api_version: 'neohar/1' } as never);
});

test('an error reply becomes an RpcError with the code of the contract', async () => {
  const { t, sockets } = transport();
  const answer = t.call('pane.close', { pane: 'w1:p1' });
  await flush();
  sockets[0]?.open();
  await flush();
  const [request] = sockets[0]?.requests() ?? [];
  sockets[0]?.receive(
    JSON.stringify({
      jsonrpc: '2.0',
      id: request?.id,
      error: {
        code: -32000,
        message: 'the pane runs',
        data: { code: 'confirm_required', details: { process: 'vim' } },
      },
    }),
  );
  const failure = await answer.catch((error: unknown) => error);
  expect(failure).toBeInstanceOf(RpcError);
  expect((failure as RpcError).code).toBe('confirm_required');
  expect((failure as RpcError).details).toEqual({ process: 'vim' });
});

test('a subscription gets the snapshot and the patches, also those that come before the answer', async () => {
  const { t, sockets } = transport();
  const seen: string[] = [];
  const subscription = t.subscribe('tree', {
    onSnapshot: (_data, version) => seen.push(`snapshot ${version}`),
    onPatch: (_ops, version) => seen.push(`patch ${version}`),
  });
  await flush();
  const socket = sockets[0];
  socket?.open();
  await flush();
  const [request] = socket?.requests() ?? [];
  expect(request?.method).toBe('projection.subscribe');
  expect(request?.params).toEqual({ name: 'tree' });
  // The snapshot is on the wire before the answer that names the subscription.
  socket?.notify('s1', { type: 'snapshot', version: 3, data: data(3) });
  socket?.reply(request?.id ?? 0, { subscription: 's1' });
  await flush();
  socket?.notify('s1', { type: 'patch', version: 4, ops: [] });
  expect(seen).toEqual(['snapshot 3', 'patch 4']);

  subscription.close();
  await flush();
  const last = socket?.requests().at(-1);
  expect(last?.method).toBe('projection.unsubscribe');
  expect(last?.params).toEqual({ subscription: 's1' });
});

test('the transport connects again with a pause that doubles up to a limit', async () => {
  const reconnects: number[] = [];
  const { t, sockets, timers } = transport({ onReconnect: () => reconnects.push(sockets.length) });
  void t.call('session.info', {}).catch(() => undefined);
  await flush();
  sockets[0]?.open();
  await flush();
  // The socket closes five times, and each time the next one does not open.
  sockets[0]?.close();
  for (let i = 1; i <= 5; i += 1) {
    timers.run();
    expect(sockets).toHaveLength(i + 1);
    sockets[i]?.close();
  }
  expect(timers.pauses).toEqual([100, 200, 400, 800, 800, 800]);
  // The next socket opens: the pause starts again, and the transport says that it is back.
  timers.run();
  sockets[6]?.open();
  expect(reconnects).toEqual([7]);
  sockets[6]?.close();
  expect(timers.pauses.at(-1)).toBe(100);
});

test('a call that waited for the socket fails when the socket closes, and a new call waits for the next one', async () => {
  const { t, sockets, timers } = transport();
  const first = t.call('session.info', {}).catch((error: unknown) => error);
  await flush();
  sockets[0]?.open();
  await flush();
  sockets[0]?.close();
  expect(((await first) as RpcError).code).toBe('ws_closed');

  const second = t.call('session.info', {});
  await flush();
  timers.run();
  sockets[1]?.open();
  await flush();
  const [request] = sockets[1]?.requests() ?? [];
  sockets[1]?.reply(request?.id ?? 0, { ok: true });
  expect(await second).toEqual({ ok: true } as never);
});

test('a subscription that the closed socket carried opens again with the last version', async () => {
  const { t, sockets, timers } = transport();
  const seen: string[] = [];
  t.subscribe('panes', {
    onSnapshot: (_data, version) => seen.push(`snapshot ${version}`),
    onPatch: (_ops, version) => seen.push(`patch ${version}`),
  });
  await flush();
  sockets[0]?.open();
  await flush();
  const first = sockets[0]?.requests()[0];
  sockets[0]?.reply(first?.id ?? 0, { subscription: 's1' });
  await flush();
  sockets[0]?.notify('s1', { type: 'snapshot', version: 9, data: data(9) });
  sockets[0]?.notify('s1', { type: 'patch', version: 10, ops: [] });

  sockets[0]?.close();
  timers.run();
  sockets[1]?.open();
  await flush();
  const again = sockets[1]?.requests()[0];
  expect(again?.method).toBe('projection.subscribe');
  expect(again?.params).toEqual({ name: 'panes', since: 10 });
  sockets[1]?.reply(again?.id ?? 0, { subscription: 's2' });
  await flush();
  sockets[1]?.notify('s2', { type: 'patch', version: 11, ops: [] });
  expect(seen).toEqual(['snapshot 9', 'patch 10', 'patch 11']);
});

test('a stream is a socket for binary frames, and it opens again when it closes', async () => {
  const { t, sockets, timers } = transport();
  const frames: number[] = [];
  const stream = t.stream('render', { window: 'w', viewport_hz: 60, proto: 1 } as never, (frame) =>
    frames.push(frame.byteLength),
  );
  const socket = sockets[0];
  expect(socket?.url).toStartWith('ws://127.0.0.1:4242/stream?token=secret&kind=render&params=');
  expect(decodeURIComponent(socket?.url.split('params=')[1] ?? '')).toBe(
    '{"window":"w","viewport_hz":60,"proto":1}',
  );
  expect(socket?.binaryType).toBe('arraybuffer');
  socket?.receive(new ArrayBuffer(8));
  socket?.receive('text is not a frame');
  expect(frames).toEqual([8]);

  socket?.close();
  timers.run();
  expect(sockets).toHaveLength(2);
  sockets[1]?.receive(new ArrayBuffer(3));
  expect(frames).toEqual([8, 3]);

  stream.close();
  expect(sockets[1]?.closed).toBe(true);
  timers.run();
  expect(sockets).toHaveLength(2);
});

test('the address of the page gives the port and the token', () => {
  const timers = manualTimers();
  const t = WebSocketTransport.fromUrl('http://localhost:5173/?ws=4242&token=abc', timers);
  expect(t.blobUrl('f'.repeat(64))).toBe(`http://127.0.0.1:4242/blob/${'f'.repeat(64)}?token=abc`);
  expect(() => WebSocketTransport.fromUrl('http://localhost:5173/', timers)).toThrow();
});

test('dispose ends the reconnection', async () => {
  const { t, sockets, timers } = transport();
  void t.call('session.info', {}).catch(() => undefined);
  await flush();
  sockets[0]?.open();
  t.dispose();
  timers.run();
  expect(sockets).toHaveLength(1);
});

test('a transport of the origin of the page has no token, and its blobs are paths', async () => {
  const sockets: FakeSocket[] = [];
  const timers = manualTimers();
  const page = { protocol: 'https:', host: 'mac.example.ts.net:8443' };
  const secure = WebSocketTransport.sameOrigin(timers, page);
  expect(secure.blobUrl('f'.repeat(64))).toBe(`/blob/${'f'.repeat(64)}`);
  // The fonts of the Mac come from the listener of the phone.
  expect(secure.fontsUrl('"Hack Nerd Font Mono", Menlo')).toBe(
    '/font/faces.css?families=%22Hack%20Nerd%20Font%20Mono%22%2C%20Menlo',
  );
  const t = new WebSocketTransport({
    origin: 'wss://mac.example.ts.net:8443',
    socket: (url) => {
      const socket = new FakeSocket(url);
      sockets.push(socket);
      return socket;
    },
    timers,
    watch: false,
  });
  void t.call('session.info', {}).catch(() => undefined);
  t.stream('render', { pane: 'p' } as never, () => undefined).close();
  await flush();
  expect(sockets.map((socket) => socket.url)).toEqual([
    'wss://mac.example.ts.net:8443/rpc',
    `wss://mac.example.ts.net:8443/stream?kind=render&params=${encodeURIComponent('{"pane":"p"}')}`,
  ]);
  t.dispose();
});

test('a transport of another machine names it in each socket and blob', async () => {
  const sockets: FakeSocket[] = [];
  const timers = manualTimers();
  const t = new WebSocketTransport({
    origin: 'wss://mac.example.ts.net:8443',
    machine: 'studio',
    socket: (url) => {
      const socket = new FakeSocket(url);
      sockets.push(socket);
      return socket;
    },
    timers,
    watch: false,
  });
  expect(t.blobUrl('f'.repeat(64))).toBe(`/blob/${'f'.repeat(64)}?machine=studio`);
  void t.call('session.info', {}).catch(() => undefined);
  t.stream('render', {} as never, () => undefined).close();
  await flush();
  expect(sockets.map((socket) => socket.url)).toEqual([
    'wss://mac.example.ts.net:8443/rpc?machine=studio',
    `wss://mac.example.ts.net:8443/stream?machine=studio&kind=render&params=${encodeURIComponent('{}')}`,
  ]);
  t.dispose();
  const page = { protocol: 'https:', host: 'mac.example.ts.net:8443' };
  expect(WebSocketTransport.sameOrigin(timers, page, 'build-1').blobUrl('a')).toBe(
    '/blob/a?machine=build-1',
  );
});

test('a page of plain HTTP gets a socket of plain WebSocket', async () => {
  const opened: string[] = [];
  const original = globalThis.WebSocket;
  // The default socket is the WebSocket of the page.
  globalThis.WebSocket = class {
    constructor(url: string) {
      opened.push(url);
    }
    close(): void {}
  } as never;
  try {
    const t = WebSocketTransport.sameOrigin(manualTimers(), {
      protocol: 'http:',
      host: '127.0.0.1:47310',
    });
    void t.call('session.info', {}).catch(() => undefined);
    await flush();
    expect(opened).toEqual(['ws://127.0.0.1:47310/rpc']);
    t.dispose();
  } finally {
    globalThis.WebSocket = original;
  }
});

test('the listeners of the connection hear each open and each loss', async () => {
  const { t, sockets, timers } = transport();
  const heard: boolean[] = [];
  const end = t.onConnection((open) => heard.push(open));
  void t.call('session.info', {}).catch(() => undefined);
  await flush();
  sockets[0]?.open();
  sockets[0]?.close();
  timers.run();
  sockets[1]?.close();
  expect(heard).toEqual([true, false, false]);
  end();
  timers.run();
  sockets[2]?.open();
  expect(heard).toEqual([true, false, false]);
  t.dispose();
});
