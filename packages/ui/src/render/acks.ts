// What the page owes the core after it painted (`render.ack`). An ack frees a slot of the
// frames in flight, so the page sends it once for each painted animation frame and only when
// something changed. A pane that needs a keyframe goes in the same call.

import type { PaneAck } from '@neohar/transport';

export interface AckParams {
  acks: PaneAck[];
  need_keyframe: number[];
}

export class AckBook {
  /** The newest revision painted for each pane, not yet acknowledged. */
  private readonly painted = new Map<number, number>();
  private readonly keyframes = new Set<number>();

  /** A frame of pane `pane` (the render id) with revision `rev` is on the screen. */
  paint(pane: number, rev: number): void {
    this.painted.set(pane, rev);
  }

  /** The decoder lacks the base of a delta for this pane. */
  needKeyframe(pane: number): void {
    this.keyframes.add(pane);
  }

  /** The pane is gone: the page owes it nothing. */
  forget(pane: number): void {
    this.painted.delete(pane);
    this.keyframes.delete(pane);
  }

  /** What to send now, or `null` when nothing changed. The book is empty afterwards. */
  take(): AckParams | null {
    if (this.painted.size === 0 && this.keyframes.size === 0) return null;
    const acks: PaneAck[] = [];
    for (const [pane, rev] of this.painted) acks.push({ pane, rev });
    const params = { acks, need_keyframe: [...this.keyframes] };
    this.painted.clear();
    this.keyframes.clear();
    return params;
  }
}
