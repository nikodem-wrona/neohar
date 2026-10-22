// The transport of the app: Tauri IPC. It calls three commands of the host, `rpc`,
// `subscribe` and `stream`, and no other. A test enforces that over the whole webview
// code (`scripts/tauri-commands.test.ts`).
//
// The host can refuse a subscription: a machine whose link is down, or a core that stopped. The
// transport then tells its failure listeners once, and it subscribes again after a pause that
// doubles up to a limit, as the WebSocket transport does after a lost socket. A stream that the
// host refuses goes to the failure listeners too.

import { Channel, invoke } from '@tauri-apps/api/core';
import { type RpcError, toRpcError } from './errors';
import type { Method, Params, ProjectionMessage, Result, StreamParamsMap } from './generated';
import { ProjectionWatch, type RawSubscribe } from './projection';
import type {
  BinaryStream,
  ProjectionHandlers,
  StreamKind,
  Subscription,
  Transport,
} from './types';
import type { ProjectionName } from './generated';
import type { Timers } from './websocket';

/** What the transport needs of Tauri: the three commands and `Channel`. A test gives its own. */
export interface TauriHost {
  rpc(args: { method: string; params: unknown }): Promise<unknown>;
  subscribe(args: {
    name: ProjectionName;
    since: number | null;
    channel: unknown;
  }): Promise<unknown>;
  stream(args: { kind: string; params: unknown; channel: unknown }): Promise<unknown>;
  channel<T>(onMessage: (message: T) => void): unknown;
}

const TAURI: TauriHost = {
  rpc: (args) => invoke('rpc', args),
  subscribe: (args) => invoke('subscribe', args),
  stream: (args) => invoke('stream', args),
  channel: (onMessage) => new Channel(onMessage),
};

export interface TauriTransportOptions {
  /**
   * The timers of the page (`realTimers` of `packages/ui/src/lib/timing.ts`). The transport waits
   * with them before it subscribes again after a refusal.
   */
  timers: Timers;
  /** The pause before the first new subscription, and the longest pause. */
  backoff?: { initialMs: number; maxMs: number };
  host?: TauriHost;
}

const DEFAULT_BACKOFF = { initialMs: 250, maxMs: 8_000 };

export class TauriTransport implements Transport {
  private readonly host: TauriHost;
  private readonly timers: Timers;
  private readonly backoff: { initialMs: number; maxMs: number };
  /** The refusals in a row of each projection. A subscription that the host takes resets it. */
  private readonly refusals = new Map<ProjectionName, number>();
  private readonly failureListeners = new Set<(failure: RpcError) => void>();

  /** Checks the subscriptions when the window comes back. */
  private readonly watch = new ProjectionWatch({
    probe: (names) => this.call('projection.versions', { names }).then((r) => r.versions),
  });

  constructor(options: TauriTransportOptions) {
    this.host = options.host ?? TAURI;
    this.timers = options.timers;
    this.backoff = options.backoff ?? DEFAULT_BACKOFF;
  }

  async call<M extends Method>(method: M, params: Params<M>): Promise<Result<M>> {
    try {
      return (await this.host.rpc({ method, params })) as Result<M>;
    } catch (failure) {
      throw toRpcError(failure);
    }
  }

  subscribe(name: ProjectionName, handlers: ProjectionHandlers): Subscription {
    return this.watch.subscribe(this.open, name, handlers);
  }

  onFailure(listener: (failure: RpcError) => void): () => void {
    this.failureListeners.add(listener);
    return () => this.failureListeners.delete(listener);
  }

  private fail(failure: RpcError): void {
    for (const listener of [...this.failureListeners]) listener(failure);
  }

  /**
   * One subscription of the host. The host answers with its id, and `close` ends it. A refusal
   * calls `onLost` after the pause, and the follower subscribes again.
   */
  private readonly open: RawSubscribe = (name, since, onMessage, onLost) => {
    const channel = this.host.channel<ProjectionMessage>(onMessage);
    let id: string | null = null;
    let closed = false;
    let retry: unknown = null;
    const end = (subscription: string): void => {
      // The subscription ends with its window or its core, so a failure here leaves nothing open.
      void this.call('projection.unsubscribe', { subscription }).catch(() => undefined);
    };
    this.host.subscribe({ name, since, channel }).then(
      (subscription) => {
        this.refusals.delete(name);
        if (closed) end(subscription as string);
        else id = subscription as string;
      },
      (failure: unknown) => {
        if (closed) return;
        const refusals = this.refusals.get(name) ?? 0;
        this.refusals.set(name, refusals + 1);
        // The first refusal in a row shows. The next ones are the same failure again.
        if (refusals === 0) this.fail(toRpcError(failure));
        const pause = Math.min(this.backoff.maxMs, this.backoff.initialMs * 2 ** refusals);
        retry = this.timers.set(() => {
          retry = null;
          if (!closed) onLost();
        }, pause);
      },
    );
    return {
      close: () => {
        closed = true;
        if (retry !== null) this.timers.clear(retry);
        if (id !== null) end(id);
      },
    };
  };

  stream<K extends StreamKind>(
    kind: K,
    params: StreamParamsMap[K],
    onFrame: (frame: ArrayBuffer) => void,
  ): BinaryStream {
    let closed = false;
    // A raw body arrives as an `ArrayBuffer`.
    const channel = this.host.channel<ArrayBuffer>((body) => {
      if (!closed) onFrame(body);
    });
    this.host.stream({ kind, params, channel }).catch((failure: unknown) => {
      if (!closed) this.fail(toRpcError(failure));
    });
    return {
      close: () => {
        closed = true;
        // A peek stream ends with its own call. The others end with the window.
        if (kind === 'peek') {
          const { pane } = params as StreamParamsMap['peek'];
          // The pane can be gone already, and then its peek ended with it.
          void this.call('pane.peek_close', { pane }).catch(() => undefined);
        }
      },
    };
  }

  blobUrl(hash: string): string {
    return `nh://blob/${hash}`;
  }

  fontsUrl(families: string): string {
    return `nh://font/faces.css?families=${encodeURIComponent(families)}`;
  }
}
