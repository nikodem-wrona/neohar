import { defineConfig } from '@playwright/test';

const port = Number(process.env.NEOHAR_KEYTEST_PORT ?? 4174);
// A Chromium for a second opinion. The matrix is about WebKit, the engine of the webview.
const chromium = process.env.NEOHAR_CHROMIUM;

export default defineConfig({
  testDir: '.',
  testMatch: '**/*.pw.ts',
  timeout: 120_000,
  workers: 1,
  fullyParallel: false,
  reporter: [['list']],
  outputDir: '../../target/keyboard-results',
  use: { baseURL: `http://127.0.0.1:${port}` },
  webServer: {
    command: 'bun serve.ts',
    url: `http://127.0.0.1:${port}/ready`,
    // A server from an earlier run would serve old pages.
    reuseExistingServer: false,
    timeout: 600_000,
    stdout: 'pipe',
    stderr: 'pipe',
  },
  projects: [
    { name: 'webkit', use: { browserName: 'webkit' } },
    ...(chromium
      ? [
          {
            name: 'chromium',
            use: {
              browserName: 'chromium' as const,
              launchOptions: { executablePath: chromium, args: ['--headless=new'] },
            },
          },
        ]
      : []),
  ],
});
