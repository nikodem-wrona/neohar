import { defineConfig, devices } from '@playwright/test';

const port = Number(process.env.NEOHAR_UI_PORT ?? 4178);

/**
 * The target phone: an iPhone 17 Pro Max with iOS 26, as the app of the home screen shows it,
 * with no bar of Safari. Playwright has no such device. The size and the agent are not yet checked
 * on a real phone.
 */
const iphone17ProMax = {
  ...devices['iPhone 15'],
  viewport: { width: 440, height: 956 },
  screen: { width: 440, height: 956 },
  deviceScaleFactor: 3,
  isMobile: true,
  hasTouch: true,
  userAgent:
    'Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Mobile/15E148 Safari/604.1',
};

// The phone app in WebKit. The test server starts the core with the listener of the phone, and
// each spec opens the page of the listener itself. The iPhone 15 keeps one test of the narrow
// width.
export default defineConfig({
  testDir: '.',
  testMatch: '**/*.pw.ts',
  timeout: 90_000,
  workers: 1,
  fullyParallel: false,
  reporter: [['list']],
  outputDir: '../../target/phone-results',
  use: { baseURL: `http://127.0.0.1:${port}` },
  webServer: {
    command: 'bun serve.ts',
    url: `http://127.0.0.1:${port}/ready`,
    // A server from an earlier run would serve an old page and an old core.
    reuseExistingServer: false,
    // The server stops the programs of the core when it gets this signal. The default kills it.
    gracefulShutdown: { signal: 'SIGTERM', timeout: 30_000 },
    timeout: 600_000,
    stdout: 'pipe',
    stderr: 'pipe',
  },
  projects: [
    { name: 'iphone-17-pro-max', use: iphone17ProMax, grepInvert: /@narrow/ },
    { name: 'iphone-15', use: { ...devices['iPhone 15'] }, grep: /@narrow/ },
  ],
});
