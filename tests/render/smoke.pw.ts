import { writeFileSync } from 'node:fs';
import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/render.html');
  await page.waitForFunction(() => 'bench' in window);
});

for (const renderer of ['webgl2', 'canvas2d'] as const) {
  test(`the fixture screen is painted by ${renderer}`, async ({ page }, info) => {
    const result = await page.evaluate(async (kind) => {
      const id = await window.bench.open('/frames/screen.frames', { renderer: kind });
      const png = window.bench.snapshot(id, 0);
      return { info: window.bench.info(id), png };
    }, renderer);
    const bytes = Buffer.from(result.png.split(',')[1]!, 'base64');
    await info.attach(`${renderer}-tick0.png`, { body: bytes, contentType: 'image/png' });
    writeFileSync(`../../target/render-results/${renderer}-${info.project.name}-tick0.png`, bytes);
    expect(result.info.kind).toBe(renderer);
    expect(bytes.length).toBeGreaterThan(5000);
  });
}
