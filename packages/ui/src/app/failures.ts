// The failures of the commands of a window. Most clicks and keys send a command and do not wait
// for its result: `run` sends it, and a failure shows in the banner over the tab bar. A promise
// that the page did not handle (`unhandledrejection`) shows there too. The banner shows the last
// failure until the person closes it, or for `FAILURE_SHOW_MS`.

import { type Method, type Params, RpcError, type Transport } from '@neohar/transport';
import { type Accessor, createSignal } from 'solid-js';
import { realTimers, type Timers } from '../lib/timing';

/** What the banner shows. */
export interface Failure {
  /** One more for each failure, so that the same failure again shows as new. */
  id: number;
  /** The code of the core or of the transport, or `error` for a fault of the page. */
  code: string;
  message: string;
}

/** How long the banner shows a failure. */
export const FAILURE_SHOW_MS = 10_000;

export interface Failures {
  /** Calls a method whose result the window does not need. A failure shows in the banner. */
  run<M extends Method>(method: M, params: Params<M>): void;
  /** Shows a failure in the banner: an `RpcError`, an `Error` or any other rejected value. */
  report(failure: unknown): void;
  /** The failure that the banner shows. */
  shown: Accessor<Failure | null>;
  /** Removes the failure from the banner. */
  dismiss(): void;
}

/** The code and the message of a failure. */
function partsOf(failure: unknown): { code: string; message: string } {
  if (failure instanceof RpcError) return { code: failure.code, message: failure.message };
  if (failure instanceof Error) return { code: 'error', message: failure.message };
  return { code: 'error', message: String(failure) };
}

export function createFailures(
  transport: Pick<Transport, 'call'>,
  timers: Timers = realTimers,
): Failures {
  const [shown, setShown] = createSignal<Failure | null>(null);
  let count = 0;
  let timer: unknown = null;
  const dismiss = (): void => {
    if (timer !== null) timers.clear(timer);
    timer = null;
    setShown(null);
  };
  const report = (failure: unknown): void => {
    dismiss();
    count += 1;
    setShown({ id: count, ...partsOf(failure) });
    timer = timers.set(() => {
      timer = null;
      setShown(null);
    }, FAILURE_SHOW_MS);
  };
  return {
    run: (method, params) => {
      transport.call(method, params).catch(report);
    },
    report,
    shown,
    dismiss,
  };
}

/**
 * Shows each promise of the page that nobody handled in the banner. The browser also writes it to
 * the console, as before. Returns the function that stops it.
 */
export function reportRejections(
  page: EventTarget,
  report: (failure: unknown) => void,
): () => void {
  const onRejection = (event: Event): void => {
    report((event as PromiseRejectionEvent).reason);
  };
  page.addEventListener('unhandledrejection', onRejection);
  return () => page.removeEventListener('unhandledrejection', onRejection);
}
