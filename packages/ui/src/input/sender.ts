// What the page sends to a pane (`pane.input`, `pane.select`). A key goes at once with a new
// `input_id`. A wheel, a mouse move and a drag of a selection can make many events for each
// frame, so they wait for the next animation frame and the core gets the last or the sum.
//
// With an outbox (the phone), a key or a text that the core did not answer stays, and
// it goes again with its `input_id` after the socket connects again. The holder applies an id once.

import {
  InputOutbox,
  type LogicalInput,
  type ModName,
  type PaneId,
  type SelectMode,
  type SelectPoint,
  type Transport,
} from '@neohar/transport';
import type { Frames } from '../lib/frames';
import { realTimers, type Timers } from '../lib/timing';
import { type InputIds, pageInputIds } from './ids';

export interface WheelDelta {
  col: number;
  row: number;
  x: number;
  y: number;
  mods: ModName[];
}

interface MoveEvent {
  col: number;
  row: number;
  mods: ModName[];
  button: 'left' | 'middle' | 'right' | 'none';
}

interface Gesture {
  anchor: SelectPoint;
  head: SelectPoint;
  mode: SelectMode;
}

/** The pause before an input that the core did not take goes again. */
const RETRY_MS = 500;

export class InputSender {
  private readonly wheels = new Map<PaneId, WheelDelta>();
  private readonly moves = new Map<PaneId, MoveEvent>();
  private readonly selects = new Map<PaneId, Gesture>();
  private pending: number | null = null;
  private readonly outbox: InputOutbox | null;
  private retry: unknown = null;
  private readonly timers: Timers;

  /**
   * `ids` gives each `input_id`: the counter of the page, so that all the senders of the page
   * share it. With `outbox`, an input without an answer goes again after `resend`.
   */
  constructor(
    private readonly transport: Transport,
    private readonly frames: Frames,
    private readonly ids: InputIds = pageInputIds,
    options: { outbox?: boolean; timers?: Timers } = {},
  ) {
    this.timers = options.timers ?? realTimers;
    this.outbox = options.outbox
      ? new InputOutbox(async (params) => {
          await this.transport.call('pane.input', params);
          // The answer of the call is the confirmation: the core took the input.
          this.outbox?.confirmed([params.input_id]);
        })
      : null;
  }

  /**
   * A new `input_id` for a method that the sender does not send, such as `agent.reply` and
   * `pane.drop_blobs`. Take each id here: an id from another counter can be the same as one of
   * this counter, and the holder drops the second input.
   */
  nextId(): number {
    return this.ids.take();
  }

  /**
   * Sends a key or a text at once. The promise does not fail: a key for a pane that ended is
   * lost, and the pane shows that it ended. The window does not show a failure for each key.
   */
  send(pane: PaneId, event: LogicalInput): Promise<unknown> {
    const params = { pane, input_id: this.nextId(), event };
    if (this.outbox) return this.outbox.submit(params).then(() => this.retryLater());
    return this.transport.call('pane.input', params).catch(() => undefined);
  }

  /**
   * Sends again, with their ids, the inputs that the core did not answer: after a reconnect. The
   * promise does not fail.
   */
  resend(): Promise<void> {
    return (this.outbox?.resend() ?? Promise.resolve()).then(() => this.retryLater());
  }

  /**
   * An input that the core did not take tries again in a moment, while it is young: a core that
   * starts again restores its panes before they take input. The outbox drops an old one.
   */
  private retryLater(): void {
    if (!this.outbox || this.outbox.size === 0 || this.retry !== null) return;
    this.retry = this.timers.set(() => {
      this.retry = null;
      this.resend();
    }, RETRY_MS);
  }

  /**
   * Sends pasted text. The promise does not fail: a failed call gives `undefined`, as for a key.
   */
  paste(pane: PaneId, text: string): Promise<unknown> {
    return this.transport
      .call('pane.paste', { pane, input_id: this.nextId(), text })
      .catch(() => undefined);
  }

  /**
   * Sends paths of the machine of the core, from the Files tab. The core quotes and
   * pastes them. A failure comes back to the caller, which shows its message.
   */
  insertPaths(pane: PaneId, paths: string[]): Promise<unknown> {
    return this.transport.call('pane.insert_paths', { pane, input_id: this.nextId(), paths });
  }

  /** Sends a mouse event at once: a press or a release. Anything that waited goes first. */
  mouse(pane: PaneId, event: Extract<LogicalInput, { type: 'mouse' }>): Promise<unknown> {
    this.flush();
    return this.send(pane, event);
  }

  /** Adds whole notches of the wheel. One event with their sum goes out in the next frame. */
  wheel(pane: PaneId, delta: WheelDelta): void {
    const kept = this.wheels.get(pane);
    this.wheels.set(
      pane,
      kept ? { ...delta, x: kept.x + delta.x, y: kept.y + delta.y } : { ...delta },
    );
    this.schedule();
  }

  /** A move of the pointer that a program asked for. The last one of a frame goes out. */
  move(pane: PaneId, event: MoveEvent): void {
    this.moves.set(pane, event);
    this.schedule();
  }

  /** A selection that grows with the pointer. The last one of a frame goes out. */
  select(pane: PaneId, gesture: Gesture): void {
    this.selects.set(pane, gesture);
    this.schedule();
  }

  /** Sends what waits now. A release of the button calls it, so that the last point is not lost. */
  flush(): void {
    if (this.pending !== null) {
      this.frames.cancel(this.pending);
      this.pending = null;
    }
    this.run();
  }

  private schedule(): void {
    this.pending ??= this.frames.request(() => {
      this.pending = null;
      this.run();
    });
  }

  private run(): void {
    const wheels = [...this.wheels];
    const moves = [...this.moves];
    const selects = [...this.selects];
    this.wheels.clear();
    this.moves.clear();
    this.selects.clear();
    for (const [pane, w] of wheels) {
      if (w.x === 0 && w.y === 0) continue;
      this.send(pane, {
        type: 'mouse',
        kind: 'wheel',
        col: w.col,
        row: w.row,
        wheel_x: w.x,
        wheel_y: w.y,
        mods: w.mods,
      });
    }
    for (const [pane, m] of moves) {
      this.send(pane, {
        type: 'mouse',
        kind: 'move',
        button: m.button,
        col: m.col,
        row: m.row,
        mods: m.mods,
      });
    }
    for (const [pane, g] of selects) {
      // A selection goes again with the next move of the pointer, and a pane that ended has none.
      void this.transport
        .call('pane.select', { pane, anchor: g.anchor, head: g.head, mode: g.mode })
        .catch(() => undefined);
    }
  }
}
