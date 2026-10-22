// What a card needs from the shell that shows it: the transport for the answer, the way that the
// answer names, how to show the pane of the card, and where a fault of the page shows. The desk names `card` and shows
// the pane in its window. The phone app names `phone` and has no pane to show.

import type { Transport, Via } from '@neohar/transport';
import { createContext, useContext } from 'solid-js';

export interface CardHost {
  transport: Transport;
  /** The way of each answer of a card: `card` on the desk, `phone` in the phone app. */
  via: Via;
  /** Shows the pane of a card. A shell without panes has none, and the card shows no Open. */
  open?: (pane: string) => void;
  /**
   * Shows a failure in the banner of the window. A card shows the failure of its own answer
   * itself: this is for a fault of the page.
   */
  report: (failure: unknown) => void;
}

const CardHostContext = createContext<CardHost>();

export const CardHostProvider = CardHostContext.Provider;

export function useCardHost(): CardHost {
  const host = useContext(CardHostContext);
  if (!host) throw new Error('a card reads its host outside of its provider');
  return host;
}
