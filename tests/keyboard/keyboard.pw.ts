// The keyboard fidelity matrix in a browser: every case with browser events is
// dispatched at a surface as synthetic DOM events, the keyboard layer of the UI turns them into
// logical events, and `nh-keytest` encodes those with the real encoder. The logical events and the
// bytes must be the ones of the case.

import { expect, test } from '@playwright/test';

interface Case {
  name: string;
  platform: string;
  modes: string[];
  kitty: number;
  modify_other_keys: number;
  config: { macos_option_as_alt: string; backspace: string };
  events: Record<string, unknown>[];
  logical: unknown[];
  expect: string;
  delivery: string;
}

interface Result {
  name: string;
  logical: unknown[];
  encoded: { hex: string; delivery: string };
}

test('every case of the matrix gives the expected logical events and bytes', async ({
  page,
  request,
}) => {
  const cases = ((await (await request.get('/cases')).json()) as Case[]).filter(
    (c) => c.events.length > 0,
  );
  expect(cases.length).toBeGreaterThanOrEqual(400);
  await page.goto('/');

  // One round trip for the whole matrix: the page runs the events and asks the encoder.
  const results = await page.evaluate(async (list) => {
    const out: Result[] = [];
    for (const c of list) {
      const logical = window.keytest.run(c.events as never, {
        platform: c.platform === 'macos' ? 'macos' : 'other',
        optionAsAlt: c.config.macos_option_as_alt as never,
      });
      const response = await fetch('/encode', {
        method: 'POST',
        body: JSON.stringify({
          modes: c.modes,
          kitty: c.kitty,
          modify_other_keys: c.modify_other_keys,
          config: c.config,
          logical,
        }),
      });
      out.push({ name: c.name, logical, encoded: await response.json() });
    }
    return out;
  }, cases);

  const failures: string[] = [];
  for (const [i, c] of cases.entries()) {
    const got = results[i];
    if (!got || got.name !== c.name) {
      failures.push(`${c.name}: no result`);
      continue;
    }
    if (
      c.logical.length > 0 &&
      JSON.stringify(sorted(got.logical)) !== JSON.stringify(sorted(c.logical))
    ) {
      failures.push(
        `${c.name}: logical events\n    expected ${JSON.stringify(c.logical)}\n    got      ${JSON.stringify(got.logical)}`,
      );
    }
    if (got.encoded.hex !== c.expect.replace(/\s+/g, '').toLowerCase()) {
      failures.push(`${c.name}: bytes expected ${c.expect}, got ${got.encoded.hex}`);
    }
  }
  expect(
    failures,
    `${failures.length} of ${cases.length} cases failed:\n${failures.join('\n')}`,
  ).toEqual([]);
});

/** The value with the keys of every object in order, so that two equal values print equal. */
function sorted(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sorted);
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .sort(([a], [b]) => (a < b ? -1 : 1))
        .map(([k, v]) => [k, sorted(v)]),
    );
  }
  return value;
}

declare global {
  interface Window {
    keytest: { run(events: never, options: { platform: string; optionAsAlt: never }): unknown[] };
  }
}
