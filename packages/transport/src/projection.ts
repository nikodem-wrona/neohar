// A subscription that follows the versions of a projection.
//
// A projection sends a snapshot and then patches. Each patch has the version after the one before
// it. A patch with another version means that a message was lost, and the state of the UI is no
// longer the state of the core. A `resync` means that the core cannot send the patches that the
// client misses. In both cases the follower subscribes again: with `since` when it can ask for the
// missed patches, and without it when it needs a new snapshot.
//
// A subscription can also stop without a gap: a channel of the host delivers in order and
// holds every message after one that it lost or whose handler did not return, and the core drops a
// sink whose send fails. Neither tells the page. So a `ProjectionWatch` asks the core for the
// versions of the projections that it follows when the window comes back (the focus, the page
// visible again), and a follower whose version is behind subscribes again with `since`. It does
// not poll: everything else is push.

import type { ProjectionName } from './generated';
import type { ProjectionHandlers, Subscription } from './types';
import type { ProjectionMessage } from './generated';

/** The handle of a subscription that the transport opened. */
export interface RawHandle {
  close(): void;
}

/**
 * Opens one subscription in the transport. `since` is the version that the client has, or `null`
 * for a snapshot. `onLost` is called when the transport lost the subscription (a closed socket),
 * and the follower then opens a new one.
 */
export type RawSubscribe = (
  name: ProjectionName,
  since: number | null,
  onMessage: (message: ProjectionMessage) => void,
  onLost: () => void,
) => RawHandle;

export class ProjectionFollower implements Subscription {
  private generation = 0;
  /** The version of the last message that the handlers got. */
  private last: number | null = null;
  private raw: RawHandle | null = null;
  private closed = false;

  constructor(
    private readonly subscribe: RawSubscribe,
    readonly name: ProjectionName,
    private readonly handlers: ProjectionHandlers,
  ) {
    this.open(null);
  }

  close(): void {
    this.closed = true;
    this.generation += 1;
    this.raw?.close();
    this.raw = null;
  }

  /** The version of the last message, for a test and for the UI to show. */
  get version(): number | null {
    return this.last;
  }

  /**
   * Compares the version of the core with the last one that the handlers got. A core
   * ahead means that messages stopped on the way: the follower asks for the patches since its
   * version. A core behind means a core that started again: the follower takes a new snapshot.
   * Before the first snapshot there is nothing to compare.
   */
  check(version: number): void {
    if (this.closed || this.last === null) return;
    if (version > this.last) {
      this.open(this.last);
    } else if (version < this.last) {
      this.last = null;
      this.open(null);
    }
  }

  private open(since: number | null): void {
    this.raw?.close();
    const generation = ++this.generation;
    const current = (): boolean => generation === this.generation && !this.closed;
    this.raw = this.subscribe(
      this.name,
      since,
      (message) => {
        if (current()) this.receive(message);
      },
      () => {
        // The transport lost the subscription. The follower asks for what it missed.
        if (current()) this.open(this.last);
      },
    );
  }

  private receive(message: ProjectionMessage): void {
    switch (message.type) {
      case 'snapshot':
        this.last = message.version;
        this.deliver(() => this.handlers.onSnapshot(message.data, message.version), false);
        return;
      case 'patch':
        if (this.last !== null && message.version <= this.last) return; // A repeat.
        if (this.last === null || message.version !== this.last + 1) {
          // A patch is missing. Ask for the patches after the last one that the UI has.
          this.open(this.last);
          return;
        }
        this.last = message.version;
        this.deliver(() => this.handlers.onPatch(message.ops, message.version), true);
        return;
      case 'resync':
        this.handlers.onResync?.();
        this.last = null;
        this.open(null);
        return;
    }
  }

  /**
   * Runs a handler. A handler that fails must not stop the messages after it: the channel of the
   * host holds every later message until the handler of this one returns. After a failed patch
   * the state of the handlers is not known, so the follower takes a new snapshot.
   */
  private deliver(handler: () => void, patch: boolean): void {
    try {
      handler();
    } catch (error) {
      console.error(`neohar: a handler of the projection ${this.name} failed`, error);
      if (patch) {
        this.last = null;
        this.open(null);
      }
    }
  }
}

/** Where a wakeup of the watch comes from: an event of a target of the page. */
export type Wakeup = readonly [target: EventTarget, event: string];

export interface ProjectionWatchOptions {
  /** Asks the core for the versions of these projections: `projection.versions {names}`. */
  probe: (names: ProjectionName[]) => Promise<Record<string, number>>;
  /**
   * The events that make the watch ask. The default is the focus of the window and the page
   * becoming visible.
   */
  wakeups?: readonly Wakeup[];
}

/**
 * Follows the versions of every open subscription of a transport. When the window gets
 * the focus or the page becomes visible, it asks the core for the versions and tells each follower
 * its own. The watch listens while it has a subscription.
 */
export class ProjectionWatch {
  private readonly followers = new Set<ProjectionFollower>();
  private running = false;
  private inFlight = false;
  private readonly onWake = (): void => {
    if (typeof document !== 'undefined' && document.visibilityState === 'hidden') return;
    // `tick` catches its own failures.
    this.tick().catch(() => undefined);
  };

  constructor(private readonly options: ProjectionWatchOptions) {}

  /** Opens a subscription that the watch checks until it closes. */
  subscribe(
    subscribe: RawSubscribe,
    name: ProjectionName,
    handlers: ProjectionHandlers,
  ): Subscription {
    const follower = new ProjectionFollower(subscribe, name, handlers);
    this.followers.add(follower);
    if (!this.running) this.start();
    return {
      name,
      close: () => {
        follower.close();
        this.followers.delete(follower);
        if (this.followers.size === 0) this.stop();
      },
    };
  }

  /** How many subscriptions the watch follows, for a test. */
  get size(): number {
    return this.followers.size;
  }

  /**
   * Asks the core for the versions once and tells each follower its own. A probe that fails is a
   * connection that is down or a core that is gone, and the transport handles those by itself.
   */
  async tick(): Promise<void> {
    if (this.inFlight || this.followers.size === 0) return;
    this.inFlight = true;
    try {
      const names = [...new Set([...this.followers].map((follower) => follower.name))];
      const versions = await this.options.probe(names);
      for (const follower of this.followers) {
        const version = versions[follower.name];
        if (typeof version === 'number') follower.check(version);
      }
    } catch {
      // See above.
    } finally {
      this.inFlight = false;
    }
  }

  private start(): void {
    this.running = true;
    for (const [target, event] of this.wakeups()) target.addEventListener(event, this.onWake);
  }

  private stop(): void {
    this.running = false;
    for (const [target, event] of this.wakeups()) target.removeEventListener(event, this.onWake);
  }

  private wakeups(): readonly Wakeup[] {
    if (this.options.wakeups) return this.options.wakeups;
    const out: Wakeup[] = [];
    if (typeof window !== 'undefined') out.push([window, 'focus']);
    if (typeof document !== 'undefined') out.push([document, 'visibilitychange']);
    return out;
  }
}
