// The Canvas2D renderer paints the same cells as the WebGL2 renderer.

import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/render.html');
  await page.waitForFunction(() => 'bench' in window);
});

// The renderers use the same glyph bitmaps and the same formulas. The compositors round in
// different places, so a pixel may differ by a few levels of 255.
const TOLERANCE = 3;

test('every cell of the fixture screen is the same in both renderers', async ({ page }) => {
  const report = await page.evaluate(
    (tolerance) => window.bench.compare('/frames/screen.frames', tolerance, {}),
    TOLERANCE,
  );
  console.log(JSON.stringify({ ...report, firstMask: undefined }));
  console.log(report.firstMask.join('\n'));
  expect(report.cells).toBe(11 * 64 * 20);
  expect(report.atlasResets).toEqual({ webgl2: 0, canvas2d: 0 });
  expect(report.worst).toEqual([]);
  expect(report.cellsOverTolerance).toBe(0);
});

test('the cells are the same in both renderers with padding and round corners', async ({
  page,
}) => {
  const report = await page.evaluate(
    (tolerance) =>
      window.bench.compare('/frames/screen.frames', tolerance, { pad: 16, radius: 20 }),
    TOLERANCE,
  );
  expect(report.cells).toBe(11 * 64 * 20);
  expect(report.worst).toEqual([]);
  expect(report.cellsOverTolerance).toBe(0);
});

test('the round corners of a pane differ between the renderers only on their edge', async ({
  page,
}) => {
  const report = await page.evaluate(
    (tolerance) =>
      window.bench.compareShapes('/frames/screen.frames', tolerance, { pad: 16, radius: 20 }),
    TOLERANCE,
  );
  console.log(JSON.stringify(report));
  expect(report.awayFromEdges).toBe(0);
  // The corner outside the curve shows the gray ground, and the pane is dark inside it.
  expect(report.outside).toEqual([0x40, 0x40, 0x40]);
  expect(report.inside).not.toEqual([0x40, 0x40, 0x40]);
});

test('the renderers agree after the atlas fills up and resets', async ({ page }) => {
  const report = await page.evaluate(
    (tolerance) => window.bench.compare('/frames/glyphs.frames', tolerance, { slots: 300 }),
    TOLERANCE,
  );
  console.log(JSON.stringify(report));
  // The atlas had 300 slots and the frames show 200 new glyphs each, so it reset more than once.
  expect(report.atlasResets.webgl2).toBeGreaterThan(1);
  expect(report.atlasResets.canvas2d).toBeGreaterThan(1);
  expect(report.worst).toEqual([]);
  expect(report.cellsOverTolerance).toBe(0);
});
