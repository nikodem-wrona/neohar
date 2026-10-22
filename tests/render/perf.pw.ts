// The render budgets, measured on a real GPU. The numbers go to
// `target/render-results/perf-*.json` and to the log. The tests need NEOHAR_GPU=1.

import { mkdirSync, writeFileSync } from 'node:fs';
import { expect, test } from '@playwright/test';

test.skip(process.env.NEOHAR_GPU !== '1', 'the budgets are measured on a real GPU (NEOHAR_GPU=1)');

/** The project without a GPU runs the Canvas2D checks only. */
const onGpu = (project: string) => project !== 'chromium-cpu';

test.beforeEach(async ({ page }) => {
  await page.goto('/render.html');
  await page.waitForFunction(() => 'bench' in window);
});

function save(name: string, project: string, report: unknown): void {
  mkdirSync('../../target/render-results', { recursive: true });
  writeFileSync(
    `../../target/render-results/perf-${name}-${project}.json`,
    JSON.stringify(report, null, 2),
  );
  console.log(`${name}: ${JSON.stringify(report)}`);
}

// A full redraw of 200x60 costs at most 2 ms to decode and upload and 1 ms to draw. The times
// are of the CPU. The GPU time comes from a timer query, when the browser has one.
test('200x60 full redraw of one pane: decode and upload, and draw', async ({ page }, info) => {
  test.skip(!onGpu(info.project.name), 'WebGL2 on the CPU is not the target');
  const report = await page.evaluate(() =>
    window.bench.perf('/frames/redraw-200x60.frames', { renderer: 'webgl2' }, 60, 240),
  );
  save('redraw-200x60', info.project.name, report);
  expect(report.renderer).toBe('webgl2');
  expect(report.glError).toBe(0);
  expect(report.decodeApply.p99 + report.upload.p99).toBeLessThanOrEqual(2);
  expect(report.drawCpu.p99).toBeLessThanOrEqual(1);
});

test('20 panes with a spinner each', async ({ page }, info) => {
  test.skip(!onGpu(info.project.name), 'WebGL2 on the CPU is not the target');
  const report = await page.evaluate(() =>
    window.bench.perf('/frames/spinner-20.frames', { renderer: 'webgl2' }, 60, 600),
  );
  save('spinner-20', info.project.name, report);
  expect(report.glError).toBe(0);
  expect(report.frame.p99).toBeLessThanOrEqual(1000 / 120);
});

test('20 panes that redraw everything in every frame', async ({ page }, info) => {
  test.skip(!onGpu(info.project.name), 'WebGL2 on the CPU is not the target');
  const report = await page.evaluate(() =>
    window.bench.perf('/frames/redraw-20.frames', { renderer: 'webgl2' }, 40, 240),
  );
  save('redraw-20', info.project.name, report);
  expect(report.glError).toBe(0);
  expect(report.frame.p99).toBeLessThanOrEqual(1000 / 120);
});

// The loop plays one display frame for each requestAnimationFrame and counts the frames that
// the browser did not call on time. The work of a frame must fit in the interval of 120 Hz.
for (const [name, file, warm, count] of [
  ['scroll of 200x60', '/frames/scroll-200x60.frames', 40, 480],
  ['20 panes with a spinner', '/frames/spinner-20.frames', 90, 480],
  ['20 panes that redraw everything', '/frames/redraw-20.frames', 40, 480],
] as const) {
  test(`no missed frame: ${name}`, async ({ page }, info) => {
    test.skip(!onGpu(info.project.name), 'WebGL2 on the CPU is not the target');
    const report = await page.evaluate(
      async ([f, w, n]) => {
        const id = await window.bench.open(f as string, { renderer: 'webgl2' });
        await window.bench.loop(id, 0, w as number);
        await window.bench.loop(id, w as number, n as number);
        return window.bench.loopReport();
      },
      [file, warm, count],
    );
    save(`frames-${name.replaceAll(' ', '-')}`, info.project.name, report);
    expect(report.drawn).toBe(count);
    // The work of a frame, which is the renderer's, must fit in a frame of 120 Hz every time.
    expect(report.work.p99).toBeLessThanOrEqual(1000 / 120);
    // A frame can also be late for a reason outside the page: another process on the Mac. A
    // run of 480 frames may lose one that way (in about 1 run in 8, with a work of 1 to 2 ms in
    // every frame), so the check allows two. A renderer that is too slow loses many.
    expect(report.missed, `late frames at ${report.missedAt.join(', ')}`).toBeLessThanOrEqual(2);
  });
}

// Glass over a pane that scrolls: the compositor blurs the canvas under the glass in every
// frame. The frames must still come on time.
test('no missed frame: scroll of 200x60 under glass', async ({ page }, info) => {
  test.skip(!onGpu(info.project.name), 'WebGL2 on the CPU is not the target');
  const report = await page.evaluate(async () => {
    const id = await window.bench.open('/frames/scroll-200x60.frames', { renderer: 'webgl2' });
    window.bench.glass(true);
    await window.bench.loop(id, 0, 40);
    await window.bench.loop(id, 40, 480);
    window.bench.glass(false);
    return window.bench.loopReport();
  });
  save('frames-scroll-under-glass', info.project.name, report);
  expect(report.drawn).toBe(480);
  expect(report.work.p99).toBeLessThanOrEqual(1000 / 120);
  expect(report.missed, `late frames at ${report.missedAt.join(', ')}`).toBeLessThanOrEqual(2);
});

// The fallback is slower than the GPU, but a full redraw of the biggest pane must stay within a
// few display frames, and a frame that changes two rows within one.
test('the Canvas2D fallback: a full redraw and a spinner', async ({ page }, info) => {
  const redraw = await page.evaluate(() =>
    window.bench.perf('/frames/redraw-200x60.frames', { renderer: 'canvas2d' }, 20, 60),
  );
  save('canvas2d-redraw-200x60', info.project.name, redraw);
  const spinner = await page.evaluate(() =>
    window.bench.perf('/frames/spinner-20.frames', { renderer: 'canvas2d' }, 60, 200),
  );
  save('canvas2d-spinner-20', info.project.name, spinner);
  expect(redraw.renderer).toBe('canvas2d');
  expect(redraw.frame.p50).toBeLessThanOrEqual(100);
  expect(spinner.frame.p50).toBeLessThanOrEqual(1000 / 60);
});
