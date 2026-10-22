// The transport over a WebSocket of the core. It has two addresses:
//
// - The dev WebSocket, a test seam for tests in a browser. It
//   speaks to `127.0.0.1` only, with the token that the core made when it started the server.
// - The origin of the page, for the phone app. The cookie of the phone is the
//   credential, so the address has no token (`sameOrigin`).
//
//   control socket  /rpc?token=…                        JSON-RPC 2.0 in text messages
//   stream sockets  /stream?token=…&kind=…&params=…     one binary message for each frame
//
// The control socket speaks the protocol of `api.sock`: a call is a request, and
// `projection.subscribe` answers with an id and then sends `projection.message` notifications.
// When the socket closes, the transport connects again after a pause that doubles up to a limit.
// The pause is set by the close event of the socket, and there is no polling. A subscription that
// the socket lost is opened again with `since`, and the inputs of the outbox go out again.

import { RpcError, toRpcError } from './errors';
import type { Method, Params, ProjectionMessage, ProjectionName, Result } from './generated';
import type { StreamParamsMap } from './generated';
import { ProjectionWatch, type RawSubscribe } from './projection';
import type {
  BinaryStream,
  ProjectionHandlers,
  StreamKind,
  Subscription,
  Transport,
} from './types';

/** What the transport needs of a socket. A browser `WebSocket` has it. */
export interface SocketLike {
  binaryType: string;
  onopen: ((event: unknown) => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
  onclose: ((event: unknown) => void) | null;
  onerror: ((event: unknown) => void) | null;
  send(data: string): void;
  close(): void;
}

export interface Timers {
  set(callback: () => void, ms: number): unknown;
  clear(handle: unknown): void;
}

/** Where the sockets of the transport go. */
export type WebSocketAddress =
  /** The dev WebSocket of the core on `127.0.0.1`, with its token. */
  | { port: number; token: string }
  /**
   * The origin of the page, such as `https://mac.example.ts.net:8443`, with no token.
   * `machine` is another machine of the Mac that the sockets go to.
   */
  | { origin: string; machine?: string };

export type WebSocketTransportOptions = WebSocketAddress & WebSocketTransportSettings;

export interface WebSocketTransportSettings {
  /** Makes a socket. The default is the `WebSocket` of the page. */
  socket?: (url: string) => SocketLike;
  /**
   * The timers of the page (`realTimers` of `packages/ui/src/lib/timing.ts`). The transport waits
   * with them before it connects again. Timers live in that one file.
   */
  timers: Timers;
  /** The pause before the first new connection, and the longest pause. */
  backoff?: { initialMs: number; maxMs: number };
  /** Called each time the control socket is open again after a loss. */
  onReconnect?: () => void;
  /**
   * Whether the transport checks the versions of its subscriptions when the window comes back.
   * The default is yes. A test of the socket alone turns it off.
   */
  watch?: boolean;
}

const DEFAULT_BACKOFF = { initialMs: 250, maxMs: 8_000 };

/** The messages for a subscription that arrive before the answer that names it. */
const EARLY_LIMIT = 1_000;

interface Reply {
  id?: number;
  result?: unknown;
  error?: { code: number; message: string; data?: { code: string; details?: unknown } };
  method?: string;
  params?: { subscription: string; message: ProjectionMessage };
}

interface Pending {
  resolve: (value: unknown) => void;
  reject: (error: RpcError) => void;
}

export class WebSocketTransport implements Transport {
  private readonly makeSocket: (url: string) => SocketLike;
  private readonly timers: Timers;
  private readonly backoff: { initialMs: number; maxMs: number };
  private socket: SocketLike | null = null;
  private isOpen = false;
  private lost = false;
  private disposed = false;
  private attempt = 0;
  private retry: unknown = null;
  private nextId = 1;
  private readonly pending = new Map<number, Pending>();
  /** Where the notifications of each subscription go. */
  private readonly listeners = new Map<string, (message: ProjectionMessage) => void>();
  private readonly early = new Map<string, ProjectionMessage[]>();
  /** Called when the socket closes: the subscriptions that it carried. */
  private readonly onLost = new Set<() => void>();
  private waiting: (() => void)[] = [];
  private readonly watch: ProjectionWatch;
  /** Who wants to know when the control socket opens and closes. */
  private readonly connection = new Set<(open: boolean) => void>();

  constructor(private readonly options: WebSocketTransportOptions) {
    this.makeSocket = options.socket ?? ((url) => new WebSocket(url) as unknown as SocketLike);
    this.timers = options.timers;
    this.backoff = options.backoff ?? DEFAULT_BACKOFF;
    this.watch = new ProjectionWatch({
      probe: (names) => this.call('projection.versions', { names }).then((r) => r.versions),
      ...(options.watch === false ? { wakeups: [] } : {}),
    });
  }

  /** A transport for the page address `…?ws=<port>&token=<token>`. */
  static fromUrl(address: string, timers: Timers): WebSocketTransport {
    const url = new URL(address);
    const port = Number(url.searchParams.get('ws'));
    const token = url.searchParams.get('token');
    if (!Number.isInteger(port) || port <= 0 || !token) {
      throw new RpcError('transport_failed', 'the address has no ws port and no token');
    }
    return new WebSocketTransport({ port, token, timers });
  }

  /**
   * A transport for the phone app: the sockets go to the origin of the page, `wss://` for a page
   * of `https://`. The browser sends the cookie of the phone, and the listener checks it and the
   * `Origin` of the socket.
   */
  static sameOrigin(
    timers: Timers,
    page: { protocol: string; host: string } = location,
    machine?: string,
  ): WebSocketTransport {
    const scheme = page.protocol === 'https:' ? 'wss' : 'ws';
    const origin = `${scheme}://${page.host}`;
    return new WebSocketTransport(machine ? { origin, machine, timers } : { origin, timers });
  }

  /** The query part that names the machine of the transport: empty for the Mac itself. */
  private machineQuery(): string {
    const options = this.options;
    return 'origin' in options && options.machine
      ? `&machine=${encodeURIComponent(options.machine)}`
      : '';
  }

  private url(path: string, query = ''): string {
    const options = this.options;
    if ('origin' in options) {
      const all = `${this.machineQuery()}${query}`;
      return `${options.origin}${path}${all && `?${all.slice(1)}`}`;
    }
    return `ws://127.0.0.1:${options.port}${path}?token=${options.token}${query}`;
  }

  /**
   * Calls `listener` with `true` each time the control socket opens, and with `false` each time
   * it closes or cannot open. Returns the function that ends it. The phone app shows a banner
   * from it.
   */
  onConnection(listener: (open: boolean) => void): () => void {
    this.connection.add(listener);
    return () => this.connection.delete(listener);
  }

  /** Ends the transport: no reconnection, and every call that waits fails. */
  dispose(): void {
    this.disposed = true;
    if (this.retry !== null) this.timers.clear(this.retry);
    this.socket?.close();
  }

  // --- the control socket ---------------------------------------------------------------

  private connect(): void {
    if (this.socket || this.disposed) return;
    const socket = this.makeSocket(this.url('/rpc'));
    this.socket = socket;
    socket.onopen = () => {
      this.isOpen = true;
      this.attempt = 0;
      const reconnected = this.lost;
      this.lost = false;
      const waiting = this.waiting;
      this.waiting = [];
      for (const wake of waiting) wake();
      if (reconnected) this.options.onReconnect?.();
      for (const listener of [...this.connection]) listener(true);
    };
    socket.onmessage = (event) => this.onMessage(event.data);
    socket.onclose = () => this.onClose(socket);
    socket.onerror = () => undefined; // The close event follows.
  }

  private onClose(socket: SocketLike): void {
    if (this.socket !== socket) return;
    this.socket = null;
    this.isOpen = false;
    this.lost = true;
    const failure = new RpcError('ws_closed', 'the control socket closed');
    for (const pending of this.pending.values()) pending.reject(failure);
    this.pending.clear();
    this.listeners.clear();
    this.early.clear();
    const lost = [...this.onLost];
    this.onLost.clear();
    for (const callback of lost) callback();
    for (const listener of [...this.connection]) listener(false);
    if (this.disposed) return;
    const pause = Math.min(this.backoff.maxMs, this.backoff.initialMs * 2 ** this.attempt);
    this.attempt += 1;
    this.retry = this.timers.set(() => {
      this.retry = null;
      this.connect();
    }, pause);
  }

  /** Resolves when the control socket is open. */
  private ready(): Promise<void> {
    if (this.isOpen) return Promise.resolve();
    return new Promise((resolve, reject) => {
      if (this.disposed) {
        reject(new RpcError('ws_closed', 'the transport has ended'));
        return;
      }
      this.waiting.push(resolve);
      this.connect();
    });
  }

  private onMessage(data: unknown): void {
    if (typeof data !== 'string') return;
    const reply = JSON.parse(data) as Reply;
    if (reply.method === 'projection.message' && reply.params) {
      const { subscription, message } = reply.params;
      const listener = this.listeners.get(subscription);
      if (listener) {
        listener(message);
      } else {
        const queue = this.early.get(subscription) ?? [];
        if (queue.length < EARLY_LIMIT) queue.push(message);
        this.early.set(subscription, queue);
      }
      return;
    }
    if (reply.id === undefined) return;
    const pending = this.pending.get(reply.id);
    if (!pending) return;
    this.pending.delete(reply.id);
    if (reply.error) pending.reject(toRpcError(reply.error));
    else pending.resolve(reply.result);
  }

  private async request(method: string, params: unknown): Promise<unknown> {
    await this.ready();
    const socket = this.socket;
    if (!socket) throw new RpcError('ws_closed', 'the control socket closed');
    const id = this.nextId++;
    const done = new Promise<unknown>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
    });
    socket.send(JSON.stringify({ jsonrpc: '2.0', id, method, params }));
    return done;
  }

  // --- Transport ------------------------------------------------------------------------

  async call<M extends Method>(method: M, params: Params<M>): Promise<Result<M>> {
    try {
      return (await this.request(method, params)) as Result<M>;
    } catch (failure) {
      throw toRpcError(failure);
    }
  }

  subscribe(name: ProjectionName, handlers: ProjectionHandlers): Subscription {
    return this.watch.subscribe(this.open, name, handlers);
  }

  private readonly open: RawSubscribe = (name, since, onMessage, onLost) => {
    let id: string | null = null;
    let closed = false;
    const lost = (): void => {
      // The socket took the subscription with it, so there is nothing left to end.
      id = null;
      if (!closed) onLost();
    };
    this.onLost.add(lost);
    this.request('projection.subscribe', since === null ? { name } : { name, since }).then(
      (result) => {
        const subscription = (result as { subscription: string }).subscription;
        if (closed) {
          // A subscription that the core does not know any more has nothing left to end.
          void this.request('projection.unsubscribe', { subscription }).catch(() => undefined);
          return;
        }
        id = subscription;
        this.listeners.set(subscription, onMessage);
        for (const message of this.early.get(subscription) ?? []) onMessage(message);
        this.early.delete(subscription);
      },
      lost,
    );
    return {
      close: () => {
        closed = true;
        this.onLost.delete(lost);
        if (id !== null) {
          this.listeners.delete(id);
          // A subscription that the core does not know any more has nothing left to end.
          void this.request('projection.unsubscribe', { subscription: id }).catch(() => undefined);
        }
      },
    };
  };

  stream<K extends StreamKind>(
    kind: K,
    params: StreamParamsMap[K],
    onFrame: (frame: ArrayBuffer) => void,
  ): BinaryStream {
    let closed = false;
    let socket: SocketLike | null = null;
    let attempt = 0;
    let retry: unknown = null;
    const query = `&kind=${encodeURIComponent(kind)}&params=${encodeURIComponent(JSON.stringify(params))}`;
    const connect = (): void => {
      if (closed || this.disposed) return;
      const next = this.makeSocket(this.url('/stream', query));
      socket = next;
      // The view shares the memory of the message, so nothing is copied.
      next.binaryType = 'arraybuffer';
      next.onopen = () => {
        attempt = 0;
      };
      next.onmessage = (event) => {
        if (event.data instanceof ArrayBuffer) onFrame(event.data);
      };
      next.onerror = () => undefined;
      next.onclose = () => {
        if (socket !== next || closed || this.disposed) return;
        // The core sends a keyframe first on a new stream, so the client starts again from one.
        const pause = Math.min(this.backoff.maxMs, this.backoff.initialMs * 2 ** attempt);
        attempt += 1;
        retry = this.timers.set(connect, pause);
      };
    };
    connect();
    return {
      close: () => {
        closed = true;
        if (retry !== null) this.timers.clear(retry);
        socket?.close();
      },
    };
  }

  blobUrl(hash: string): string {
    const options = this.options;
    // The page and the blob have one origin, so the cookie goes with the request.
    if ('origin' in options) {
      const machine = this.machineQuery();
      return `/blob/${hash}${machine && `?${machine.slice(1)}`}`;
    }
    return `http://127.0.0.1:${options.port}/blob/${hash}?token=${options.token}`;
  }

  fontsUrl(families: string): string {
    const query = `families=${encodeURIComponent(families)}`;
    // The listener of the phone serves the fonts of the Mac. The dev WebSocket has no
    // font route, and a page in a test gets an error for the sheet and goes on at once.
    if ('origin' in this.options) return `/font/faces.css?${query}`;
    return `nh://font/faces.css?${query}`;
  }
}
