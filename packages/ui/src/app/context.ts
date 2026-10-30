// What every component of a window reads: the transport for commands, the stores for what the
// core shows, and the id of this window.

import type { Method, Params, Transport } from '@neohar/transport';
import { createContext, useContext } from 'react';
import type { Platform } from '../input/clipboard';
import type { Keyboard } from '../input/keyboard';
import type { InputSender } from '../input/sender';
import type { RenderHost } from '../render';
import type { Stores } from '../stores';
import type { Failures } from './failures';
import type { Toasts } from './toasts';
import type { ViewState } from './view';

/**
 * The form of the shell: the desk of a window of the app, or the desk on a phone. The
 * phone has its own layout and touch, and it has no native window.
 */
export type Form = 'desk' | 'phone';

export interface App {
  transport: Transport;
  /**
   * Calls a method whose result the window does not need, such as a click on a button. A failure
   * shows in the banner of the window.
   */
  run<M extends Method>(method: M, params: Params<M>): void;
  /** Shows a failure in the banner of the window, for a call that the window waits for. */
  report(failure: unknown): void;
  /** The failure that the banner shows. */
  failures: Failures;
  /** The toasts in the bottom right corner, such as an answer that arrived. */
  toasts: Toasts;
  form: Form;
  stores: Stores;
  windowId: string;
  view: ViewState;
  /** What the window sends to its panes. */
  input: InputSender;
  /** The keyboard layer: it turns a DOM key event into a logical key. */
  keyboard: Keyboard;
  platform: Platform;
  /** The canvas and the renderer of the window. A page without a decoder has none. */
  render?: RenderHost;
  /**
   * Puts a file that the person attaches into the blob store of the machine and gives its hash,
   * for `pane.drop_blobs`. A phone has it. The desk drops files on a pane instead.
   */
  upload?: (file: File) => Promise<string>;
}

const AppContext = createContext<App | undefined>(undefined);

export const AppProvider = AppContext.Provider;

/** The app, or `undefined` outside of a window: a part that the phone app of one view uses too. */
export function useAppMaybe(): App | undefined {
  return useContext(AppContext);
}

export function useApp(): App {
  const app = useContext(AppContext);
  if (!app) throw new Error('a component reads the app outside of its provider');
  return app;
}
