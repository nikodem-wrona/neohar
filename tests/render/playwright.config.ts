import { defineConfig } from '@playwright/test';

const port = Number(process.env.NEOHAR_RENDER_PORT ?? 4173);
// Chromium for the checks that need CDP (the heap). Set it to a Chrome for Testing that exists.
const chromium = process.env.NEOHAR_CHROMIUM;

export default defineConfig({
  testDir: '.',
  testMatch: '**/*.pw.ts',
  timeout: 240_000,
  workers: 1,
  fullyParallel: false,
  reporter: [['list']],
  outputDir: '../../target/render-results',
  use: { baseURL: `http://127.0.0.1:${port}` },
  webServer: {
    command: 'bun serve.ts',
    url: `http://127.0.0.1:${port}/ready`,
    reuseExistingServer: true,
    timeout: 600_000,
    stdout: 'pipe',
    stderr: 'pipe',
  },
  projects: [
    // A browser without a GPU: the case that Canvas2D is for.
    {
      name: 'chromium-cpu',
      use: {
        browserName: 'chromium',
        deviceScaleFactor: 2,
        launchOptions: {
          ...(chromium ? { executablePath: chromium } : {}),
          args: ['--headless=new', '--disable-gpu', '--disable-accelerated-2d-canvas'],
        },
      },
    },
    { name: 'webkit', use: { browserName: 'webkit', deviceScaleFactor: 2 } },
    {
      name: 'chromium',
      use: {
        browserName: 'chromium',
        deviceScaleFactor: 2,
        launchOptions: {
          ...(chromium ? { executablePath: chromium } : {}),
          args: ['--headless=new', '--use-angle=metal', '--ignore-gpu-blocklist', '--enable-gpu'],
        },
      },
    },
  ],
});
