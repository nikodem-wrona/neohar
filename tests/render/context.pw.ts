// A lost WebGL context comes back and the window is painted again.

import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/render.html');
  await page.waitForFunction(() => 'bench' in window);
});

test('WEBGL_lose_context: the renderer recovers and repaints the same screen', async ({ page }) => {
  const report = await page.evaluate(() =>
    window.bench.contextLoss('/frames/screen.frames', { renderer: 'webgl2' }, 3),
  );
  console.log(JSON.stringify(report));
  expect(report.beforeWasBlank).toBe(false);
  // No frame is painted while the context is lost.
  expect(report.drewWhileLost).toBe(false);
  // After the restore, the window shows what a renderer that never lost its context shows for
  // the same frames, including the frame that arrived while the context was lost.
  expect(report.differentPixelsFromFresh).toBe(0);
  expect(report.glError).toBe(0);
});
