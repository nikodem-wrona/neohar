// The size of a pane in cells, and how often the page tells the core. A drag of a
// divider or of the window changes the box of a pane many times. The core gets one `pane.resize`
// 50 ms after the last change, and at most ten more while the drag goes on.

import type { Timers } from '../lib/timing';
import type { CellMetrics } from '../surface';

export interface Cells {
  cols: number;
  rows: number;
}

/** The cells that fit in a box of `w` by `h` device pixels. A pane keeps at least 2 by 1 cells. */
export function cellsFor(box: { w: number; h: number }, metrics: CellMetrics): Cells {
  return {
    cols: Math.max(2, Math.floor(box.w / metrics.cellW)),
    rows: Math.max(1, Math.floor(box.h / metrics.cellH)),
  };
}

export interface ResizeTiming {
  /** The quiet time after the last change that ends a drag. */
  debounceMs: number;
  /** The time between two sends while the changes go on. */
  maxWaitMs: number;
  /** How many sends before the last one a single drag can make. */
  maxIntermediate: number;
}

export const RESIZE_TIMING: ResizeTiming = { debounceMs: 50, maxWaitMs: 100, maxIntermediate: 10 };

export class ResizeScheduler<T> {
  private latest: { value: T } | null = null;
  private quiet: unknown = null;
  private cap: unknown = null;
  private intermediate = 0;

  constructor(
    private readonly send: (value: T) => void,
    private readonly timers: Timers,
    private readonly timing: ResizeTiming = RESIZE_TIMING,
  ) {}

  /** A new size. The last one of a drag is the one that the core keeps. */
  request(value: T): void {
    this.latest = { value };
    if (this.quiet !== null) this.timers.clear(this.quiet);
    this.quiet = this.timers.set(() => this.settle(), this.timing.debounceMs);
    this.cap ??= this.timers.set(() => this.tick(), this.timing.maxWaitMs);
  }

  /** Drops the size that waits. */
  cancel(): void {
    if (this.quiet !== null) this.timers.clear(this.quiet);
    if (this.cap !== null) this.timers.clear(this.cap);
    this.quiet = null;
    this.cap = null;
    this.latest = null;
    this.intermediate = 0;
  }

  /** The drag ended: send the last size. */
  private settle(): void {
    this.quiet = null;
    if (this.cap !== null) this.timers.clear(this.cap);
    this.cap = null;
    this.intermediate = 0;
    this.flush();
  }

  /** The drag goes on: send the latest size, unless the drag has used its share. */
  private tick(): void {
    this.cap = null;
    if (this.intermediate >= this.timing.maxIntermediate) return;
    this.intermediate++;
    this.flush();
    this.cap = this.timers.set(() => this.tick(), this.timing.maxWaitMs);
  }

  private flush(): void {
    const pending = this.latest;
    this.latest = null;
    if (pending) this.send(pending.value);
  }
}
