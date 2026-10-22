import { defineConfig } from '@playwright/test';

const port = Number(process.env.NEOHAR_TRANSPORT_PORT ?? 4175);

export default defineConfig({
  testDir: '.',
  testMatch: '**/*.pw.ts',
  timeout: 60_000,
  workers: 1,
  fullyParallel: false,
  reporter: [['list']],
  outputDir: '../../target/transport-results',
  use: { baseURL: `http://127.0.0.1:${port}` },
  webServer: {
    command: 'bun serve.ts',
    url: `http://127.0.0.1:${port}/ready`,
    // A server from an earlier run would serve old pages and an old core.
    reuseExistingServer: false,
    // The server stops the programs of the core when it gets this signal. The default kills it.
    gracefulShutdown: { signal: 'SIGTERM', timeout: 30_000 },
    timeout: 600_000,
    stdout: 'pipe',
    stderr: 'pipe',
  },
  projects: [{ name: 'webkit', use: { browserName: 'webkit' } }],
});
