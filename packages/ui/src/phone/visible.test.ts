import { describe, expect, test } from 'bun:test';
import type { Transport } from '@neohar/transport';
import { startVisibleReport, type VisibleSource } from './visible';

function page(state: string) {
  const listeners = new Set<() => void>();
  const doc = {
    visibilityState: state,
    addEventListener: (_: string, listener: () => void) => listeners.add(listener),
    removeEventListener: (_: string, listener: () => void) => listeners.delete(listener),
  };
  const change = (next: string): void => {
    doc.visibilityState = next;
    for (const listener of listeners) listener();
  };
  return { doc: doc as VisibleSource, change, listeners };
}

function transport() {
  const sent: unknown[] = [];
  const fake = {
    call: (method: string, params: unknown) => {
      sent.push({ method, params });
      return Promise.resolve({});
    },
  } as unknown as Pick<Transport, 'call'>;
  return { fake, sent };
}

describe('phone.visible', () => {
  test('goes at the start and on each change of the visibility', () => {
    const t = transport();
    const p = page('visible');
    const report = startVisibleReport(t.fake, p.doc);
    p.change('hidden');
    p.change('hidden');
    p.change('visible');
    expect(t.sent).toEqual([
      { method: 'phone.visible', params: { visible: true } },
      { method: 'phone.visible', params: { visible: false } },
      { method: 'phone.visible', params: { visible: true } },
    ]);
    report.stop();
    expect(p.listeners.size).toBe(0);
  });

  test('goes again when the socket opens again, also with no change', () => {
    const t = transport();
    const report = startVisibleReport(t.fake, page('visible').doc);
    report.resend();
    expect(t.sent).toHaveLength(2);
    report.stop();
  });

  test('a call that fails does not throw', async () => {
    const fake = {
      call: () => Promise.reject(new Error('closed')),
    } as unknown as Pick<Transport, 'call'>;
    const report = startVisibleReport(fake, page('hidden').doc);
    await Promise.resolve();
    report.stop();
  });
});
