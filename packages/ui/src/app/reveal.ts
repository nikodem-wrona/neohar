// A request of the core to show a card (`interaction.reveal`). The row of the window
// carries the interaction and a number that grows with each request, so that a second request for
// the same card still counts. The page acts once for each number.

import type { Reveal } from '@neohar/transport';

export interface Revealed {
  /** The number of the request that the page acted on last. */
  seen: number;
  /** The interaction to show, or `null` when there is nothing new. */
  show: string | null;
}

/** Decides what a row of the window asks for, given the last number that the page acted on. */
export function revealed(seen: number, reveal: Reveal | null | undefined): Revealed {
  if (!reveal || reveal.seq === seen) return { seen, show: null };
  return { seen: reveal.seq, show: reveal.interaction };
}
