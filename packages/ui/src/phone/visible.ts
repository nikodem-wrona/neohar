// Whether the phone app is open and visible. The core sends no push to a phone whose
// app the person sees. The app reports the fact at the start, on each `visibilitychange`, and
// again when its socket opens: the core forgets it when a connection ends.

import type { Transport } from '@neohar/transport';

/** The part of a document that the report reads. */
export interface VisibleSource {
  readonly visibilityState: string;
  addEventListener(type: 'visibilitychange', listener: () => void): void;
  removeEventListener(type: 'visibilitychange', listener: () => void): void;
}

/**
 * Sends `phone.visible` now and after each change. `resend` sends it again, also when nothing
 * changed. `stop` ends the report. A call that fails is not tried again: the next change or the
 * next connection sends the fact.
 */
export function startVisibleReport(
  transport: Pick<Transport, 'call'>,
  doc: VisibleSource,
): { resend(): void; stop(): void } {
  let last: boolean | undefined;
  const report = (force: boolean): void => {
    const visible = doc.visibilityState === 'visible';
    if (!force && visible === last) return;
    last = visible;
    // See above: the next change or the next connection sends the fact again.
    transport.call('phone.visible', { visible }).catch(() => undefined);
  };
  const onChange = (): void => report(false);
  doc.addEventListener('visibilitychange', onChange);
  report(true);
  return {
    resend: () => report(true),
    stop: () => doc.removeEventListener('visibilitychange', onChange),
  };
}
