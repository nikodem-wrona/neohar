// Starts a headless core and the page of the UI for the Playwright spec.
//
// Run it with `bun tests/ui/serve.ts`. The Playwright config starts it. It starts the core
// (`tests/support/daemon.ts`), builds the page with Vite into `target/ui-web` (the shell is
// React, which Vite compiles), and serves the page and `/info` (where the core listens).
//
// `NEOHAR_UI_LINK=1` puts the core behind `link.sock`: the page then talks to a client of
// `link/1`. `mise run test:shell:link` sets it.

import { join } from 'node:path';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { build } from 'vite';
import { root, startDaemon } from '../support/daemon';

// `NEOHAR_UI_DIST` gives another folder, so that two runs on two ports do not build into one.
const dist = process.env.NEOHAR_UI_DIST ?? join(root, 'target', 'ui-web');
const port = Number(process.env.NEOHAR_UI_PORT ?? 4176);

await build({
  root: import.meta.dir,
  plugins: [react(), tailwindcss()],
  resolve: { alias: { '@': join(root, 'packages', 'ui', 'src') } },
  css: { transformer: 'lightningcss', lightningcss: { targets: { safari: 17 << 16 } } },
  logLevel: 'warn',
  build: {
    outDir: dist,
    emptyOutDir: true,
    target: 'safari17',
    sourcemap: 'inline',
    rollupOptions: { input: join(import.meta.dir, 'page.html') },
  },
});

const daemon = await startDaemon('nh-ui', process.env.NEOHAR_UI_LINK === '1');
const { info, nh, configDir, tailscaleDir } = daemon;

const server = Bun.serve({
  port,
  hostname: '127.0.0.1',
  async fetch(request) {
    const url = new URL(request.url);
    if (url.pathname === '/ready') return new Response('ok');
    if (url.pathname === '/info') {
      return Response.json({ ...info, nh, config_dir: configDir, tailscale_dir: tailscaleDir });
    }
    const file = Bun.file(join(dist, url.pathname === '/' ? 'page.html' : url.pathname));
    return (await file.exists()) ? new Response(file) : new Response('not found', { status: 404 });
  },
});
console.log(`ui test server on ${server.url}`);

let ending = false;
async function end(): Promise<void> {
  if (ending) return;
  ending = true;
  server.stop(true);
  await daemon.stop();
  process.exit(0);
}
for (const signal of ['SIGINT', 'SIGTERM'] as const) process.on(signal, () => void end());
