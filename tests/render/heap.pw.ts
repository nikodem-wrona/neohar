// The renderer allocates nothing per frame after warm-up.
//
// Two measures, both from Chromium, because WebKit has no such interface:
//  - the garbage collector does not run at all during a loop of 10 s (a trace of the browser has
//    an event for each collection);
//  - the sampling heap profiler, with the objects that the collector freed included, says how
//    much the loop allocates. An idle loop (`requestAnimationFrame` and nothing else) gives the
//    allocations of the test and the profiler. The difference is the work of the renderer.

import { mkdirSync, writeFileSync } from 'node:fs';
import { type Browser, type CDPSession, expect, type Page, test } from '@playwright/test';

test.skip(({ browserName }) => browserName !== 'chromium', 'the profiler is a Chromium interface');

interface Node {
  id: number;
  callFrame: { functionName: string; url: string; lineNumber: number };
  children: Node[];
}

interface TraceEvent {
  name: string;
  ph: string;
  ts: number;
  args?: { data?: { message?: string } };
}

/** The bytes that each function of a heap profile allocated, from its samples. */
function bySource(profile: { head: Node; samples: Array<{ nodeId: number; size: number }> }) {
  const nodes = new Map<number, Node>();
  const walk = (n: Node) => {
    nodes.set(n.id, n);
    n.children.forEach(walk);
  };
  walk(profile.head);
  const totals = new Map<string, number>();
  let all = 0;
  for (const s of profile.samples) {
    const n = nodes.get(s.nodeId);
    if (!n) continue;
    const key = `${n.callFrame.functionName || '(anonymous)'} ${n.callFrame.url.split('/').pop()}:${n.callFrame.lineNumber}`;
    totals.set(key, (totals.get(key) ?? 0) + s.size);
    all += s.size;
  }
  const top = [...totals.entries()].sort((a, b) => b[1] - a[1]).slice(0, 12);
  return { all, top };
}

interface Run {
  sampledBytes: number;
  top: Array<[string, number]>;
  collections: string[];
  seconds: number;
}

/** Plays `frames` display frames and says what the heap profiler and the trace saw in them. */
async function run(
  page: Page,
  browser: Browser,
  cdp: CDPSession,
  id: number,
  from: number,
  frames: number,
  options: { stage?: number; garbage?: number } = {},
): Promise<Run> {
  await cdp.send('HeapProfiler.enable');
  // The forced collection belongs before the trace and the loop, not in them.
  await cdp.send('HeapProfiler.collectGarbage');
  await cdp.send('HeapProfiler.startSampling', {
    samplingInterval: 64,
    includeObjectsCollectedByMajorGC: true,
    includeObjectsCollectedByMinorGC: true,
  });
  await browser.startTracing(page, {
    categories: ['v8', 'devtools.timeline', 'disabled-by-default-v8.gc'],
  });
  await page.evaluate(
    ([i, f, n, stage, garbage]) => window.bench.loop(i!, f!, n!, false, stage!, garbage!),
    [id, from, frames, options.stage ?? 3, options.garbage ?? 0],
  );
  const trace = JSON.parse((await browser.stopTracing()).toString()) as {
    traceEvents: TraceEvent[];
  };
  const { profile } = await cdp.send('HeapProfiler.stopSampling');
  // The loop marks its start and end in the trace, so that the collections of the setup and the
  // teardown are left out.
  const mark = (message: string) =>
    trace.traceEvents.find((e) => e.name === 'TimeStamp' && e.args?.data?.message === message)?.ts;
  const [start, end] = [mark('nh-loop-start'), mark('nh-loop-end')];
  expect(start, 'the loop marks its start').toBeDefined();
  expect(end, 'the loop marks its end').toBeDefined();
  const collections = trace.traceEvents
    .filter((e) => (e.name === 'MinorGC' || e.name === 'MajorGC') && e.ph !== 'E')
    .filter((e) => e.ts >= start! && e.ts <= end!)
    .map((e) => e.name);
  const sources = bySource(profile as never);
  return {
    sampledBytes: sources.all,
    top: sources.top,
    collections,
    seconds: (end! - start!) / 1e6,
  };
}

test.beforeEach(async ({ page }, info) => {
  // A WebGL2 on the CPU makes the browser collect garbage for its own memory.
  test.skip(info.project.name === 'chromium-cpu', 'the allocation checks run on a GPU');
  await page.goto('/render.html');
  await page.waitForFunction(() => 'bench' in window);
});

test('the check sees a collector that runs: a loop that wastes 64 KB in each frame', async ({
  page,
  browser,
}) => {
  const cdp = await page.context().newCDPSession(page);
  const id = await page.evaluate(() =>
    window.bench.open('/frames/scroll-200x60.frames', { renderer: 'webgl2' }),
  );
  const result = await run(page, browser, cdp, id, 0, 300, { stage: 0, garbage: 64 * 1024 });
  console.log(
    `control: ${result.collections.length} collections in ${result.seconds.toFixed(1)} s`,
  );
  expect(result.collections.length).toBeGreaterThan(0);
});

for (const [name, file, warm, frames] of [
  ['scroll of 10 s', '/frames/scroll-200x60.frames', 40, 600],
  ['20 panes with a spinner', '/frames/spinner-20.frames', 90, 600],
] as const) {
  test(`no allocation per frame: ${name}`, async ({ page, browser }, info) => {
    const cdp = await page.context().newCDPSession(page);
    const id = await page.evaluate((f) => window.bench.open(f, { renderer: 'webgl2' }), file);
    // Warm-up: glyphs are drawn into the atlas, styles are known, the views exist.
    await page.evaluate(([i, n]) => window.bench.loop(i!, 0, n!), [id, warm]);
    const idle = await run(page, browser, cdp, id, warm, frames, { stage: 0 });
    const full = await run(page, browser, cdp, id, warm, frames);
    const report = await page.evaluate(() => window.bench.loopReport());
    mkdirSync('../../target/render-results', { recursive: true });
    writeFileSync(
      `../../target/render-results/heap-${name.replaceAll(' ', '-')}.json`,
      JSON.stringify({ frames, idle, full, report }, null, 2),
    );
    const extra = full.sampledBytes - idle.sampledBytes;
    console.log(
      `${name}: ${full.seconds.toFixed(1)} s, ${full.collections.length} collections; ` +
        `sampled ${full.sampledBytes} bytes, an idle loop ${idle.sampledBytes} ` +
        `(${(extra / frames).toFixed(1)} bytes per frame more)`,
    );
    for (const [key, bytes] of full.top) console.log(`  ${bytes}  ${key}`);
    info.annotations.push({ type: 'bytes per frame', description: (extra / frames).toFixed(1) });
    // The loop lasts 10 s at 60 frames per second.
    expect(full.seconds).toBeGreaterThan(8);
    expect(full.collections).toEqual([]);
    // About one boxed number a frame is what is left, and the collector never runs. A drawing
    // that allocated a few objects in each frame would be 100 times more.
    expect(extra / frames).toBeLessThan(64);
    expect(report.frames).toBe(frames);
    expect(report.drawn).toBeGreaterThan(frames / 2);
  });
}
